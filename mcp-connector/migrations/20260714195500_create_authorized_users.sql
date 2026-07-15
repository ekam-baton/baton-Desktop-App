CREATE TABLE IF NOT EXISTS authorized_users (
    client_id TEXT PRIMARY KEY,
    public_key TEXT NOT NULL,
    device_name TEXT,
    status TEXT NOT NULL,
    role TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now'))
);