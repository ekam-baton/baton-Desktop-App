import { useState, useEffect } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { toast } from 'sonner';

export const TunnelSettings = () => {
  const [envConfig, setEnvConfig] = useState<Record<string, string>>({});

  useEffect(() => {
    invoke<Record<string, string>>('get_env')
      .then(res => setEnvConfig(res))
      .catch(err => console.error("Error fetching env:", err));
  }, []);

  const handleSave = async () => {
    try {
      await invoke('set_env', { updates: envConfig });
      toast.success("Tunnel configuration saved! Please restart the app.", { duration: 8000 });
    } catch(err) {
      toast.error("Failed to save configuration");
    }
  };

  return (
    <>
      <h1>Network Tunnel</h1>
      <p className="subtitle">Remote access configuration via Baton Cloud Router</p>
      
      <div className="settings-panel">
        <div className="setting-row">
          <div>
            <label style={{display: 'block'}}>Enable Remote Tunnel</label>
            <span className="settings-hint">Allow secure access when away from home</span>
          </div>
          <input 
            type="checkbox" 
            checked={envConfig['ENABLE_TUNNEL'] === 'true'}
            onChange={(e) => setEnvConfig({...envConfig, 'ENABLE_TUNNEL': e.target.checked ? 'true' : 'false'})}
            className="toggle-switch" 
          />
        </div>
        
        {envConfig['ENABLE_TUNNEL'] === 'true' && (
          <div className="setting-row" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: '1rem' }}>
            <label style={{ display: 'block' }}>Cloud Router URL</label>
            <input 
              type="text" 
              placeholder="wss://your-router-url.com"
              value={envConfig['A2A_ROUTER_URL'] || ''}
              onChange={(e) => setEnvConfig({...envConfig, 'A2A_ROUTER_URL': e.target.value})}
              className="input-field"
              style={{ width: '100%' }}
            />
          </div>
        )}

        <button 
          className="btn-approve" 
          style={{ marginTop: '1.5rem', width: '100%', padding: '0.8rem' }}
          onClick={handleSave}
        >
          Save Configuration
        </button>
      </div>
    </>
  );
};
