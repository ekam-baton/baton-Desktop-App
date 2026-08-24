import { useState } from 'react';
import { toast } from 'sonner';
import { useAppContext } from '../../contexts/AppContext';

const API_BASE = import.meta.env.VITE_MCP_URL || 'http://localhost:8081';

export const GroupsSettings = () => {
  const { groups, authorized, loading, loadData, adminPassword } = useAppContext();
  const [groupName, setGroupName] = useState("");
  const [selectedMembers, setSelectedMembers] = useState<string[]>([]);

  const handleCreateGroup = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!groupName.trim() || selectedMembers.length === 0) return;

    try {
      const res = await fetch(`${API_BASE}/admin/api/group/create`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': 'Basic ' + btoa('admin:' + adminPassword)
        },
        body: JSON.stringify({ name: groupName.trim(), member_ids: selectedMembers })
      });

      if (res.ok) {
        toast.success('Group created successfully');
        setGroupName("");
        setSelectedMembers([]);
        loadData();
      } else {
        toast.error('Failed to create group');
      }
    } catch (err) {
      toast.error('Network error while creating group');
    }
  };

  const handleDeleteGroup = async (id: string) => {
    try {
      const res = await fetch(`${API_BASE}/admin/api/group/${encodeURIComponent(id)}`, {
        method: 'DELETE',
        headers: { 'Authorization': 'Basic ' + btoa('admin:' + adminPassword) }
      });
      if (res.ok) {
        toast.success('Group deleted');
        loadData();
      } else {
        toast.error('Failed to delete group');
      }
    } catch (err) {
      toast.error('Network error');
    }
  };

  const toggleMember = (clientId: string) => {
    setSelectedMembers(prev => 
      prev.includes(clientId) ? prev.filter(id => id !== clientId) : [...prev, clientId]
    );
  };

  return (
    <>
      <h1>Group Management</h1>
      <p className="subtitle">Manage authorized groups and sharing</p>

      <div className="settings-panel" style={{ marginBottom: '2rem' }}>
        <form onSubmit={handleCreateGroup} style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
          <div>
            <label style={{ display: 'block', marginBottom: '0.5rem' }}>Group Name</label>
            <input 
              type="text" 
              placeholder="e.g., Family Devices"
              value={groupName}
              onChange={(e) => setGroupName(e.target.value)}
              className="input-field"
              style={{ width: '100%' }}
            />
          </div>
          <div>
            <label style={{ display: 'block', marginBottom: '0.5rem' }}>Select Members (Authorized Agents)</label>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem', maxHeight: '150px', overflowY: 'auto', background: 'var(--surface)', padding: '0.5rem', borderRadius: '8px' }}>
              {authorized.length === 0 ? (
                <span className="settings-hint">No authorized agents available to add.</span>
              ) : (
                authorized.map(agent => (
                  <label key={agent.client_id} style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
                    <input 
                      type="checkbox" 
                      checked={selectedMembers.includes(agent.client_id)}
                      onChange={() => toggleMember(agent.client_id)}
                    />
                    {agent.device_name || 'Unknown Device'} ({agent.client_id.substring(0, 8)}...)
                  </label>
                ))
              )}
            </div>
          </div>
          <button type="submit" className="btn-approve" style={{ padding: '0.8rem', marginTop: '0.5rem' }} disabled={!groupName.trim() || selectedMembers.length === 0}>
            Create Group
          </button>
        </form>
      </div>

      <div className="list-container">
        {loading && groups.length === 0 ? (
          <div className="empty-state">Loading groups...</div>
        ) : groups.length === 0 ? (
          <div className="empty-state">No groups available.</div>
        ) : (
          groups.map((group) => (
            <div key={group.id} className="request-card">
              <div className="info">
                <div className="client-name">{group.name}</div>
                <div className="client-pubkey">Members: {group.member_ids.length} • ID: {group.id.substring(0, 8)}...</div>
              </div>
              <div className="actions">
                <button className="btn-deny" onClick={() => handleDeleteGroup(group.id)}>Delete</button>
              </div>
            </div>
          ))
        )}
      </div>
    </>
  );
};
