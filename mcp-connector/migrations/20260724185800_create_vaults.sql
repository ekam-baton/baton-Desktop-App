CREATE TABLE IF NOT EXISTS vaults (
    client_id TEXT PRIMARY KEY,
    vault_data TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
