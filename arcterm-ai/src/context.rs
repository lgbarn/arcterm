//! Pane context extraction for AI queries.

/// A snapshot of a terminal pane's state.
#[derive(Debug, Clone)]
pub struct PaneContext {
    /// Last N lines of terminal scrollback output.
    pub scrollback: String,
    /// Current working directory of the pane's process.
    pub cwd: String,
    /// The foreground process name (approximation of "last command").
    pub foreground_process: Option<String>,
    /// Pane dimensions (rows, columns).
    pub dimensions: (u32, u32),
}

impl PaneContext {
    /// Create an empty context (when no sibling pane is available).
    pub fn empty() -> Self {
        Self {
            scrollback: String::new(),
            cwd: String::new(),
            foreground_process: None,
            dimensions: (0, 0),
        }
    }

    /// Check if this context has meaningful content.
    pub fn has_content(&self) -> bool {
        !self.scrollback.is_empty() || !self.cwd.is_empty()
    }

    /// Format this context as an LLM message using the prompts module.
    pub fn format_for_llm(&self) -> String {
        crate::prompts::format_context_message(
            &self.cwd,
            self.foreground_process.as_deref(),
            None, // exit code not yet available via Pane trait
            &self.scrollback,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_context() {
        let ctx = PaneContext::empty();
        assert!(!ctx.has_content());
    }

    #[test]
    fn test_context_with_content() {
        let ctx = PaneContext {
            scrollback: "some output".to_string(),
            cwd: "/home/user".to_string(),
            foreground_process: Some("cargo".to_string()),
            dimensions: (24, 80),
        };
        assert!(ctx.has_content());
    }

    #[test]
    fn test_format_for_llm() {
        let ctx = PaneContext {
            scrollback: "error: compilation failed".to_string(),
            cwd: "/home/user/project".to_string(),
            foreground_process: Some("cargo build".to_string()),
            dimensions: (24, 80),
        };
        let formatted = ctx.format_for_llm();
        assert!(formatted.contains("Working directory: /home/user/project"));
        assert!(formatted.contains("cargo build"));
        assert!(formatted.contains("error: compilation failed"));
    }
}
