import { toast } from 'sonner';
import { useAppContext } from '../../contexts/AppContext';
import { basicAuthHeader } from '../../lib/utils';
import { ShieldCheck, ShieldAlert } from 'lucide-react';

const API_BASE = import.meta.env.VITE_MCP_URL || 'http://localhost:8081';

export const PermissionsSettings = () => {
  const { permissions, loadData, adminPassword } = useAppContext();

  const handleTogglePermission = async (key: string, currentValue: boolean) => {
    try {
      const res = await fetch(`${API_BASE}/api/permissions/${encodeURIComponent(key)}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Authorization': basicAuthHeader(adminPassword) },
        body: JSON.stringify({ value: !currentValue })
      });
      if (res.ok) {
        await loadData();
      } else {
        toast.error('Failed to update permission');
      }
    } catch (err) {
      console.error(err);
      toast.error('Network error updating permission');
    }
  };

  return (
    <>
      <h1>MCP Permissions</h1>
      <p className="subtitle">Manage local AI capabilities</p>
      <div className="settings-panel">
        {['fs_read', 'fs_write', 'shell', 'web_search'].map(key => {
          const p = permissions.find(x => x.key === key);
          const isChecked = p ? p.value : false;
          return (
            <div className="setting-row" key={key}>
              <div>
                <label style={{display: 'flex', alignItems: 'center', gap: '0.5rem'}}>
                  {key === 'fs_write' || key === 'shell' ? <ShieldAlert size={16} color="var(--danger)" /> : <ShieldCheck size={16} color="var(--success)" />}
                  {key === 'fs_read' ? 'Filesystem Read Access' :
                   key === 'fs_write' ? 'Filesystem Write Access' :
                   key === 'shell' ? 'Terminal / Shell Execution' :
                   'Web Search'}
                </label>
                <span className="settings-hint" style={{ display: 'block', marginLeft: '1.5rem' }}>
                  {key === 'fs_read' ? 'Allow AI to read local files' :
                   key === 'fs_write' ? 'Allow AI to modify local files (High Risk)' :
                   key === 'shell' ? 'Allow AI to run system commands (High Risk)' :
                   'Allow AI to fetch external data'}
                </span>
              </div>
              <input 
                type="checkbox" 
                checked={isChecked} 
                onChange={() => handleTogglePermission(key, isChecked)} 
                className="toggle-switch" 
              />
            </div>
          );
        })}
      </div>
    </>
  );
};
