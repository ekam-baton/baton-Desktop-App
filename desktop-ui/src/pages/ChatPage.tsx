import { useState, useRef, useEffect } from 'react';
import { MessageSquare, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import logo from '../assets/logo.png';
import { useAppContext } from '../contexts/AppContext';
import ReactMarkdown from 'react-markdown';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { vscDarkPlus } from 'react-syntax-highlighter/dist/esm/styles/prism';

const API_BASE = import.meta.env.VITE_MCP_URL || 'http://localhost:8081';

export const ChatPage = () => {
  const { adminPassword } = useAppContext();
  const [chatInput, setChatInput] = useState("");
  const [chatHistory, setChatHistory] = useState<{role: string, content: string}[]>(() => {
    const saved = localStorage.getItem('baton_chat_history');
    if (saved) {
      try {
        return JSON.parse(saved);
      } catch (e) {
        return [];
      }
    }
    return [];
  });
  const chatEndRef = useRef<HTMLDivElement>(null);
  
  useEffect(() => {
    localStorage.setItem('baton_chat_history', JSON.stringify(chatHistory.slice(-100)));
    if (chatEndRef.current) chatEndRef.current.scrollIntoView({ behavior: 'smooth' });
  }, [chatHistory]);

  const clearChat = () => {
    setChatHistory([]);
    localStorage.removeItem('baton_chat_history');
  };

  const handleSend = async () => {
    if (!chatInput.trim()) return;
    const msg = chatInput;
    setChatInput('');
    setChatHistory(prev => [...prev, {role: 'user', content: msg}]);
    
    try {
      const response = await fetch(`${API_BASE}/admin/api/chat`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': 'Basic ' + btoa('admin:' + adminPassword)
        },
        body: JSON.stringify({
          messages: [...chatHistory, {role: 'user', content: msg}],
          options: {}
        })
      });
      
      const reader = response.body?.getReader();
      const decoder = new TextDecoder();
      setChatHistory(prev => [...prev, {role: 'agent', content: ''}]);
      
      if (reader) {
        while (true) {
          const {done, value} = await reader.read();
          if (done) break;
          const chunk = decoder.decode(value);
          const lines = chunk.split('\n');
          for (const line of lines) {
            if (line.startsWith('data: ')) {
              const data = line.slice(6);
              setChatHistory(prev => {
                const newHist = [...prev];
                newHist[newHist.length - 1].content += data;
                return newHist;
              });
            }
          }
        }
      }
    } catch (err) {
      toast.error("Failed to communicate with agent.");
    }
  };

  return (
    <div className="chat-container flex flex-col h-full">
      <div className="chat-header glass-panel" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div className="chat-header-info">
          <div className="avatar-agent">
            <img src={logo} alt="Agent" />
          </div>
          <div>
            <h2 style={{ margin: 0, fontSize: '1.2rem' }}>Baton AI</h2>
            <span className="chat-status">● Online (Local Compute)</span>
          </div>
        </div>
        <button onClick={clearChat} className="btn-secondary" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <Trash2 size={16} /> Clear Chat
        </button>
      </div>
      <div className="chat-history flex-1 overflow-y-auto">
        {chatHistory.length === 0 && (
          <div className="chat-empty">
            <div className="chat-empty-icon"><img src={logo} alt="Baton Logo" /></div>
            <h3>Your Local AI</h3>
            <p>Start a secure, completely private conversation with your agent.</p>
          </div>
        )}
        {chatHistory.map((msg, idx) => (
          <div key={idx} className={`chat-message-wrapper ${msg.role === 'user' ? 'wrapper-user' : 'wrapper-agent'}`}>
            {msg.role === 'agent' && (
                <div className="message-avatar"><img src={logo} alt="Agent" /></div>
            )}
            <div className={`chat-message ${msg.role === 'user' ? 'message-user' : 'message-agent'}`}>
              <div className="message-content">
                {msg.content ? (
                  msg.role === 'agent' ? (
                    <ReactMarkdown
                      components={{
                        code({node, inline, className, children, ...props}: any) {
                          const match = /language-(\w+)/.exec(className || '')
                          return !inline && match ? (
                            <SyntaxHighlighter
                              style={vscDarkPlus as any}
                              language={match[1]}
                              PreTag="div"
                              {...props}
                            >
                              {String(children).replace(/\n$/, '')}
                            </SyntaxHighlighter>
                          ) : (
                            <code className={className} {...props}>
                              {children}
                            </code>
                          )
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
        ))}
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
          />
          <button className="chat-send-btn" onClick={handleSend}><MessageSquare size={18} /></button>
        </div>
      </div>
    </div>
  );
};

