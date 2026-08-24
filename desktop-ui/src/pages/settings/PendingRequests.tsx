import { toast } from 'sonner';
import { useAppContext } from '../../contexts/AppContext';

export const PendingRequests = () => {
  const { requests, loading, loadData, adminPassword } = useAppContext();

  const handleAction = async (clientId: string, action: 'approve' | 'deny') => {
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
                <button className="btn-approve" onClick={() => handleAction(req.client_id, 'approve')}>Approve</button>
                <button className="btn-deny" onClick={() => handleAction(req.client_id, 'deny')}>Deny</button>
              </div>
            </div>
          ))
        )}
      </div>
    </>
  );
};
