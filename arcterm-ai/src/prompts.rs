//! System prompt templates for AI pane and command overlay.

/// System prompt for the AI pane — conversational terminal assistant.
pub const AI_PANE_SYSTEM_PROMPT: &str = "\
You are a terminal assistant embedded in ArcTerm, a GPU-accelerated terminal emulator. \
You have context from the user's active terminal pane including recent output, \
working directory, and last command with exit code.\n\n\
Be terse. Return shell commands directly when applicable. Prefer one-liners. \
Flag destructive operations (rm -rf, DROP TABLE, force push, etc.) with a \
⚠ DESTRUCTIVE warning before suggesting them. When explaining, keep it short.";

/// System prompt for the command overlay — one-shot command generator.
pub const COMMAND_OVERLAY_SYSTEM_PROMPT: &str = "\
You are a shell command generator. Given a question and terminal context, \
return exactly one shell command. No explanation, no markdown, no backticks. \
Just the command.";

/// Format pane context as a user message for the LLM.
pub fn format_context_message(
    cwd: &str,
    last_command: Option<&str>,
    exit_code: Option<i32>,
    scrollback: &str,
) -> String {
    let mut parts = Vec::new();

    parts.push(format!("Working directory: {}", cwd));

    if let Some(cmd) = last_command {
        parts.push(format!("Last command: {}", cmd));
    }

    if let Some(code) = exit_code {
        parts.push(format!("Exit code: {}", code));
    }

    if !scrollback.is_empty() {
        parts.push(format!("Recent terminal output:\n```\n{}\n```", scrollback));
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_context_full() {
        let msg = format_context_message(
            "/home/user/project",
            Some("cargo build"),
            Some(1),
            "error[E0308]: mismatched types",
        );
        assert!(msg.contains("Working directory: /home/user/project"));
        assert!(msg.contains("Last command: cargo build"));
        assert!(msg.contains("Exit code: 1"));
        assert!(msg.contains("error[E0308]"));
    }

    #[test]
    fn test_format_context_minimal() {
        let msg = format_context_message("/home/user", None, None, "");
        assert!(msg.contains("Working directory: /home/user"));
        assert!(!msg.contains("Last command"));
        assert!(!msg.contains("Exit code"));
        assert!(!msg.contains("```"));
    }

    #[test]
    fn test_system_prompts_not_empty() {
        assert!(!AI_PANE_SYSTEM_PROMPT.is_empty());
        assert!(!COMMAND_OVERLAY_SYSTEM_PROMPT.is_empty());
        assert!(AI_PANE_SYSTEM_PROMPT.contains("DESTRUCTIVE"));
        assert!(COMMAND_OVERLAY_SYSTEM_PROMPT.contains("one shell command"));
    }
}
