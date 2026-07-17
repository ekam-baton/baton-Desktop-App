-- Use SQLite FTS5 for full-text search without LLMs
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_chunks USING fts5(
    id UNINDEXED,
    namespace UNINDEXED, -- Client Isolation
    document_title,
    content,
    created_at UNINDEXED
);
