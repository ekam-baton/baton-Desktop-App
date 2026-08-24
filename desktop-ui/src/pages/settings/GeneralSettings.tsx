import { useState, useEffect } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { enable, disable } from '@tauri-apps/plugin-autostart';
import { toast } from 'sonner';

export const GeneralSettings = () => {
  const [envConfig, setEnvConfig] = useState<Record<string, string>>({});
  const [backupPassword, setBackupPassword] = useState('');

  useEffect(() => {
    invoke<Record<string, string>>('get_env')
      .then(res => setEnvConfig(res))
      .catch(err => console.error("Error fetching env:", err));
  }, []);

  const handleCloudBackup = async () => {
    if (!backupPassword) {
      toast.error('Please enter a password for the backup');
      return;
    }
    try {
      toast.error('Cloud backup requires database export. Feature coming soon.');
      return;
    } catch (err) {
      console.error(err);
      toast.error('An error occurred during backup');
    }
  };

  return (
    <>
      <h1>Settings</h1>
      <p className="subtitle">Application Configuration</p>
      <div className="settings-panel">
        <div className="setting-row">
          <div>
            <label style={{display: 'block'}}>Voice Pipeline Mode</label>
            <span className="settings-hint">Route WebRTC to Local STT or stream binary to Agent</span>
          </div>
          <select 
            style={{ width: '220px' }}
            value={envConfig['VOICE_PIPELINE'] || 'stt'}
            onChange={(e) => setEnvConfig({...envConfig, 'VOICE_PIPELINE': e.target.value})}
          >
            <option value="stt">Local Compute (STT/TTS)</option>
            <option value="agent">Agent Compute (Raw Audio)</option>
          </select>
        </div>
        <div className="setting-row glass">
          <label>Launch on Boot</label>
          <input
            type="checkbox"
            onChange={async (e) => {
              if (e.target.checked) await enable();
              else await disable();
            }}
            className="toggle-switch"
          />
        </div>
        <div className="setting-row">
          <label>Enable mDNS Discovery</label>
          <input 
            type="checkbox" 
            checked={envConfig['ENABLE_MDNS'] !== 'false'}
            onChange={(e) => setEnvConfig({...envConfig, 'ENABLE_MDNS': e.target.checked ? 'true' : 'false'})}
            className="toggle-switch" 
          />
        </div>
        
        <button 
          className="btn-approve" 
          style={{ marginTop: '1.5rem', width: '100%', padding: '0.8rem' }}
          onClick={async () => {
            try {
              await invoke('set_env', { updates: envConfig });
              toast.success("Configuration saved! Please restart the app for changes to take effect.", { duration: 8000 });
            } catch(err) {
              toast.error("Failed to save configuration");
            }
          }}
        >
          Save Configuration
        </button>

        <div style={{ marginTop: '2rem', paddingTop: '2rem', borderTop: '1px solid var(--border)' }}>
          <h3>Cloud Backup</h3>
          <p className="settings-hint" style={{ marginBottom: '1rem' }}>Encrypt and backup your local database to the cloud.</p>
          <div style={{ display: 'flex', gap: '1rem' }}>
            <input 
              type="password" 
              placeholder="Backup Password" 
              value={backupPassword}
              onChange={e => setBackupPassword(e.target.value)}
              style={{ flex: 1, padding: '0.8rem', borderRadius: '8px', border: '1px solid var(--border)', background: 'var(--surface)' }}
            />
            <button className="btn-approve" onClick={handleCloudBackup}>
              Backup to Cloud
            </button>
          </div>
        </div>
      </div>
    </>
  );
};
