//! AI configuration types and defaults.

/// Configuration for the AI subsystem.
#[derive(Debug, Clone)]
pub struct AiConfig {
    /// LLM endpoint URL (default: Ollama on localhost)
    pub endpoint: String,
    /// Model identifier (default: qwen2.5-coder:7b)
    pub model: String,
    /// API key for remote providers (None for Ollama)
    pub api_key: Option<String>,
    /// Number of scrollback lines to include as context
    pub context_lines: u32,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder:7b".to_string(),
            api_key: None,
            context_lines: 30,
        }
    }
}

impl AiConfig {
    /// Determine if this config targets a Claude backend.
    pub fn is_claude(&self) -> bool {
        self.api_key.is_some() && self.model.starts_with("claude")
    }

    /// Determine if this config targets an Ollama backend.
    pub fn is_ollama(&self) -> bool {
        !self.is_claude()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AiConfig::default();
        assert_eq!(config.endpoint, "http://localhost:11434");
        assert_eq!(config.model, "qwen2.5-coder:7b");
        assert!(config.api_key.is_none());
        assert_eq!(config.context_lines, 30);
        assert!(config.is_ollama());
        assert!(!config.is_claude());
    }

    #[test]
    fn test_claude_detection() {
        let config = AiConfig {
            api_key: Some("sk-ant-test".to_string()),
            model: "claude-sonnet-4-20250514".to_string(),
            ..Default::default()
        };
        assert!(config.is_claude());
        assert!(!config.is_ollama());
    }
}
