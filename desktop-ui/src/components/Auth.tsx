import { useState } from 'react';
import logo from '../assets/logo.png';
import { initOlm, generateOlmAccount, generateOneTimeKeys } from '../utils/crypto';

const API_BASE = import.meta.env.VITE_MCP_URL || 'http://localhost:8081';

interface AuthScreenProps {
  onAuthenticated: (token: string, refreshToken: string, subStatus?: string) => void;
}

export const AuthScreen = ({ onAuthenticated }: AuthScreenProps) => {
  const [password, setPassword] = useState('');
  
  const handleLogin = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      const response = await fetch(`${API_BASE}/admin/api/auth`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ password })
      });
      if (response.ok) {
        const data = await response.json();
        
        try {
          await initOlm();
          const account = generateOlmAccount();
          const keysStr = generateOneTimeKeys(account, 50);
          const keys = JSON.parse(keysStr);

          // Persist the Olm account so we can decrypt incoming messages later
          const encoder = new TextEncoder();
          const keyBuffer = await crypto.subtle.digest('SHA-256', encoder.encode(password));
          const pickledAccount = account.pickle(new Uint8Array(keyBuffer));
          localStorage.setItem('baton_olm_account', pickledAccount);
          
          await fetch(`${API_BASE}/keys/upload`, {
            method: 'POST',
            headers: { 
              'Content-Type': 'application/json',
              'Authorization': `Bearer ${data.token}`
            },
            body: JSON.stringify({ keys })
          });
        } catch (err) {
          console.error('Failed to upload keys', err);
        }

        onAuthenticated(data.token, data.refreshToken, data.subscriptionStatus);
      } else {
        alert('Invalid password');
      }
    } catch (error) {
      console.error('Login failed', error);
      alert('Could not connect to backend');
    }
  };

  return (
    <div className="layout" style={{ justifyContent: 'center', alignItems: 'center' }}>
      <div className="lock-screen" style={{ width: '400px', padding: '3rem' }}>
        <img src={logo} alt="Logo" style={{ width: '60px', marginBottom: '1.5rem', filter: 'grayscale(100%) contrast(1.2)' }} />
        <h2 style={{ marginBottom: '1.5rem' }}>Admin Login</h2>
        <form onSubmit={handleLogin} style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
          <input 
            type="password" 
            value={password} 
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Enter admin password"
            style={{ padding: '0.8rem', borderRadius: '8px', border: '1px solid var(--border)', background: 'var(--surface)' }}
          />
          <button type="submit" className="btn-approve" style={{ padding: '0.8rem' }}>Login</button>
        </form>
      </div>
    </div>
  );
};
