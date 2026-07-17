use std::sync::Arc;
use sqlx::{AnyPool, Row};
use anyhow::Result;
use uuid::Uuid;

/// A chunk of knowledge in the FTS5 DB.
#[derive(Debug, Clone)]
pub struct KnowledgeChunk {
    pub document_title: String,
    pub content: String,
}

pub struct KnowledgeBase {
    db: AnyPool,
}

impl KnowledgeBase {
    pub fn new(db: AnyPool) -> Self {
        Self { db }
    }

    /// Add a new document chunk to the database with strict client isolation (namespace).
    pub async fn add_chunk(
        &self,
        namespace: &str,
        title: &str,
        content: &str,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO knowledge_chunks (id, namespace, document_title, content) 
             VALUES ($1, $2, $3, $4)"
        )
        .bind(&id)
        .bind(namespace)
        .bind(title)
        .bind(content)
        .execute(&self.db)
        .await?;

        Ok(id)
    }

    /// Perform a full-text search, strictly isolated by namespace.
    pub async fn search(
        &self,
        namespace: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeChunk>> {
        // Escape query for FTS5 (treat as literal phrase)
        let fts_query = format!("\"{}\"", query.replace("\"", "\"\""));

        // Retrieve ONLY the chunks for this namespace using FTS5 MATCH
        let rows = sqlx::query(
            "SELECT document_title, content 
             FROM knowledge_chunks 
             WHERE namespace = $1 AND knowledge_chunks MATCH $2
             ORDER BY rank
             LIMIT $3"
        )
        .bind(namespace)
        .bind(&fts_query)
        .bind(limit as i64)
        .fetch_all(&self.db)
        .await?;

        let mut results: Vec<KnowledgeChunk> = Vec::with_capacity(rows.len());

        for row in rows {
            let title: String = row.get("document_title");
            let content: String = row.get("content");

            let chunk = KnowledgeChunk {
                document_title: title,
                content,
            };

            results.push(chunk);
        }

        Ok(results)
    }

    /// Very simple word-based chunker for documents.
    pub fn chunk_text(text: &str, chunk_size_words: usize) -> Vec<String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        words.chunks(chunk_size_words)
            .map(|chunk| chunk.join(" "))
            .collect()
    }
}
