import { useState } from 'react';
import { toast } from 'sonner';
import { useAppContext } from '../../contexts/AppContext';

export const ScheduleSettings = () => {
  const { tasks, loading, loadData, adminPassword } = useAppContext();
  const [taskName, setTaskName] = useState("");
  const [cronExpression, setCronExpression] = useState("");

  const handleCreateTask = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!taskName.trim() || !cronExpression.trim()) return;

    try {
      const res = await fetch('http://localhost:8081/api/schedule', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': 'Basic ' + btoa('admin:' + adminPassword)
        },
        body: JSON.stringify({ task_name: taskName.trim(), cron_expression: cronExpression.trim() })
      });

      if (res.ok) {
        toast.success('Task scheduled successfully');
        setTaskName("");
        setCronExpression("");
        loadData();
      } else {
        toast.error('Failed to schedule task');
      }
    } catch (err) {
      toast.error('Network error');
    }
  };

  const handleDeleteTask = async (id: number) => {
    try {
      const res = await fetch(`http://localhost:8081/api/schedule/${id}`, {
        method: 'DELETE',
        headers: { 'Authorization': 'Basic ' + btoa('admin:' + adminPassword) }
      });
      if (res.ok) {
        toast.success('Task removed');
        loadData();
      } else {
        toast.error('Failed to remove task');
      }
    } catch (err) {
      toast.error('Network error');
    }
  };

  return (
    <>
      <h1>Scheduled Tasks</h1>
      <p className="subtitle">Manage recurring automation</p>
      
      <div className="settings-panel" style={{ marginBottom: '2rem' }}>
        <form onSubmit={handleCreateTask} style={{ display: 'flex', gap: '1rem', alignItems: 'flex-end', flexWrap: 'wrap' }}>
          <div style={{ flex: 1, minWidth: '200px' }}>
            <label style={{ display: 'block', marginBottom: '0.5rem' }}>Task Instructions</label>
            <input 
              type="text" 
              placeholder="e.g., Run daily system health check"
              value={taskName}
              onChange={(e) => setTaskName(e.target.value)}
              className="input-field"
              style={{ width: '100%' }}
            />
          </div>
          <div style={{ flex: 1, minWidth: '150px' }}>
            <label style={{ display: 'block', marginBottom: '0.5rem' }}>Cron Expression</label>
            <input 
              type="text" 
              placeholder="0 0 * * *"
              value={cronExpression}
              onChange={(e) => setCronExpression(e.target.value)}
              className="input-field"
              style={{ width: '100%' }}
            />
          </div>
          <button type="submit" className="btn-approve" style={{ padding: '0.8rem 1.5rem', height: 'fit-content' }}>Schedule</button>
        </form>
      </div>

      <div className="list-container">
        {loading && tasks.length === 0 ? (
          <div className="empty-state">Loading tasks...</div>
        ) : tasks.length === 0 ? (
          <div className="empty-state">No tasks scheduled.</div>
        ) : (
          tasks.map((task) => (
            <div key={task.id} className="request-card">
              <div className="info">
                <div className="client-name">{task.task_name}</div>
                <div className="client-pubkey">Cron: {task.cron_expression} • Created: {task.created_at}</div>
              </div>
              <div className="actions">
                <button className="btn-deny" onClick={() => handleDeleteTask(task.id)}>Remove</button>
              </div>
            </div>
          ))
        )}
      </div>
    </>
  );
};
