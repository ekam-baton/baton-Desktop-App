CREATE TABLE IF NOT EXISTS paired_devices (
    client_id TEXT PRIMARY KEY,
    public_key TEXT NOT NULL,
    device_name TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now'))
);
