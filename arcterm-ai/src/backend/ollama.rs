//! Ollama REST API backend.

use super::{LlmBackend, Message};
use std::io::Read;

/// Ollama backend connecting to a local Ollama instance.
pub struct OllamaBackend {
    endpoint: String,
    model: String,
}

impl OllamaBackend {
    pub fn new(endpoint: String, model: String) -> Self {
        Self { endpoint, model }
    }

    fn chat_url(&self) -> String {
        format!("{}/api/chat", self.endpoint)
    }

    fn tags_url(&self) -> String {
        format!("{}/api/tags", self.endpoint)
    }
}

impl LlmBackend for OllamaBackend {
    fn chat(&self, messages: &[Message]) -> anyhow::Result<Box<dyn Read + Send>> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
        });

        let response = ureq::post(&self.chat_url())
            .set("Content-Type", "application/json")
            .send_json(&body)
            .map_err(|e| anyhow::anyhow!("Ollama request failed: {}", e))?;

        Ok(Box::new(response.into_reader()))
    }

    fn is_available(&self) -> bool {
        match ureq::get(&self.tags_url())
            .timeout(std::time::Duration::from_secs(2))
            .call()
        {
            Ok(resp) => resp.status() == 200,
            Err(_) => false,
        }
    }

    fn name(&self) -> &str {
        "Ollama"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_urls() {
        let backend = OllamaBackend::new(
            "http://localhost:11434".to_string(),
            "qwen2.5-coder:7b".to_string(),
        );
        assert_eq!(backend.chat_url(), "http://localhost:11434/api/chat");
        assert_eq!(backend.tags_url(), "http://localhost:11434/api/tags");
    }

    #[test]
    fn test_ollama_unavailable_when_not_running() {
        // Connect to a port that's unlikely to be in use
        let backend = OllamaBackend::new(
            "http://127.0.0.1:19999".to_string(),
            "test".to_string(),
        );
        assert!(!backend.is_available());
    }
}
