//! LLM backend trait and factory.

pub mod claude;
pub mod ollama;

use std::io::Read;

/// A message in a conversation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".to_string(), content: content.into() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".to_string(), content: content.into() }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".to_string(), content: content.into() }
    }
}

/// Trait for LLM backends (Ollama, Claude, etc.).
pub trait LlmBackend: Send + Sync {
    /// Send a conversation and receive a streaming response.
    /// Returns a reader that yields response tokens as NDJSON lines.
    fn chat(&self, messages: &[Message]) -> anyhow::Result<Box<dyn Read + Send>>;

    /// One-shot generation (for command overlay).
    /// Convenience wrapper around chat with a single user message.
    fn generate(&self, prompt: &str, system: &str) -> anyhow::Result<Box<dyn Read + Send>> {
        let messages = vec![
            Message::system(system),
            Message::user(prompt),
        ];
        self.chat(&messages)
    }

    /// Check if the backend is reachable.
    fn is_available(&self) -> bool;

    /// Human-readable name for error messages.
    fn name(&self) -> &str;
}

/// Create the appropriate backend based on configuration.
pub fn create_backend(config: &crate::config::AiConfig) -> Box<dyn LlmBackend> {
    if config.is_claude() {
        Box::new(claude::ClaudeBackend::new(
            config.api_key.clone().unwrap_or_default(),
            config.model.clone(),
        ))
    } else {
        Box::new(ollama::OllamaBackend::new(
            config.endpoint.clone(),
            config.model.clone(),
        ))
    }
}
