CREATE TABLE IF NOT EXISTS audit_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type VARCHAR(100) NOT NULL,
    detail TEXT NOT NULL,
    severity VARCHAR(20) NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_name VARCHAR(255) NOT NULL,
    cron_expression VARCHAR(100) NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS permissions (
    key VARCHAR(100) PRIMARY KEY,
    value BOOLEAN NOT NULL
);

CREATE TABLE IF NOT EXISTS inbox_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_name VARCHAR(255) NOT NULL,
    sender VARCHAR(100) NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS knowledge_base (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    directory_path VARCHAR(255) NOT NULL,
    file_count INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Insert default permissions
INSERT OR IGNORE INTO permissions (key, value) VALUES ('fs_read', 1);
INSERT OR IGNORE INTO permissions (key, value) VALUES ('fs_write', 0);
INSERT OR IGNORE INTO permissions (key, value) VALUES ('shell', 0);
INSERT OR IGNORE INTO permissions (key, value) VALUES ('web_search', 1);
