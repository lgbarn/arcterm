//! AI configuration types and defaults.

/// Which LLM backend to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendKind {
    Ollama,
    Claude,
}

/// Configuration for the AI subsystem.
#[derive(Debug, Clone)]
pub struct AiConfig {
    /// Which backend to use
    pub backend: BackendKind,
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
            backend: BackendKind::Ollama,
            endpoint: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder:7b".to_string(),
            api_key: None,
            context_lines: 30,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AiConfig::default();
        assert_eq!(config.backend, BackendKind::Ollama);
        assert_eq!(config.endpoint, "http://localhost:11434");
        assert_eq!(config.model, "qwen2.5-coder:7b");
        assert!(config.api_key.is_none());
        assert_eq!(config.context_lines, 30);
    }

    #[test]
    fn test_claude_config() {
        let config = AiConfig {
            backend: BackendKind::Claude,
            api_key: Some("sk-ant-test".to_string()),
            model: "claude-sonnet-4-20250514".to_string(),
            ..Default::default()
        };
        assert_eq!(config.backend, BackendKind::Claude);
    }
}
