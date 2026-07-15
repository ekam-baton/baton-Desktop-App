import { useState, useEffect } from "react";
import "./App.css";
import logo from "./assets/logo.png";

interface Agent {
  client_id: string;
  device_name: string;
}

function App() {
  const [activeTab, setActiveTab] = useState<'pending' | 'authorized' | 'settings'>('pending');
  const [requests, setRequests] = useState<Agent[]>([]);
  const [authorized, setAuthorized] = useState<Agent[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchPending = async () => {
    try {
      const response = await fetch('http://127.0.0.1:8081/admin/api/pending');
      if (response.ok) {
        setRequests(await response.json());
      }
    } catch (err) {
      console.error(err);
    }
  };

  const fetchAuthorized = async () => {
    try {
      const response = await fetch('http://127.0.0.1:8081/admin/api/authorized');
      if (response.ok) {
        setAuthorized(await response.json());
      }
    } catch (err) {
      console.error(err);
    }
  };

  const loadData = async () => {
    setLoading(true);
    await Promise.all([fetchPending(), fetchAuthorized()]);
    setLoading(false);
  }

  useEffect(() => {
    loadData();
    const interval = setInterval(() => {
      fetchPending();
      fetchAuthorized();
    }, 5000);
    return () => clearInterval(interval);
  }, []);

  const handleAction = async (clientId: string, action: 'approve' | 'deny' | 'revoke') => {
    try {
      const res = await fetch(`http://127.0.0.1:8081/admin/api/${action}/${encodeURIComponent(clientId)}`, { 
        method: 'POST' 
      });
      if (res.ok) {
        loadData();
      } else {
        alert('Action failed: ' + res.status);
      }
    } catch (err) {
      console.error(err);
    }
  };

  return (
    <div className="layout">
      <aside className="sidebar">
        <div className="sidebar-logo">
          <img src={logo} alt="Baton Logo" className="logo-image" />
          <h2>Baton</h2>
        </div>
        <nav className="sidebar-nav">
          <button 
            className={`nav-item ${activeTab === 'pending' ? 'active' : ''}`}
            onClick={() => setActiveTab('pending')}
          >
            Pending Requests {requests.length > 0 && <span className="badge">{requests.length}</span>}
          </button>
          <button 
            className={`nav-item ${activeTab === 'authorized' ? 'active' : ''}`}
            onClick={() => setActiveTab('authorized')}
          >
            Authorized Agents
          </button>
          <button 
            className={`nav-item ${activeTab === 'settings' ? 'active' : ''}`}
            onClick={() => setActiveTab('settings')}
          >
            Settings
          </button>
        </nav>
      </aside>
      
      <main className="main-content">
        <div className="container">
          {activeTab === 'pending' && (
            <>
              <h1>Pending Access</h1>
              <p className="subtitle">Review new agent connections</p>
              
              <div className="list-container">
                {loading && requests.length === 0 ? (
                  <div className="empty-state">Loading pending requests...</div>
                ) : requests.length === 0 ? (
                  <div className="empty-state">No pending authorization requests.</div>
                ) : (
                  requests.map((req) => (
                    <div key={req.client_id} className="request-card">
                      <div className="info">
                        <div className="client-name">{req.device_name || 'Unknown Device'}</div>
                        <div className="client-pubkey">ID: {req.client_id.substring(0, 16)}...</div>
                      </div>
                      <div className="actions">
                        <button 
                          className="btn-approve"
                          onClick={() => handleAction(req.client_id, 'approve')}
                        >
                          Approve
                        </button>
                        <button 
                          className="btn-deny"
                          onClick={() => handleAction(req.client_id, 'deny')}
                        >
                          Deny
                        </button>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </>
          )}

          {activeTab === 'authorized' && (
            <>
              <h1>Authorized Agents</h1>
              <p className="subtitle">Manage active connections</p>
              
              <div className="list-container">
                {loading && authorized.length === 0 ? (
                  <div className="empty-state">Loading authorized agents...</div>
                ) : authorized.length === 0 ? (
                  <div className="empty-state">No authorized agents found.</div>
                ) : (
                  authorized.map((req) => (
                    <div key={req.client_id} className="request-card">
                      <div className="info">
                        <div className="client-name">{req.device_name || 'Unknown Device'}</div>
                        <div className="client-pubkey">ID: {req.client_id.substring(0, 16)}...</div>
                      </div>
                      <div className="actions">
                        <button 
                          className="btn-deny"
                          onClick={() => handleAction(req.client_id, 'revoke')}
                        >
                          Revoke Access
                        </button>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </>
          )}

          {activeTab === 'settings' && (
            <>
              <h1>Settings</h1>
              <p className="subtitle">Application Configuration</p>
              <div className="settings-panel">
                <div className="setting-row">
                  <label>Connector Port</label>
                  <input type="number" defaultValue="8081" disabled className="text-input" />
                </div>
                <div className="setting-row">
                  <label>Enable mDNS Discovery</label>
                  <input type="checkbox" defaultChecked disabled />
                </div>
                <p className="settings-hint">Configuration is currently managed via .env files.</p>
              </div>
            </>
          )}
        </div>
      </main>
    </div>
  );
}

export default App;
