import { useState } from 'react';
import { toast } from 'sonner';
import { useAppContext } from '../../contexts/AppContext';

const API_BASE = import.meta.env.VITE_MCP_URL || 'http://localhost:8081';

export const HandoffSettings = () => {
  const { adminPassword } = useAppContext();
  const [selectedFile, setSelectedFile] = useState<File | null>(null);

  const handleUpload = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedFile) return;

    const formData = new FormData();
    formData.append("file", selectedFile);

    try {
      const res = await fetch(`${API_BASE}/api/handoff/upload`, {
        method: 'POST',
        headers: {
          'Authorization': 'Basic ' + btoa('admin:' + adminPassword)
        },
        body: formData
      });

      if (res.ok) {
        toast.success('Context successfully pushed to mobile agent');
        setSelectedFile(null);
      } else {
        toast.error('Failed to push context');
      }
    } catch (err) {
      toast.error('Network error during upload');
    }
  };

  return (
    <>
      <h1>Context Handoff</h1>
      <p className="subtitle">Send files or context to your phone before you leave</p>
      
      <div className="settings-panel">
        <form onSubmit={handleUpload} style={{ display: 'flex', flexDirection: 'column', gap: '1.5rem', alignItems: 'center', padding: '2rem 1rem' }}>
          
          <div style={{ border: '2px dashed var(--border)', padding: '3rem', borderRadius: '12px', textAlign: 'center', width: '100%', cursor: 'pointer' }}>
            <input 
              type="file" 
              onChange={(e) => {
                if (e.target.files && e.target.files[0]) setSelectedFile(e.target.files[0]);
              }}
              style={{ display: 'none' }}
              id="file-upload"
            />
            <label htmlFor="file-upload" style={{ cursor: 'pointer', color: 'var(--text-muted)' }}>
              {selectedFile ? selectedFile.name : 'Click to select a file for handoff'}
            </label>
          </div>

          <button 
            type="submit" 
            className="btn-approve" 
            style={{ padding: '0.8rem 2rem', width: '100%' }}
            disabled={!selectedFile}
          >
            Push to Mobile Device
          </button>
        </form>
      </div>
    </>
  );
};
