//! AI pane: persistent LLM chat session with sibling context awareness.
//!
//! The AI pane maintains a conversation history and injects sibling pane
//! context (scrollback, CWD, last command) into the system prompt.

use crate::ollama::ChatMessage;

/// System prompt for the AI pane.
pub const SYSTEM_PROMPT: &str = "\
You are a terminal assistant embedded in a GPU-accelerated terminal emulator. \
The user is a DevOps engineer. You have context from their active terminal pane \
including recent output, working directory, and last command with exit code.\n\n\
Be terse. Return shell commands directly when applicable. Prefer one-liners. \
Flag destructive operations (rm -rf, DROP TABLE, force push, etc.) before \
suggesting them. When explaining, keep it short.";

/// Per-pane AI chat state.
pub struct AiPaneState {
    /// Full conversation history (system + user + assistant messages).
    pub history: Vec<ChatMessage>,
    /// Whether a response is currently being streamed.
    pub streaming: bool,
    /// Accumulated response text for the current streaming response.
    pub pending_response: String,
    /// Current user input buffer (typed characters not yet submitted).
    pub input_buffer: String,
}

impl AiPaneState {
    /// Create a new AI pane state with the system prompt.
    pub fn new() -> Self {
        Self {
            history: vec![ChatMessage {
                role: "system".to_string(),
                content: SYSTEM_PROMPT.to_string(),
            }],
            streaming: false,
            pending_response: String::new(),
            input_buffer: String::new(),
        }
    }

    /// Inject sibling pane context into the conversation as a system message.
    pub fn inject_context(
        &mut self,
        cwd: Option<&str>,
        last_cmd: Option<&str>,
        exit_code: Option<i32>,
        scrollback: &[String],
    ) {
        let mut parts = Vec::new();
        if let Some(cwd) = cwd {
            parts.push(format!("CWD: {cwd}"));
        }
        if let Some(cmd) = last_cmd {
            parts.push(format!("Last command: {cmd}"));
        }
        if let Some(code) = exit_code {
            parts.push(format!("Exit code: {code}"));
        }
        if !scrollback.is_empty() {
            let joined = scrollback.join("\n");
            parts.push(format!(
                "Terminal output (last {} lines):\n{joined}",
                scrollback.len()
            ));
        }
        if !parts.is_empty() {
            self.history.push(ChatMessage {
                role: "system".to_string(),
                content: format!("[Context from sibling pane]\n{}", parts.join("\n")),
            });
        }
    }

    /// Add a user message to the history.
    pub fn add_user_message(&mut self, content: String) {
        self.history.push(ChatMessage {
            role: "user".to_string(),
            content,
        });
        self.streaming = true;
        self.pending_response.clear();
    }

    /// Append a chunk of streamed response text.
    pub fn append_response_chunk(&mut self, chunk: &str) {
        self.pending_response.push_str(chunk);
    }

    /// Finalize the current streaming response.
    pub fn finalize_response(&mut self) {
        self.history.push(ChatMessage {
            role: "assistant".to_string(),
            content: self.pending_response.clone(),
        });
        self.streaming = false;
        self.pending_response.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_has_system_prompt() {
        let state = AiPaneState::new();
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].role, "system");
        assert!(state.history[0].content.contains("terminal assistant"));
    }

    #[test]
    fn inject_context_adds_system_message() {
        let mut state = AiPaneState::new();
        state.inject_context(
            Some("/home/user/project"),
            Some("cargo build"),
            Some(1),
            &["error[E0308]: type mismatch".to_string()],
        );
        assert_eq!(state.history.len(), 2);
        assert_eq!(state.history[1].role, "system");
        assert!(state.history[1].content.contains("CWD: /home/user/project"));
        assert!(state.history[1].content.contains("cargo build"));
        assert!(state.history[1].content.contains("Exit code: 1"));
        assert!(state.history[1].content.contains("error[E0308]"));
    }

    #[test]
    fn inject_empty_context_does_nothing() {
        let mut state = AiPaneState::new();
        state.inject_context(None, None, None, &[]);
        assert_eq!(state.history.len(), 1); // only system prompt
    }

    #[test]
    fn user_message_and_streaming_lifecycle() {
        let mut state = AiPaneState::new();
        state.add_user_message("what failed?".to_string());
        assert_eq!(state.history.len(), 2);
        assert!(state.streaming);

        state.append_response_chunk("The build ");
        state.append_response_chunk("failed because...");
        assert_eq!(state.pending_response, "The build failed because...");

        state.finalize_response();
        assert!(!state.streaming);
        assert_eq!(state.history.len(), 3);
        assert_eq!(state.history[2].role, "assistant");
        assert_eq!(state.history[2].content, "The build failed because...");
        assert!(state.pending_response.is_empty());
    }
}
