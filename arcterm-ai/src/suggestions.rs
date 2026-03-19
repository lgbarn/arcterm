//! Inline AI command suggestion logic.
//!
//! Provides prompt detection, query building, and response cleaning
//! for the ghost-text suggestion feature.

use crate::backend::Message;
use crate::context::PaneContext;

/// System prompt for command completion — returns only the remaining characters.
const COMPLETION_SYSTEM_PROMPT: &str = "\
Given a partial shell command and terminal context, return ONLY the completion \
text that should follow the cursor. No explanation, no backticks, no newlines, \
no repeat of what was already typed. Just the remaining characters to complete \
the command.";

/// Configuration for the suggestion system.
#[derive(Debug, Clone)]
pub struct SuggestionConfig {
    /// Master toggle for suggestions.
    pub enabled: bool,
    /// Debounce delay in milliseconds before querying the LLM.
    pub debounce_ms: u32,
    /// Key to accept suggestions (default: "Tab").
    pub accept_key: String,
    /// Number of scrollback lines to include as context.
    pub context_lines: u32,
}

impl Default for SuggestionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            debounce_ms: 300,
            accept_key: "Tab".to_string(),
            context_lines: 10,
        }
    }
}

/// Check if the user is at a shell prompt (eligible for suggestions).
///
/// Primary: checks OSC 133 semantic zones for `Input` type at cursor.
/// Fallback: cursor on last row and foreground process looks like a shell.
pub fn is_at_shell_prompt(
    semantic_zones: &[(std::ops::Range<usize>, String)],
    cursor_row: usize,
    total_rows: usize,
    foreground_process: Option<&str>,
) -> bool {
    // Primary: check semantic zones for Input type at cursor
    // SemanticType::Input is set between OSC 133;B and 133;C
    for (range, zone_type) in semantic_zones {
        if zone_type == "Input" && range.contains(&cursor_row) {
            return true;
        }
    }

    // Fallback: heuristic — cursor on last row + shell-like process
    if cursor_row >= total_rows.saturating_sub(2) {
        if let Some(proc) = foreground_process {
            let shell_names = ["bash", "zsh", "fish", "sh", "dash", "tcsh", "ksh", "nu"];
            let proc_lower = proc.to_lowercase();
            return shell_names.iter().any(|s| proc_lower.contains(s));
        }
    }

    false
}

/// Build the LLM messages for a command completion query.
pub fn build_suggestion_query(partial_cmd: &str, context: &PaneContext) -> Vec<Message> {
    let context_msg = context.format_for_llm();
    vec![
        Message::system(COMPLETION_SYSTEM_PROMPT),
        Message::user(format!(
            "{}\n\nPartial command at cursor: {}",
            context_msg, partial_cmd
        )),
    ]
}

/// Clean an LLM response to produce a usable command completion.
///
/// Strips backticks, markdown, leading/trailing whitespace, and removes
/// any prefix that repeats what the user already typed.
pub fn clean_suggestion(response: &str, partial_cmd: &str) -> String {
    let mut cleaned = response.trim().to_string();

    // Strip backtick fences
    if cleaned.starts_with("```") {
        if let Some(end) = cleaned[3..].find("```") {
            let inner = &cleaned[3..3 + end];
            // Skip language tag on first line
            cleaned = inner
                .lines()
                .skip_while(|l| !l.contains(' ') && l.len() < 20) // skip lang tag
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
        }
    }

    // Strip inline backticks
    cleaned = cleaned.trim_matches('`').trim().to_string();

    // Remove newlines — suggestion should be a single line
    if let Some(first_line) = cleaned.lines().next() {
        cleaned = first_line.trim().to_string();
    }

    // Remove prefix that repeats the partial command
    let partial_trimmed = partial_cmd.trim();
    if !partial_trimmed.is_empty() && cleaned.starts_with(partial_trimmed) {
        cleaned = cleaned[partial_trimmed.len()..].to_string();
    }

    // Also check if response starts with just the last word of partial
    if !cleaned.is_empty() {
        if let Some(last_word) = partial_trimmed.split_whitespace().last() {
            if cleaned.starts_with(last_word) && last_word.len() < cleaned.len() {
                cleaned = cleaned[last_word.len()..].to_string();
            }
        }
    }

    cleaned.trim_start().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_at_prompt_with_semantic_zone() {
        let zones = vec![(5..10, "Input".to_string())];
        assert!(is_at_shell_prompt(&zones, 7, 24, None));
        assert!(!is_at_shell_prompt(&zones, 3, 24, None));
    }

    #[test]
    fn test_is_at_prompt_heuristic_fallback() {
        let zones: Vec<(std::ops::Range<usize>, String)> = vec![];
        assert!(is_at_shell_prompt(&zones, 23, 24, Some("zsh")));
        assert!(is_at_shell_prompt(&zones, 23, 24, Some("/bin/bash")));
        assert!(!is_at_shell_prompt(&zones, 23, 24, Some("vim")));
        assert!(!is_at_shell_prompt(&zones, 10, 24, Some("zsh")));
    }

    #[test]
    fn test_build_query_includes_context() {
        let ctx = PaneContext {
            scrollback: "$ cargo build\nerror: expected `;`".to_string(),
            cwd: "/home/user/project".to_string(),
            foreground_process: Some("bash".to_string()),
            dimensions: (24, 80),
        };
        let messages = build_suggestion_query("cargo", &ctx);
        assert_eq!(messages.len(), 2);
        assert!(messages[1].content.contains("cargo"));
        assert!(messages[1].content.contains("/home/user/project"));
    }

    #[test]
    fn test_clean_suggestion_basic() {
        assert_eq!(clean_suggestion(" build --release", "cargo"), "build --release");
    }

    #[test]
    fn test_clean_suggestion_strips_backticks() {
        assert_eq!(clean_suggestion("`build --release`", "cargo"), "build --release");
    }

    #[test]
    fn test_clean_suggestion_removes_repeated_prefix() {
        assert_eq!(clean_suggestion("cargo build", "cargo"), "build");
    }

    #[test]
    fn test_clean_suggestion_multiline_takes_first() {
        assert_eq!(
            clean_suggestion("build --release\nThis builds in release mode", "cargo"),
            "build --release"
        );
    }

    #[test]
    fn test_clean_suggestion_empty() {
        assert_eq!(clean_suggestion("", "git"), "");
    }

    #[test]
    fn test_suggestion_config_defaults() {
        let config = SuggestionConfig::default();
        assert!(config.enabled);
        assert_eq!(config.debounce_ms, 300);
        assert_eq!(config.accept_key, "Tab");
        assert_eq!(config.context_lines, 10);
    }
}
