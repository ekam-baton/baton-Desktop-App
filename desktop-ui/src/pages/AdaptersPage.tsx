import React, { useState, useEffect } from 'react';
import { Plus, Server, Activity, Trash2, X } from 'lucide-react';
import { toast } from 'sonner';

interface Adapter {
  id: string;
  name: string;
  url: string;
}

export const AdaptersPage: React.FC = () => {
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [newAdapterUrl, setNewAdapterUrl] = useState('');
  const [newAdapterName, setNewAdapterName] = useState('');
  const [selectedAdapter, setSelectedAdapter] = useState<Adapter | null>(null);
  const [activeApiKey, setActiveApiKey] = useState<string>('');
  
  const [promptModalOpen, setPromptModalOpen] = useState(false);
  const [promptInput, setPromptInput] = useState('');
  const [pendingAdapter, setPendingAdapter] = useState<Adapter | null>(null);

  const [statusInfo, setStatusInfo] = useState<any>(null);
  const [statusError, setStatusError] = useState<boolean>(false);
  const [logs, setLogs] = useState<string[]>([]);
  
  useEffect(() => {
    const saved = localStorage.getItem('baton_adapters');
    if (saved) {
      try {
        setAdapters(JSON.parse(saved));
      } catch (e) {
        console.error('Failed to parse adapters', e);
      }
    }
  }, []);

  const saveAdapters = (updated: Adapter[]) => {
    setAdapters(updated);
    localStorage.setItem('baton_adapters', JSON.stringify(updated));
  };

  const handleAddAdapter = () => {
    if (!newAdapterUrl || !newAdapterName) {
      toast.error('Please fill all fields');
      return;
    }
    const newAdapter: Adapter = {
      id: Date.now().toString(),
      name: newAdapterName,
      url: newAdapterUrl,
    };
    saveAdapters([...adapters, newAdapter]);
    setIsModalOpen(false);
    setNewAdapterName('');
    setNewAdapterUrl('');
    toast.success('Adapter linked successfully');
  };

  const handleDeleteAdapter = (id: string) => {
    saveAdapters(adapters.filter(a => a.id !== id));
    if (selectedAdapter?.id === id) {
      setSelectedAdapter(null);
      setActiveApiKey('');
    }
    toast.success('Adapter removed');
  };

  const handleSelectAdapter = (adapter: Adapter) => {
    setPendingAdapter(adapter);
    setPromptInput('');
    setPromptModalOpen(true);
  };

  const handlePromptSubmit = () => {
    if (pendingAdapter && promptInput) {
      setSelectedAdapter(pendingAdapter);
      setActiveApiKey(promptInput);
    }
    setPromptModalOpen(false);
  };

  useEffect(() => {
    if (!selectedAdapter || !activeApiKey) return;
    
    setStatusInfo(null);
    setStatusError(false);

    fetch(selectedAdapter.url + '/mgmt/status', {
      headers: { 'Mgmt-Api-Key': activeApiKey }
    })
      .then(res => {
        if (res.status === 501) return { status: 'not_available' };
        if (!res.ok) throw new Error('Failed to fetch status');
        return res.json();
      })
      .then(data => setStatusInfo(data))
      .catch(() => setStatusError(true));
  }, [selectedAdapter, activeApiKey]);

  useEffect(() => {
    if (!selectedAdapter || !activeApiKey) return;
    
    setLogs(['Connecting to management API...', 'Connected to ' + selectedAdapter.url]);
    
    const interval = setInterval(() => {
      fetch(selectedAdapter.url + '/mgmt/logs', {
        headers: { 'Mgmt-Api-Key': activeApiKey }
      })
      .then(res => {
         if (!res.ok) throw new Error('Fetch failed');
         return res.json();
      })
      .then(data => {
         if (Array.isArray(data)) {
           setLogs(data.map(d => typeof d === 'string' ? d : JSON.stringify(d)));
         } else {
           setLogs([JSON.stringify(data)]);
         }
      })
      .catch(() => {
         setLogs(prev => [...prev.slice(-19), `[${new Date().toLocaleTimeString()}] Connection error`]);
      });
    }, 3000);
    
    return () => clearInterval(interval);
  }, [selectedAdapter, activeApiKey]);

  return (
    <div className="page-container" style={{ padding: '20px', height: '100%', display: 'flex', flexDirection: 'column' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '20px' }}>
        <h2><Server size={24} style={{ marginRight: '10px', verticalAlign: 'middle' }} /> Adapter Management</h2>
        <button className="btn-primary" onClick={() => setIsModalOpen(true)} style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <Plus size={16} /> Link Adapter
        </button>
      </div>

      <div style={{ display: 'flex', gap: '20px', flex: 1, minHeight: 0 }}>
        {/* Adapters List */}
        <div style={{ width: '300px', borderRight: '1px solid var(--border-color)', paddingRight: '20px', overflowY: 'auto' }}>
          {adapters.length === 0 ? (
            <div style={{ color: 'var(--text-muted)' }}>No adapters linked.</div>
          ) : (
            adapters.map(adapter => (
              <div 
                key={adapter.id}
                onClick={() => handleSelectAdapter(adapter)}
                style={{
                  padding: '12px',
                  borderRadius: '8px',
                  marginBottom: '10px',
                  cursor: 'pointer',
                  backgroundColor: selectedAdapter?.id === adapter.id ? 'var(--bg-secondary)' : 'transparent',
                  border: '1px solid var(--border-color)',
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center'
                }}
              >
                <div>
                  <div style={{ fontWeight: 'bold' }}>{adapter.name}</div>
                  <div style={{ fontSize: '12px', color: 'var(--text-muted)' }}>{adapter.url}</div>
                </div>
                <button 
                  onClick={(e) => { e.stopPropagation(); handleDeleteAdapter(adapter.id); }}
                  style={{ background: 'none', border: 'none', color: 'var(--danger)', cursor: 'pointer' }}
                >
                  <Trash2 size={16} />
                </button>
              </div>
            ))
          )}
        </div>

        {/* Dashboard View */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflowY: 'auto' }}>
          {selectedAdapter ? (
            <>
              <div style={{ display: 'flex', gap: '20px', marginBottom: '20px' }}>
                <div style={{ flex: 1, padding: '20px', borderRadius: '8px', border: '1px solid var(--border-color)', backgroundColor: 'var(--bg-secondary)' }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: '10px', marginBottom: '10px' }}>
                    <Activity size={20} color="var(--primary)" />
                    <h3 style={{ margin: 0 }}>Status</h3>
                  </div>
                  {statusError ? (
                    <div style={{ color: 'var(--danger)', fontWeight: 'bold' }}>Offline</div>
                  ) : statusInfo?.status === 'not_available' ? (
                    <div style={{ color: '#ffaa00', fontWeight: 'bold', padding: '4px 8px', backgroundColor: 'rgba(255, 170, 0, 0.1)', borderRadius: '4px', display: 'inline-block' }}>Coming Soon</div>
                  ) : statusInfo ? (
                    <pre style={{ margin: 0, fontSize: '12px', overflowX: 'auto', color: 'var(--success)' }}>{JSON.stringify(statusInfo, null, 2)}</pre>
                  ) : (
                    <div style={{ color: 'var(--text-muted)' }}>Loading...</div>
                  )}
                </div>
              </div>
              
              <div style={{ flex: 1, padding: '20px', borderRadius: '8px', border: '1px solid var(--border-color)', display: 'flex', flexDirection: 'column' }}>
                <h3 style={{ marginTop: 0 }}>Live Logs</h3>
                <div style={{ 
                  flex: 1, 
                  backgroundColor: '#000', 
                  color: '#0f0', 
                  padding: '10px', 
                  borderRadius: '4px', 
                  fontFamily: 'monospace',
                  overflowY: 'auto',
                  fontSize: '12px'
                }}>
                  {logs.map((log, i) => (
                    <div key={i}>{log}</div>
                  ))}
                </div>
              </div>
            </>
          ) : (
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', color: 'var(--text-muted)' }}>
              Select an adapter to view details
            </div>
          )}
        </div>
      </div>

      {/* Modal */}
      {isModalOpen && (
        <div className="modal-overlay" style={{
          position: 'fixed', top: 0, left: 0, right: 0, bottom: 0,
          backgroundColor: 'rgba(0,0,0,0.5)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000
        }}>
          <div className="modal-content" style={{
            backgroundColor: 'var(--bg-primary)', padding: '24px', borderRadius: '8px', width: '400px', border: '1px solid var(--border-color)'
          }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '20px' }}>
              <h3 style={{ margin: 0 }}>Link New Adapter</h3>
              <button onClick={() => setIsModalOpen(false)} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-primary)' }}>
                <X size={20} />
              </button>
            </div>
            
            <div className="form-group" style={{ marginBottom: '16px' }}>
              <label style={{ display: 'block', marginBottom: '8px' }}>Name</label>
              <input 
                type="text" 
                value={newAdapterName} 
                onChange={e => setNewAdapterName(e.target.value)} 
                placeholder="Local Server"
                style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid var(--border-color)', backgroundColor: 'var(--bg-secondary)', color: 'var(--text-primary)' }}
              />
            </div>
            <div className="form-group" style={{ marginBottom: '16px' }}>
              <label style={{ display: 'block', marginBottom: '8px' }}>Adapter URL</label>
              <input 
                type="text" 
                value={newAdapterUrl} 
                onChange={e => setNewAdapterUrl(e.target.value)} 
                placeholder="http://localhost:8080"
                style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid var(--border-color)', backgroundColor: 'var(--bg-secondary)', color: 'var(--text-primary)' }}
              />
            </div>
            <div className="form-group" style={{ marginBottom: '24px' }}>
              <p style={{ color: 'var(--text-muted)', fontSize: '12px', margin: 0 }}>API Key is not stored for security. You will need to re-enter it each session.</p>
            </div>
            
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '10px' }}>
              <button className="btn-secondary" onClick={() => setIsModalOpen(false)} style={{ padding: '8px 16px', borderRadius: '4px', cursor: 'pointer', border: '1px solid var(--border-color)', backgroundColor: 'transparent', color: 'var(--text-primary)' }}>Cancel</button>
              <button className="btn-primary" onClick={handleAddAdapter} style={{ padding: '8px 16px', borderRadius: '4px', cursor: 'pointer', border: 'none', backgroundColor: 'var(--primary)', color: 'white' }}>Link</button>
            </div>
          </div>
        </div>
      )}

      {/* Prompt Modal */}
      {promptModalOpen && pendingAdapter && (
        <div className="modal-overlay" style={{
          position: 'fixed', top: 0, left: 0, right: 0, bottom: 0,
          backgroundColor: 'rgba(0,0,0,0.5)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000
        }}>
          <div className="modal-content" style={{
            backgroundColor: 'var(--bg-primary)', padding: '24px', borderRadius: '8px', width: '400px', border: '1px solid var(--border-color)'
          }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '20px' }}>
              <h3 style={{ margin: 0 }}>Enter API Key</h3>
              <button onClick={() => setPromptModalOpen(false)} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-primary)' }}>
                <X size={20} />
              </button>
            </div>
            
            <div className="form-group" style={{ marginBottom: '24px' }}>
              <label style={{ display: 'block', marginBottom: '8px' }}>Management API Key for {pendingAdapter.name}:</label>
              <input 
                type="text" 
                value={promptInput} 
                onChange={e => setPromptInput(e.target.value)} 
                placeholder="Enter API Key"
                style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid var(--border-color)', backgroundColor: 'var(--bg-secondary)', color: 'var(--text-primary)' }}
              />
            </div>
            
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '10px' }}>
              <button className="btn-secondary" onClick={() => setPromptModalOpen(false)} style={{ padding: '8px 16px', borderRadius: '4px', cursor: 'pointer', border: '1px solid var(--border-color)', backgroundColor: 'transparent', color: 'var(--text-primary)' }}>Cancel</button>
              <button className="btn-primary" onClick={handlePromptSubmit} style={{ padding: '8px 16px', borderRadius: '4px', cursor: 'pointer', border: 'none', backgroundColor: 'var(--primary)', color: 'white' }}>Submit</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
