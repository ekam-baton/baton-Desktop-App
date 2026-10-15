CREATE TABLE IF NOT EXISTS agent_authorizations (
    client_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    permission TEXT NOT NULL DEFAULT 'read_write',
    created_at INTEGER NOT NULL,
    PRIMARY KEY (client_id, agent_id),
    FOREIGN KEY(client_id) REFERENCES authorized_users(client_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_auth_client ON agent_authorizations(client_id);
CREATE INDEX IF NOT EXISTS idx_agent_auth_agent ON agent_authorizations(agent_id);
