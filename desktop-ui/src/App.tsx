import { useState, useEffect } from "react";
import "./App.css";

interface PendingRequest {
  client_id: string;
  device_name: string;
  public_key_hex: string;
  requested_at: string;
}

function App() {
  const [requests, setRequests] = useState<PendingRequest[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchPending = async () => {
    try {
      // In Tauri, we can fetch the local axum server directly
      const response = await fetch('http://127.0.0.1:8081/admin/api/pending');
      if (response.ok) {
        const data = await response.json();
        setRequests(data);
      } else if (response.status === 401) {
        // Handle basic auth prompt or unauthorized state
        console.error("Unauthorized. Check basic auth credentials.");
      }
    } catch (err) {
      console.error('Error fetching pending requests', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchPending();
    // Poll every 5 seconds for new requests
    const interval = setInterval(fetchPending, 5000);
    return () => clearInterval(interval);
  }, []);

  const handleAction = async (clientId: string, action: 'approve' | 'deny') => {
    try {
      const res = await fetch(`http://127.0.0.1:8081/admin/api/${action}/${encodeURIComponent(clientId)}`, { 
        method: 'POST' 
      });
      if (res.ok) {
        fetchPending();
      } else {
        alert('Action failed: ' + res.status);
      }
    } catch (err) {
      console.error(err);
    }
  };

  return (
    <main className="container">
      <h1>Baton Connector</h1>
      <p className="subtitle">Agent Access Control Dashboard</p>
      
      <div className="list-container">
        {loading ? (
          <div className="empty-state">Loading pending requests...</div>
        ) : requests.length === 0 ? (
          <div className="empty-state">No pending authorization requests.</div>
        ) : (
          requests.map((req) => (
            <div key={req.client_id} className="request-card">
              <div className="info">
                <div className="client-name">{req.device_name || 'Unknown Device'}</div>
                <div className="client-pubkey">Key: {req.public_key_hex.substring(0, 16)}...</div>
                <div className="request-time">Time: {new Date(req.requested_at).toLocaleString()}</div>
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
    </main>
  );
}

export default App;
