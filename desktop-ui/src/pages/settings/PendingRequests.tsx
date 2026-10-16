import { useState } from 'react';
import { toast } from 'sonner';
import { useAppContext } from '../../contexts/AppContext';
import { basicAuthHeader } from '../../lib/utils';

const API_BASE = import.meta.env.VITE_MCP_URL || 'http://localhost:8081';

export const PendingRequests = () => {
  const { requests, loading, loadData, adminPassword } = useAppContext();
  const [processingId, setProcessingId] = useState<string | null>(null);

  const handleAction = async (clientId: string, action: 'approve' | 'deny') => {
    if (processingId) return;
    setProcessingId(clientId);
    try {
      const res = await fetch(`${API_BASE}/admin/api/${action}/${encodeURIComponent(clientId)}`, { 
        method: 'POST', headers: { 'Authorization': basicAuthHeader(adminPassword) } 
      });
      if (res.ok) {
        await loadData();
      } else {
        toast.error('Action failed: ' + res.status);
      }
    } catch (err) {
      console.error(err);
      toast.error('Network error during action');
    } finally {
      setProcessingId(null);
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
                <div className="client-pubkey">ID: {req.client_id ? `${req.client_id.slice(0, 16)}...` : 'Unknown'}</div>
              </div>
              <div className="actions">
                <button 
                  className="btn-approve"
                  disabled={processingId === req.client_id}
                  onClick={async () => {
                    try {
                      const res = await fetch(`${API_BASE}/admin/api/safety-number/${encodeURIComponent(req.client_id)}`, {
                        headers: { 'Authorization': basicAuthHeader(adminPassword) }
                      });
                      if (res.ok) {
                        const data = await res.json();
                        toast(`Safety Number for ${req.device_name || 'Device'}: ${data.safety_number}`, { duration: 10000 });
                      } else {
                        toast.error('Could not compute Safety Number (missing identity keys?)');
                      }
                    } catch {
                      toast.error('Network error fetching Safety Number');
                    }
                  }}
                >
                  Verify Identity
                </button>
                <button 
                  className="btn-approve" 
                  disabled={processingId === req.client_id}
                  onClick={() => handleAction(req.client_id, 'approve')}
                >
                  Approve
                </button>
                <button 
                  className="btn-deny" 
                  disabled={processingId === req.client_id}
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
  );
};
