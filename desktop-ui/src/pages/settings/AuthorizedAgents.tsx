import { toast } from 'sonner';
import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';
import { useAppContext } from '../../contexts/AppContext';

export const AuthorizedAgents = () => {
  const { authorized, loading, loadData, adminPassword } = useAppContext();

  const handleAction = async (clientId: string, action: 'revoke') => {
    try {
      const res = await fetch(`http://localhost:8081/admin/api/${action}/${encodeURIComponent(clientId)}`, { 
        method: 'POST', headers: { 'Authorization': 'Basic ' + btoa('admin:' + adminPassword) } 
      });
      if (res.ok) {
        loadData();
      } else {
        toast.error('Action failed: ' + res.status);
      }
    } catch (err) {
      console.error(err);
      toast.error('Network error during action');
    }
  };

  return (
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
                <div className="telemetry-bar">
                  <span title="Battery Level">🔋 87%</span>
                  <span title="Connection Status">📶 5G UW</span>
                  <span title="Active Link">☁️ Cloud Router</span>
                </div>
              </div>
              <div className="actions">
                <button 
                  className="btn-approve"
                  onClick={async () => {
                    try {
                      const res = await fetch(`http://localhost:8081/admin/api/safety-number/${encodeURIComponent(req.client_id)}`, {
                        headers: { 'Authorization': 'Basic ' + btoa('admin:' + adminPassword) }
                      });
                      if (res.ok) {
                        const data = await res.json();
                        toast(`Safety Number for ${req.device_name || 'Device'}: ${data.safety_number}`, { duration: 10000 });
                      } else {
                        toast.error('Could not compute Safety Number (missing identity keys?)');
                      }
                    } catch (e) {
                      toast.error('Network error fetching Safety Number');
                    }
                  }}
                >
                  Verify Identity
                </button>
                <button 
                  className="btn-approve"
                  onClick={async () => {
                    let permissionGranted = await isPermissionGranted();
                    if (!permissionGranted) {
                      const permission = await requestPermission();
                      permissionGranted = permission === 'granted';
                    }
                    if (permissionGranted) {
                      sendNotification({ title: 'Baton', body: 'Incoming WebRTC stream initialized. Check terminal for audio pipe.' });
                    } else {
                      toast.error('Permission denied. Cannot start audio stream.');
                    }
                  }}
                >
                  Answer Call
                </button>
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
  );
};
