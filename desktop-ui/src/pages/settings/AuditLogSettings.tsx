import { useAppContext } from '../../contexts/AppContext';

export const AuditLogSettings = () => {
  const { auditLogs, loading } = useAppContext();

  return (
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
  );
};
