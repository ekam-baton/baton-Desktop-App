import { useState, useEffect, useRef } from "react";
import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';
import { enable, disable } from '@tauri-apps/plugin-autostart';
import { ShieldCheck, ShieldAlert, KeyRound, HardDrive, Calendar, Network, FileDown, Settings, ListTodo, Box, Folder } from 'lucide-react';
import "./App.css";
import logo from "./assets/logo.png";

interface Agent {
  client_id: string;
  device_name: string;
}

interface AuditLog { id: number, event_type: string, detail: string, severity: string, created_at: string }
interface ScheduledTask { id: number, task_name: string, cron_expression: string, created_at: string }
interface Permission { key: string, value: boolean }
interface InboxFile { id: number, file_name: string, sender: string, created_at: string }
interface KnowledgeDir { id: number, directory_path: string, file_count: number, created_at: string }

function App() {
  const [activeTab, setActiveTab] = useState<'pending' | 'authorized' | 'inbox' | 'knowledge' | 'schedule' | 'tunnel' | 'handoff' | 'settings' | 'audit' | 'permissions'>('pending');
  const [requests, setRequests] = useState<Agent[]>([]);
  const [authorized, setAuthorized] = useState<Agent[]>([]);
  const [loading, setLoading] = useState(true);
  const [tunnelUrl, setTunnelUrl] = useState("");

  const [auditLogs, setAuditLogs] = useState<AuditLog[]>([]);
  const [tasks, setTasks] = useState<ScheduledTask[]>([]);
  const [permissions, setPermissions] = useState<Permission[]>([]);
  const [inboxFiles, setInboxFiles] = useState<InboxFile[]>([]);
  const [knowledgeDirs, setKnowledgeDirs] = useState<KnowledgeDir[]>([]);

  const [newTaskName, setNewTaskName] = useState("");
  const [newCronExp, setNewCronExp] = useState("0 7 * * *");
  
  const [kbTitle, setKbTitle] = useState("");
  const [kbContent, setKbContent] = useState("");

  const previousPendingCount = useRef(0);
  const [isLocked, setIsLocked] = useState(true);
  const [pin, setPin] = useState("");
  const CORRECT_PIN = "1234";

  const loadData = async (signal?: AbortSignal) => {
    setLoading(true);
    try {
        await Promise.all([
            fetch('http://127.0.0.1:8081/admin/api/pending', { signal }).then(res => { 
                if (res.ok) return res.json().then(data => {
                    if (data.length > previousPendingCount.current) {
                        isPermissionGranted().then(granted => {
                            if (granted) sendNotification({ title: 'Baton', body: 'New agent requesting access!' });
                            else requestPermission().then(p => { if (p === 'granted') sendNotification({ title: 'Baton', body: 'New agent requesting access!' }); });
                        });
                    }
                    previousPendingCount.current = data.length;
                    setRequests(data);
                }); 
            }),
            fetch('http://127.0.0.1:8081/admin/api/authorized', { signal }).then(res => { if (res.ok) return res.json().then(setAuthorized); }),
            fetch('http://127.0.0.1:8081/api/audit', { signal }).then(res => { if (res.ok) return res.json().then(setAuditLogs); }),
            fetch('http://127.0.0.1:8081/api/schedule', { signal }).then(res => { if (res.ok) return res.json().then(setTasks); }),
            fetch('http://127.0.0.1:8081/api/permissions', { signal }).then(res => { if (res.ok) return res.json().then(setPermissions); }),
            fetch('http://127.0.0.1:8081/api/inbox', { signal }).then(res => { if (res.ok) return res.json().then(setInboxFiles); }),
            fetch('http://127.0.0.1:8081/api/knowledge', { signal }).then(res => { if (res.ok) return res.json().then(setKnowledgeDirs); })
        ]);
    } catch (err: unknown) {
        if (err instanceof Error && err.name !== 'AbortError') {
            console.error(err);
        }
    }
    setLoading(false);
  }

  useEffect(() => {
    const controller = new AbortController();
    loadData(controller.signal);
    
    const interval = setInterval(() => {
      loadData(controller.signal);
    }, 5000);
    
    return () => {
        clearInterval(interval);
        controller.abort();
    };
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

  const handleTogglePermission = async (key: string, currentValue: boolean) => {
    try {
      await fetch(`http://127.0.0.1:8081/api/permissions/${encodeURIComponent(key)}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ value: !currentValue })
      });
      loadData();
    } catch (err) {
      console.error(err);
    }
  };

  const handleCreateTask = async () => {
    if (!newTaskName || !newCronExp) return;
    try {
      await fetch('http://127.0.0.1:8081/api/schedule', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ task_name: newTaskName, cron_expression: newCronExp })
      });
      setNewTaskName("");
      loadData();
    } catch (err) {
      console.error(err);
    }
  };

  const handleDeleteTask = async (id: number) => {
    try {
      await fetch(`http://127.0.0.1:8081/api/schedule/${id}`, { method: 'DELETE' });
      loadData();
    } catch (err) {
      console.error(err);
    }
  }

  if (isLocked) {
    return (
      <div className="layout" style={{ justifyContent: 'center', alignItems: 'center' }}>
        <div className="lock-screen">
          <img src={logo} alt="Baton Logo" style={{ width: '60px', marginBottom: '1.5rem', filter: 'grayscale(100%) contrast(1.2)' }} />
          <h2 style={{ marginBottom: '2rem' }}>Authentication Required</h2>
          <div style={{ display: 'flex', gap: '1rem', justifyContent: 'center', marginBottom: '2.5rem' }}>
            {[0, 1, 2, 3].map(i => (
              <input
                key={i}
                type="password"
                maxLength={1}
                value={pin[i] || ""}
                readOnly
                className="lock-input"
              />
            ))}
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '0.5rem', maxWidth: '280px', margin: '0 auto' }}>
            {[1, 2, 3, 4, 5, 6, 7, 8, 9].map(num => (
              <button 
                key={num}
                className="keypad-btn"
                onClick={() => {
                  if (pin.length < 4) {
                    const newPin = pin + num.toString();
                    setPin(newPin);
                    if (newPin === CORRECT_PIN) setIsLocked(false);
                    else if (newPin.length === 4) { alert("Incorrect PIN"); setPin(""); }
                  }
                }}
              >
                {num}
              </button>
            ))}
            <div></div>
            <button 
              className="keypad-btn"
              onClick={() => {
                if (pin.length < 4) {
                  const newPin = pin + "0";
                  setPin(newPin);
                  if (newPin === CORRECT_PIN) setIsLocked(false);
                  else if (newPin.length === 4) { alert("Incorrect PIN"); setPin(""); }
                }
              }}
            >
              0
            </button>
            <button 
              className="keypad-btn keypad-btn-del"
              onClick={() => setPin(pin.slice(0, -1))}
            >
              Del
            </button>
          </div>
        </div>
      </div>
    );
  }

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
            <ShieldAlert size={18} /> Pending Requests
            {requests.length > 0 && <span className="badge">{requests.length}</span>}
          </button>
          <button 
            className={`nav-item ${activeTab === 'authorized' ? 'active' : ''}`}
            onClick={() => setActiveTab('authorized')}
          >
            <ShieldCheck size={18} /> Authorized Agents
          </button>
          <button 
            className={`nav-item ${activeTab === 'inbox' ? 'active' : ''}`}
            onClick={() => setActiveTab('inbox')}
          >
            <Box size={18} /> Inbox Sync
          </button>
          <button 
            className={`nav-item ${activeTab === 'knowledge' ? 'active' : ''}`}
            onClick={() => setActiveTab('knowledge')}
          >
            <HardDrive size={18} /> Knowledge Base
          </button>
          <button 
            className={`nav-item ${activeTab === 'schedule' ? 'active' : ''}`}
            onClick={() => setActiveTab('schedule')}
          >
            <Calendar size={18} /> Scheduled Tasks
          </button>
          <button 
            className={`nav-item ${activeTab === 'tunnel' ? 'active' : ''}`}
            onClick={() => setActiveTab('tunnel')}
          >
            <Network size={18} /> Network Tunnel
          </button>
          <button 
            className={`nav-item ${activeTab === 'handoff' ? 'active' : ''}`}
            onClick={() => setActiveTab('handoff')}
          >
            <FileDown size={18} /> Context Handoff
          </button>
          <button 
            className={`nav-item ${activeTab === 'permissions' ? 'active' : ''}`}
            onClick={() => setActiveTab('permissions')}
          >
            <KeyRound size={18} /> MCP Permissions
          </button>
          <button 
            className={`nav-item ${activeTab === 'audit' ? 'active' : ''}`}
            onClick={() => setActiveTab('audit')}
          >
            <ListTodo size={18} /> Activity Audit Log
          </button>
          <button 
            className={`nav-item ${activeTab === 'settings' ? 'active' : ''}`}
            onClick={() => setActiveTab('settings')}
          >
            <Settings size={18} /> Settings
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
                            let permissionGranted = await isPermissionGranted();
                            if (!permissionGranted) {
                              const permission = await requestPermission();
                              permissionGranted = permission === 'granted';
                            }
                            if (permissionGranted) {
                              sendNotification({ title: 'Baton', body: 'Incoming WebRTC stream initialized. Check terminal for audio pipe.' });
                            } else {
                              alert('Incoming WebRTC stream initialized. Check terminal for audio pipe.');
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
          )}

          {activeTab === 'settings' && (
            <>
              <h1>Settings</h1>
              <p className="subtitle">Application Configuration</p>
              <div className="settings-panel">
                <div className="setting-row">
                  <div>
                    <label style={{display: 'block'}}>Voice Pipeline Mode</label>
                    <span className="settings-hint">Route WebRTC to Local STT or stream binary to Agent</span>
                  </div>
                  <select style={{ width: '220px' }}>
                    <option value="stt">Local Compute (STT/TTS)</option>
                    <option value="agent">Agent Compute (Raw Audio)</option>
                  </select>
                </div>
                <div className="setting-row">
                  <label>Connector Port</label>
                  <input type="number" defaultValue="8081" disabled className="text-input" />
                </div>
                <div className="setting-row">
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
                  <input type="checkbox" defaultChecked disabled className="toggle-switch" />
                </div>
                <p className="settings-hint">Configuration is currently managed via .env files.</p>
              </div>
            </>
          )}

          {activeTab === 'handoff' && (
            <>
              <h1>Context Handoff</h1>
              <p className="subtitle">Send context to your phone before you leave</p>
              
              <div 
                className="drop-zone"
                onDragOver={(e) => { e.preventDefault(); e.currentTarget.classList.add('drag-active'); }}
                onDragLeave={(e) => { e.preventDefault(); e.currentTarget.classList.remove('drag-active'); }}
                onDrop={async (e) => {
                  e.preventDefault();
                  e.currentTarget.classList.remove('drag-active');
                  const files = Array.from(e.dataTransfer.files);
                  if (files.length > 0) {
                    const formData = new FormData();
                    files.forEach((file, index) => {
                      formData.append(`file_${index}`, file);
                    });
                    try {
                      const res = await fetch('http://127.0.0.1:8081/api/handoff', {
                        method: 'POST',
                        body: formData
                      });
                      if (res.ok) {
                        alert(`Handoff initiated for ${files.length} file(s). Context has been injected into the local agent's memory.`);
                        loadData();
                      } else {
                        alert('Handoff failed.');
                      }
                    } catch (err) {
                      console.error(err);
                      alert('Handoff network error.');
                    }
                  } else {
                    const text = e.dataTransfer.getData('text');
                    if (text) alert(`Text handoff initiated. Content sent to local agent.`);
                  }
                }}
              >
                <FileDown size={48} className="drop-icon" />
                <h3>Drop files or text here</h3>
                <p>Drag files, folders, or text snippets here. They will be seamlessly injected into your agent's memory, so your AI has full context when you access it from your phone.</p>
              </div>
            </>
          )}

          {activeTab === 'inbox' && (
            <>
              <h1>Inbox Sync</h1>
              <p className="subtitle">Files synced from your mobile agent</p>
              <div className="list-container">
                {loading && inboxFiles.length === 0 ? (
                   <div className="empty-state">Loading inbox...</div>
                ) : inboxFiles.length === 0 ? (
                   <div className="empty-state">Inbox is empty. Sync files from your mobile agent to see them here.</div>
                ) : (
                  inboxFiles.map(file => (
                    <div key={file.id} className="request-card">
                      <div className="info">
                        <div className="client-name">{file.file_name}</div>
                        <div className="client-pubkey">From: {file.sender} • {file.created_at}</div>
                      </div>
                      <div className="actions">
                        <button className="btn-approve">Open File</button>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </>
          )}

          {activeTab === 'knowledge' && (
            <>
              <h1>Local Knowledge Base</h1>
              <p className="subtitle">Directories indexed for RAG vector search</p>
              <div className="list-container">
                {loading && knowledgeDirs.length === 0 ? (
                  <div className="empty-state">Loading indices...</div>
                ) : knowledgeDirs.length === 0 ? (
                  <div className="empty-state">No directories indexed yet.</div>
                ) : (
                  knowledgeDirs.map(dir => (
                    <div key={dir.id} className="request-card">
                      <div className="info">
                        <div className="client-name">{dir.directory_path}</div>
                        <div className="client-pubkey">Indexed: {dir.file_count} files • Vectorized at {dir.created_at}</div>
                      </div>
                      <div className="actions">
                        <button className="btn-deny">Remove</button>
                      </div>
                    </div>
                  ))
                )}
              </div>
              
              <div className="kb-form">
                <h3 style={{ margin: 0, fontSize: '1.1rem' }}>Add Note to Vector Database</h3>
                <input 
                  type="text" 
                  placeholder="Document Title" 
                  className="text-input"
                  value={kbTitle}
                  onChange={(e) => setKbTitle(e.target.value)}
                />
                <textarea 
                  placeholder="Paste text content here..." 
                  className="text-input"
                  value={kbContent}
                  onChange={(e) => setKbContent(e.target.value)}
                />
                <button 
                  className="btn-approve" 
                  onClick={async () => {
                    if (!kbTitle || !kbContent) return;
                    try {
                      // Call the local MCP tool directly
                      const res = await fetch('http://127.0.0.1:8081/mcp', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({
                          jsonrpc: "2.0",
                          id: Date.now(),
                          method: "tools/call",
                          params: {
                            name: "kb_add_document",
                            arguments: { title: kbTitle, content: kbContent }
                          }
                        })
                      });
                      if (res.ok) {
                        alert("Added to Knowledge Base successfully!");
                        setKbTitle("");
                        setKbContent("");
                        loadData();
                      }
                    } catch (err) {
                      console.error(err);
                    }
                  }}
                  style={{ alignSelf: 'flex-start' }}
                >
                  Save Document
                </button>
              </div>

              <div 
                className="drop-zone"
                style={{ padding: '2rem 1rem', marginTop: '1rem' }}
                onDragOver={(e) => { e.preventDefault(); e.currentTarget.classList.add('drag-active'); }}
                onDragLeave={(e) => { e.preventDefault(); e.currentTarget.classList.remove('drag-active'); }}
                onDrop={async (e) => {
                  e.preventDefault();
                  e.currentTarget.classList.remove('drag-active');
                  const files = Array.from(e.dataTransfer.files);
                  if (files.length > 0) {
                    const path = (files[0] as any).path || files[0].name;
                    try {
                      const res = await fetch('http://127.0.0.1:8081/api/knowledge', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ directory_path: path })
                      });
                      if (res.ok) {
                        alert(`Directory '${path}' added to Vector DB for indexing.`);
                        loadData();
                      }
                    } catch (err) {
                      console.error(err);
                    }
                  }
                }}
              >
                <Folder size={32} className="drop-icon" style={{ marginBottom: '0.5rem' }} />
                <h3 style={{ fontSize: '1.1rem' }}>Drop folders to index</h3>
                <p style={{ fontSize: '0.85rem' }}>Your mobile agent will be able to search these directories.</p>
              </div>
            </>
          )}

          {activeTab === 'schedule' && (
            <>
              <h1>Scheduled Tasks</h1>
              <p className="subtitle">Background autonomous cron jobs</p>
              <div className="settings-panel">
                <div className="setting-row" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: '1rem' }}>
                  <label>Create New Task</label>
                  <input 
                    type="text" 
                    placeholder="e.g. Summarize my morning emails" 
                    value={newTaskName}
                    onChange={(e) => setNewTaskName(e.target.value)}
                    className="text-input"
                    style={{ width: '100%' }}
                  />
                  <div style={{ display: 'flex', gap: '1rem', width: '100%' }}>
                    <select 
                      value={newCronExp}
                      onChange={(e) => setNewCronExp(e.target.value)}
                      style={{ flex: 1 }}
                    >
                      <option value="0 7 * * *">Every morning at 7:00 AM (0 7 * * *)</option>
                      <option value="0 */4 * * *">Every 4 hours (0 */4 * * *)</option>
                      <option value="0 0 * * *">Once tonight at Midnight (0 0 * * *)</option>
                    </select>
                    <button className="btn-approve" onClick={handleCreateTask}>Schedule</button>
                  </div>
                </div>
              </div>
              <h3 style={{ marginTop: '2.5rem', marginBottom: '1rem', fontSize: '1.1rem', color: 'var(--text-main)', fontWeight: 500 }}>Active Tasks</h3>
              <div className="list-container">
                {loading && tasks.length === 0 ? (
                   <div className="empty-state">Loading tasks...</div>
                ) : tasks.length === 0 ? (
                   <div className="empty-state">No scheduled tasks active.</div>
                ) : (
                  tasks.map(task => (
                    <div key={task.id} className="request-card">
                      <div className="info">
                        <div className="client-name">{task.task_name}</div>
                        <div className="client-pubkey">Schedule: {task.cron_expression} (Created {task.created_at})</div>
                      </div>
                      <div className="actions">
                        <button className="btn-deny" onClick={() => handleDeleteTask(task.id)}>Cancel</button>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </>
          )}

          {activeTab === 'tunnel' && (
            <>
              <h1>Network Tunnel</h1>
              <p className="subtitle">Bypass the Cloud Router using your own Ngrok or Cloudflare tunnel</p>
              <div className="settings-panel">
                <div className="setting-row" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: '1rem' }}>
                  <label>1. Start your tunnel locally</label>
                  <code style={{ background: 'var(--bg-dark)', padding: '1.25rem', width: '100%', borderRadius: '6px', border: '1px solid var(--border-color)', color: 'var(--accent-light)', fontFamily: 'JetBrains Mono', fontSize: '0.9rem' }}>
                    ngrok http 8081<br />
                    <span style={{ color: 'var(--text-muted)' }}># or</span><br />
                    cloudflared tunnel --url http://localhost:8081
                  </code>
                </div>
                
                <div className="setting-row" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: '1rem', marginTop: '0.5rem' }}>
                  <label>2. Paste your Tunnel URL</label>
                  <input 
                    type="text" 
                    placeholder="https://abc-123.ngrok-free.app" 
                    className="text-input" 
                    style={{ width: '100%' }}
                    value={tunnelUrl}
                    onChange={(e) => setTunnelUrl(e.target.value)}
                  />
                </div>

                {tunnelUrl && tunnelUrl.startsWith("http") && (
                  <div style={{ marginTop: '2rem', display: 'flex', flexDirection: 'column', alignItems: 'center', gap: '1rem', padding: '2rem', background: 'var(--bg-card)', borderRadius: '8px', border: '1px dashed var(--border-highlight)' }}>
                    <h3 style={{ margin: 0 }}>Scan to Pair</h3>
                    <p style={{ color: 'var(--text-muted)', textAlign: 'center', marginBottom: '1rem' }}>Open the Baton Mobile App and scan this QR code to connect directly over your secure tunnel.</p>
                    <img 
                      src={`https://api.qrserver.com/v1/create-qr-code/?size=200x200&data=${encodeURIComponent(`baton://connect?url=${tunnelUrl}`)}&bgcolor=FFFFFF`} 
                      alt="Pairing QR Code" 
                      style={{ borderRadius: '8px', border: '8px solid white' }}
                    />
                    <code style={{ marginTop: '1rem', color: 'var(--text-muted)', fontSize: '0.85rem' }}>baton://connect?url={tunnelUrl}</code>
                  </div>
                )}
              </div>
            </>
          )}

          {activeTab === 'permissions' && (
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
          )}

          {activeTab === 'audit' && (
            <>
              <h1>Activity Audit Log</h1>
              <p className="subtitle">Real-time security and invocation ledger</p>
              <div className="audit-log-container">
                {loading && auditLogs.length === 0 ? (
                  <div className="empty-state" style={{border: 'none'}}>Loading audit logs...</div>
                ) : auditLogs.length === 0 ? (
                  <div className="empty-state" style={{border: 'none'}}>No activity logged yet.</div>
                ) : (
                  auditLogs.map(log => (
                    <div key={log.id} className={`audit-entry ${log.severity}`}>
                      <span className="audit-time">[{log.created_at}]</span>
                      <span className="audit-event">{log.event_type}</span>
                      <span className="audit-detail">{log.detail}</span>
                    </div>
                  ))
                )}
              </div>
            </>
          )}
        </div>
      </main>
    </div>
  );
}

export default App;
