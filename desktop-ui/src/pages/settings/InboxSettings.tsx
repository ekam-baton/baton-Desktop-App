import { useAppContext } from '../../contexts/AppContext';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

export const InboxSettings = () => {
  const { inboxFiles, loading } = useAppContext();

  return (
    <>
      <h1>Inbox Sync</h1>
      <p className="subtitle">Files synced from your mobile agent</p>
      
      <div className="list-container">
        {loading && inboxFiles.length === 0 ? (
          <div className="empty-state">Loading inbox...</div>
        ) : inboxFiles.length === 0 ? (
          <div className="empty-state">Inbox is empty. Sync files from your mobile agent to see them here.</div>
        ) : (
          inboxFiles.map((file) => (
            <div key={file.id} className="request-card">
              <div className="info">
                <div className="client-name">{file.file_name}</div>
                <div className="client-pubkey">From: {file.sender} • {file.created_at}</div>
              </div>
              <div className="actions">
                <button 
                  className="btn-approve" 
                  onClick={async () => {
                    try {
                      await invoke('open_inbox_file', { fileName: file.file_name });
                      toast.success(`Opening ${file.file_name}`);
                    } catch (e) {
                      toast.error(`Failed to open: ${e}`);
                    }
                  }}
                >
                  Open File
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </>
  );
};
