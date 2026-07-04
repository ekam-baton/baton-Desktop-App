CREATE TABLE IF NOT EXISTS offline_messages (
    id UUID PRIMARY KEY,
    receiver_id VARCHAR(64) NOT NULL,
    sender_id VARCHAR(64) NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Index for fast lookup by receiver_id when a client connects
CREATE INDEX idx_offline_messages_receiver ON offline_messages(receiver_id);
