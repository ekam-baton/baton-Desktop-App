use async_trait::async_trait;

pub mod nvidia;
pub mod openai;
pub mod anthropic;
pub mod gemini;
pub mod ollama;

/// Unified streaming provider trait.
/// Each provider implements `chat_stream()` which returns Server-Sent Events.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Name used in logs and metrics
    fn name(&self) -> &'static str;

    /// Non-streaming chat completion — returns the full response as a Value.
    async fn chat(&self, messages: &[ChatMessage], options: &ChatOptions) -> Result<String, ProviderError>;

    /// Streaming chat completion — yields text deltas via a channel.
    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        options: &ChatOptions,
        tx: tokio::sync::mpsc::Sender<Result<String, ProviderError>>,
    ) -> Result<(), ProviderError>;

    /// Generate embeddings for knowledge base chunks.
    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        Err(ProviderError::NotConfigured("Embeddings not supported by this provider".into()))
    }
}

/// A chat message with role and content.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Options that can accompany a chat request.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChatOptions {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub system_prompt: Option<String>,
}

impl Default for ChatOptions {
    fn default() -> Self {
        Self {
            max_tokens: Some(2048),
            temperature: Some(0.7),
            system_prompt: None,
        }
    }
}

/// Errors returned by providers.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Provider not configured: missing API key for {0}")]
    NotConfigured(String),
    #[error("Stream closed unexpectedly")]
    StreamClosed,
    #[error("Timeout")]
    Timeout,
}

impl axum::response::IntoResponse for ProviderError {
    fn into_response(self) -> axum::response::Response {
        let msg = self.to_string();
        (axum::http::StatusCode::BAD_GATEWAY, msg).into_response()
    }
}

/// Select the right provider by name string.
/// Returns a boxed trait object.
pub fn get_provider(
    name: &str,
    config: &crate::config::Config,
    http: &reqwest::Client,
) -> Result<Box<dyn Provider>, ProviderError> {
    match name.to_lowercase().as_str() {
        "nvidianim" | "nvidia" => Ok(Box::new(nvidia::NvidiaProvider::new(config, http.clone())?)),
        "openai" | "openai-compatible" => Ok(Box::new(openai::OpenAiProvider::new(config, http.clone())?)),
        "anthropic" | "claude" => Ok(Box::new(anthropic::AnthropicProvider::new(config, http.clone())?)),
        "gemini" | "google" => Ok(Box::new(gemini::GeminiProvider::new(config, http.clone())?)),
        "ollama" | "local" => Ok(Box::new(ollama::OllamaProvider::new(config, http.clone()))),
        other => Err(ProviderError::NotConfigured(other.to_owned())),
    }
}

/// Helper to extract complete newline-delimited lines from an incoming stream of byte chunks,
/// preserving partial lines across chunk boundaries without loss or corruption.
#[derive(Default)]
pub struct LineStreamBuffer {
    buffer: Vec<u8>,
}

impl LineStreamBuffer {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn push_chunk(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    pub fn next_line(&mut self) -> Option<String> {
        if let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.buffer.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line_bytes).trim().to_string();
            Some(line)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_stream_buffer_chunk_split_and_multibyte() {
        let mut buf = LineStreamBuffer::new();
        // Chunk 1: Partial line ending mid-word with partial json
        buf.push_chunk(b"data: {\"text\": \"Hello ");
        assert_eq!(buf.next_line(), None);

        // Chunk 2: Rest of line with emoji and newline
        let emoji_part = "World 🌍\"}\n".as_bytes();
        buf.push_chunk(emoji_part);

        let line = buf.next_line().expect("Should extract complete line");
        assert_eq!(line, "data: {\"text\": \"Hello World 🌍\"}");
        assert_eq!(buf.next_line(), None);
    }
}
