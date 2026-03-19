//! Claude (Anthropic) API backend.

use super::{LlmBackend, Message};
use std::io::Read;

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Claude API backend.
pub struct ClaudeBackend {
    api_key: String,
    model: String,
}

impl ClaudeBackend {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }
}

impl LlmBackend for ClaudeBackend {
    fn chat(&self, messages: &[Message]) -> anyhow::Result<Box<dyn Read + Send>> {
        // Claude API separates system prompt from messages
        let system = messages
            .iter()
            .find(|m| m.role == super::Role::System)
            .map(|m| m.content.clone())
            .unwrap_or_default();

        let user_messages: Vec<_> = messages
            .iter()
            .filter(|m| m.role != super::Role::System)
            .map(|m| serde_json::json!({"role": m.role, "content": m.content}))
            .collect();

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": system,
            "messages": user_messages,
            "stream": true,
        });

        let response = ureq::post(CLAUDE_API_URL)
            .set("Content-Type", "application/json")
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", ANTHROPIC_VERSION)
            .send_json(&body)
            .map_err(|e| anyhow::anyhow!("Claude API request failed: {}", e))?;

        Ok(Box::new(response.into_reader()))
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn name(&self) -> &str {
        "Claude"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_available_with_key() {
        let backend = ClaudeBackend::new("sk-ant-test".to_string(), "claude-sonnet-4-20250514".to_string());
        assert!(backend.is_available());
    }

    #[test]
    fn test_claude_unavailable_without_key() {
        let backend = ClaudeBackend::new("".to_string(), "claude-sonnet-4-20250514".to_string());
        assert!(!backend.is_available());
    }
}
