import { useState } from 'react';
import { toast } from 'sonner';
import { Trash2 } from 'lucide-react';
import { useAppContext } from '../../contexts/AppContext';
import { basicAuthHeader } from '../../lib/utils';

const API_BASE = import.meta.env.VITE_MCP_URL || 'http://localhost:8081';

export const KnowledgeBaseSettings = () => {
  const { knowledgeDirs, loading, loadData, adminPassword } = useAppContext();
  const [newDirPath, setNewDirPath] = useState("");

  const handleAddDirectory = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newDirPath.trim()) return;

    try {
      const res = await fetch(`${API_BASE}/api/knowledge`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': basicAuthHeader(adminPassword)
        },
        body: JSON.stringify({ directory_path: newDirPath.trim() })
      });

      if (res.ok) {
        toast.success('Directory indexed successfully');
        setNewDirPath("");
        loadData();
      } else {
        toast.error('Failed to index directory');
      }
    } catch {
      toast.error('Network error while indexing');
    }
  };

  const handleDeleteDirectory = async (id: number) => {
    try {
      const res = await fetch(`${API_BASE}/api/knowledge/${id}`, {
        method: 'DELETE',
        headers: {
          'Authorization': basicAuthHeader(adminPassword)
        }
      });
      if (res.ok) {
        toast.success('Directory removed from index');
        loadData();
      } else {
        toast.error('Failed to remove directory');
      }
    } catch {
      toast.error('Network error while removing directory');
    }
  };

  return (
    <>
      <h1>Local Knowledge Base</h1>
      <p className="subtitle">Directories indexed for RAG vector search</p>

      <div className="settings-panel" style={{ marginBottom: '2rem' }}>
        <form onSubmit={handleAddDirectory} style={{ display: 'flex', gap: '1rem', alignItems: 'flex-end' }}>
          <div style={{ flex: 1 }}>
            <label style={{ display: 'block', marginBottom: '0.5rem' }}>Add Directory to Index</label>
            <input 
              type="text" 
              placeholder="e.g., C:\Users\Username\Documents\Research"
              value={newDirPath}
              onChange={(e) => setNewDirPath(e.target.value)}
              className="input-field"
              style={{ width: '100%' }}
            />
          </div>
          <button type="submit" className="btn-approve" style={{ padding: '0.8rem 1.5rem', height: 'fit-content' }}>Index</button>
        </form>
      </div>

      <div className="list-container">
        {loading && knowledgeDirs.length === 0 ? (
          <div className="empty-state">Loading knowledge base...</div>
        ) : knowledgeDirs.length === 0 ? (
          <div className="empty-state">No directories indexed yet.</div>
        ) : (
          knowledgeDirs.map((dir) => (
            <div key={dir.id} className="request-card" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div className="info">
                <div className="client-name">{dir.directory_path}</div>
                <div className="client-pubkey">Files: {dir.file_count} • Indexed: {dir.created_at}</div>
              </div>
              <button 
                onClick={() => handleDeleteDirectory(dir.id)} 
                className="btn-deny" 
                title="Remove directory"
                style={{ padding: '0.5rem', display: 'flex', alignItems: 'center' }}
              >
                <Trash2 size={16} />
              </button>
            </div>
          ))
        )}
      </div>
    </>
  );
};
