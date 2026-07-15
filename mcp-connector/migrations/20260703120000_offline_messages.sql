CREATE TABLE IF NOT EXISTS offline_messages (
    id TEXT PRIMARY KEY,
    receiver_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now'))
);

-- Index for fast lookup by receiver_id when a client connects
CREATE INDEX IF NOT EXISTS idx_offline_messages_receiver ON offline_messages(receiver_id);
