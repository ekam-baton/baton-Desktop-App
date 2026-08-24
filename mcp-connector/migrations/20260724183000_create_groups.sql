CREATE TABLE IF NOT EXISTS groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    creator_id TEXT NOT NULL,
    member_ids TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
