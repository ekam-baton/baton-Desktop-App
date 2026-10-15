import { useState, useRef, useEffect, useCallback } from 'react';
import { MessageSquare, Trash2, Plus, MessageCircle, PanelLeftClose, PanelLeftOpen } from 'lucide-react';
import { toast } from 'sonner';
import logo from '../assets/logo.png';
import { useAppContext } from '../contexts/AppContext';
import ReactMarkdown from 'react-markdown';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { vscDarkPlus } from 'react-syntax-highlighter/dist/esm/styles/prism';

const API_BASE = import.meta.env.VITE_MCP_URL || 'http://localhost:8081';

interface ChatSession {
  id: string;
  title: string;
  agent_id?: string;
  created_at: number;
  updated_at: number;
  is_pinned: boolean;
}

interface StoredMessage {
  id?: string;
  session_id?: string;
  role: string;
  content: string;
  tool_call_json?: string;
  timestamp?: number;
}

export const ChatPage = () => {
  const { adminPassword } = useAppContext();
  const [chatInput, setChatInput] = useState("");
  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [chatHistory, setChatHistory] = useState<StoredMessage[]>([]);
  const [showSessions, setShowSessions] = useState(true);
  const [isStreaming, setIsStreaming] = useState(false);
  const chatEndRef = useRef<HTMLDivElement>(null);

  const getAuthHeaders = useCallback(() => ({
    'Content-Type': 'application/json',
    'Authorization': 'Basic ' + btoa('admin:' + adminPassword)
  }), [adminPassword]);

  // Load all sessions from the persistent database
  const loadSessions = useCallback(async () => {
    try {
      const res = await fetch(`${API_BASE}/admin/api/sessions`, {
        headers: getAuthHeaders()
      });
      if (res.ok) {
        const data: ChatSession[] = await res.json();
        setSessions(data);
        if (data.length > 0 && !activeSessionId) {
          setActiveSessionId(data[0].id);
        } else if (data.length === 0) {
          // Auto-create initial session if none exist
          const createRes = await fetch(`${API_BASE}/admin/api/sessions`, {
            method: 'POST',
            headers: getAuthHeaders(),
            body: JSON.stringify({ title: "New Conversation" })
          });
          if (createRes.ok) {
            const newSess: ChatSession = await createRes.json();
            setSessions([newSess]);
            setActiveSessionId(newSess.id);
          }
        }
      }
    } catch (err) {
      console.error("Failed to load sessions:", err);
    }
  }, [getAuthHeaders, activeSessionId]);

  // Load messages whenever active session changes
  const loadMessages = useCallback(async (sessionId: string) => {
    try {
      const res = await fetch(`${API_BASE}/admin/api/sessions/${sessionId}/messages`, {
        headers: getAuthHeaders()
      });
      if (res.ok) {
        const msgs: StoredMessage[] = await res.json();
        setChatHistory(msgs);
      }
    } catch (err) {
      console.error("Failed to load messages for session:", err);
    }
  }, [getAuthHeaders]);

  useEffect(() => {
    loadSessions();
  }, [loadSessions]);

  useEffect(() => {
    if (activeSessionId) {
      loadMessages(activeSessionId);
    }
  }, [activeSessionId, loadMessages]);

  useEffect(() => {
    if (chatEndRef.current) chatEndRef.current.scrollIntoView({ behavior: 'smooth' });
  }, [chatHistory]);

  const createNewSession = async () => {
    try {
      const res = await fetch(`${API_BASE}/admin/api/sessions`, {
        method: 'POST',
        headers: getAuthHeaders(),
        body: JSON.stringify({ title: "New Conversation" })
      });
      if (res.ok) {
        const newSess: ChatSession = await res.json();
        setSessions(prev => [newSess, ...prev]);
        setActiveSessionId(newSess.id);
        setChatHistory([]);
        return newSess;
      }
    } catch {
      toast.error("Failed to create new session.");
    }
    return null;
  };

  const deleteSession = async (sessionId: string) => {
    try {
      await fetch(`${API_BASE}/admin/api/sessions/${sessionId}`, {
        method: 'DELETE',
        headers: getAuthHeaders()
      });
      const remaining = sessions.filter(s => s.id !== sessionId);
      setSessions(remaining);
      if (activeSessionId === sessionId) {
        if (remaining.length > 0) {
          setActiveSessionId(remaining[0].id);
        } else {
          setActiveSessionId(null);
          setChatHistory([]);
          createNewSession();
        }
      }
      toast.success("Session deleted.");
    } catch {
      toast.error("Failed to delete session.");
    }
  };

  const clearChat = async () => {
    if (!activeSessionId) return;
    try {
      await fetch(`${API_BASE}/admin/api/sessions/${activeSessionId}/messages`, {
        method: 'DELETE',
        headers: getAuthHeaders()
      });
      setChatHistory([]);
      toast.success("Chat history cleared from database.");
    } catch {
      toast.error("Failed to clear chat history.");
    }
  };

  const handleSend = async () => {
    if (!chatInput.trim() || isStreaming) return;
    const msg = chatInput.trim();
    setChatInput('');

    let targetSessionId = activeSessionId;
    if (!targetSessionId) {
      const newSess = await createNewSession();
      if (!newSess) return;
      targetSessionId = newSess.id;
    }

    // Optimistically update UI
    const userMsg: StoredMessage = { role: 'user', content: msg, timestamp: Date.now() };
    setChatHistory(prev => [...prev, userMsg]);

    // Persist user message to SQLite
    try {
      await fetch(`${API_BASE}/admin/api/sessions/${targetSessionId}/messages`, {
        method: 'POST',
        headers: getAuthHeaders(),
        body: JSON.stringify({ role: 'user', content: msg })
      });
    } catch (err) {
      console.error("Failed to persist user message:", err);
    }

    // Auto-update session title if default
    const activeSess = sessions.find(s => s.id === targetSessionId);
    if (activeSess && (activeSess.title === 'New Conversation' || !activeSess.title)) {
      const newTitle = msg.length > 28 ? msg.slice(0, 28) + '...' : msg;
      fetch(`${API_BASE}/admin/api/sessions/${targetSessionId}`, {
        method: 'PUT',
        headers: getAuthHeaders(),
        body: JSON.stringify({ title: newTitle })
      }).then(() => {
        setSessions(prev => prev.map(s => s.id === targetSessionId ? { ...s, title: newTitle } : s));
      }).catch(console.error);
    }

    // Call LLM streaming endpoint
    setIsStreaming(true);
    let agentResponseText = '';
    try {
      const response = await fetch(`${API_BASE}/admin/api/chat`, {
        method: 'POST',
        headers: getAuthHeaders(),
        body: JSON.stringify({
          messages: [...chatHistory, { role: 'user', content: msg }].map(m => ({
            role: (m.role === 'agent' || m.role === 'assistant') ? 'assistant' : m.role,
            content: m.content
          })),
          options: {}
        })
      });

      const reader = response.body?.getReader();
      const decoder = new TextDecoder();
      setChatHistory(prev => [...prev, { role: 'assistant', content: '' }]);

      if (reader) {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          const chunk = decoder.decode(value);
          const lines = chunk.split('\n');
          for (const line of lines) {
            if (line.startsWith('data: ')) {
              const data = line.slice(6);
              agentResponseText += data;
              setChatHistory(prev => {
                const newHist = [...prev];
                newHist[newHist.length - 1].content = agentResponseText;
                return newHist;
              });
            }
          }
        }
      }

      // Persist completed agent response to SQLite
      if (agentResponseText) {
        await fetch(`${API_BASE}/admin/api/sessions/${targetSessionId}/messages`, {
          method: 'POST',
          headers: getAuthHeaders(),
          body: JSON.stringify({ role: 'assistant', content: agentResponseText })
        });
      }
    } catch {
      toast.error("Failed to communicate with agent.");
    } finally {
      setIsStreaming(false);
    }
  };

  const activeSessionTitle = sessions.find(s => s.id === activeSessionId)?.title || "Baton AI";

  return (
    <div className="flex h-full w-full gap-4 max-w-[1300px] mx-auto p-2">
      {/* Sessions Drawer / Sidebar */}
      {showSessions && (
        <div className="w-64 glass-panel rounded-2xl flex flex-col p-3 border border-white/5 flex-shrink-0">
          <div className="flex items-center justify-between pb-3 border-b border-white/10 mb-3">
            <span className="text-sm font-semibold text-neutral-300 flex items-center gap-2">
              <MessageCircle size={16} /> Sessions
            </span>
            <button
              onClick={createNewSession}
              className="p-1.5 rounded-lg hover:bg-white/10 text-neutral-200 transition-colors"
              title="New Chat"
            >
              <Plus size={16} />
            </button>
          </div>
          <div className="flex-1 overflow-y-auto space-y-1 pr-1">
            {sessions.map(s => (
              <div
                key={s.id}
                onClick={() => setActiveSessionId(s.id)}
                className={`group flex items-center justify-between p-2.5 rounded-xl cursor-pointer text-xs transition-colors ${
                  s.id === activeSessionId
                    ? 'bg-white/15 text-white font-medium'
                    : 'text-neutral-400 hover:bg-white/5 hover:text-neutral-200'
                }`}
              >
                <span className="truncate flex-1 pr-2">{s.title || 'New Conversation'}</span>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    deleteSession(s.id);
                  }}
                  className="opacity-0 group-hover:opacity-100 hover:text-red-400 p-1 rounded transition-opacity"
                  title="Delete Session"
                >
                  <Trash2 size={13} />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Main Chat Area */}
      <div className="chat-container flex flex-col h-full flex-1 min-w-0">
        <div className="chat-header glass-panel" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div className="chat-header-info">
            <button
              onClick={() => setShowSessions(prev => !prev)}
              className="p-2 rounded-lg hover:bg-white/10 text-neutral-300 transition-colors"
              title={showSessions ? "Hide Sessions" : "Show Sessions"}
            >
              {showSessions ? <PanelLeftClose size={18} /> : <PanelLeftOpen size={18} />}
            </button>
            <div className="avatar-agent">
              <img src={logo} alt="Agent" />
            </div>
            <div>
              <h2 style={{ margin: 0, fontSize: '1.2rem' }}>{activeSessionTitle}</h2>
              <span className="chat-status">● SQLite Relational Engine Active</span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <button onClick={createNewSession} className="btn-secondary" style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
              <Plus size={16} /> New Chat
            </button>
            <button onClick={clearChat} className="btn-secondary" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <Trash2 size={16} /> Clear Chat
            </button>
          </div>
        </div>

        <div className="chat-history flex-1 overflow-y-auto">
          {chatHistory.length === 0 && (
            <div className="chat-empty">
              <div className="chat-empty-icon"><img src={logo} alt="Baton Logo" /></div>
              <h3>Your Sovereign AI</h3>
              <p>Every message is encrypted and persisted in your local SQLite relational database.</p>
            </div>
          )}
          {chatHistory.map((msg, idx) => {
            const isAgent = msg.role === 'agent' || msg.role === 'assistant';
            return (
              <div key={idx} className={`chat-message-wrapper ${isAgent ? 'wrapper-agent' : 'wrapper-user'}`}>
                {isAgent && (
                  <div className="message-avatar"><img src={logo} alt="Agent" /></div>
                )}
                <div className={`chat-message ${isAgent ? 'message-agent' : 'message-user'}`}>
                  <div className="message-content">
                    {msg.content ? (
                      isAgent ? (
                        <ReactMarkdown
                          components={{
                            code({ inline, className, children, ...props }: React.ComponentPropsWithoutRef<'code'> & { inline?: boolean }) {
                              const restCodeProps: Record<string, unknown> = {};
                              for (const [key, val] of Object.entries(props)) {
                                if (key !== 'node' && key !== 'style') {
                                  restCodeProps[key] = val;
                                }
                              }
                              const match = /language-(\w+)/.exec(className || '');
                              return !inline && match ? (
                                <SyntaxHighlighter
                                  style={vscDarkPlus as unknown as { [key: string]: React.CSSProperties }}
                                  language={match[1]}
                                  PreTag="div"
                                  {...restCodeProps}
                                >
                                  {String(children).replace(/\n$/, '')}
                                </SyntaxHighlighter>
                              ) : (
                                <code className={className} {...props}>
                                  {children}
                                </code>
                              );
                            }
                          }}
                        >
                          {msg.content}
                        </ReactMarkdown>
                      ) : (
                        msg.content
                      )
                    ) : (
                      <span className="typing-indicator"><span></span><span></span><span></span></span>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
          <div ref={chatEndRef} style={{ height: '10px' }} />
        </div>

        <div className="chat-input-area">
          <div className="chat-input-wrapper glass-panel">
            <input 
              type="text" 
              value={chatInput} 
              onChange={(e) => setChatInput(e.target.value)} 
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleSend();
              }}
              placeholder={"Message Baton AI..."}
              disabled={isStreaming}
            />
            <button className="chat-send-btn" onClick={handleSend} disabled={isStreaming}>
              <MessageSquare size={18} />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
