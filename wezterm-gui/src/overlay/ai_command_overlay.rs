//! AI Command Overlay — floating prompt that generates shell commands via LLM.
//!
//! The overlay collects a natural-language question from the user, sends it to the
//! configured LLM backend (default: Ollama), and displays the generated command.
//! The caller is responsible for pasting the returned command into the active pane.

use arcterm_ai::backend::{create_backend, LlmBackend};
use arcterm_ai::config::AiConfig;
use arcterm_ai::destructive::maybe_warn;
use arcterm_ai::prompts::COMMAND_OVERLAY_SYSTEM_PROMPT;
use mux::termwiztermtab::TermWizTerminal;
use std::io::BufRead;
use termwiz::input::{InputEvent, KeyCode, KeyEvent, Modifiers};
use termwiz::lineedit::*;
use termwiz::surface::Change;
use termwiz::terminal::Terminal;

// ---------------------------------------------------------------------------
// LineEditor host — mirrors the minimal PromptHost in prompt.rs
// ---------------------------------------------------------------------------

struct CommandOverlayHost {
    history: BasicHistory,
}

impl CommandOverlayHost {
    fn new() -> Self {
        Self {
            history: BasicHistory::default(),
        }
    }
}

impl LineEditorHost for CommandOverlayHost {
    fn history(&mut self) -> &mut dyn History {
        &mut self.history
    }

    fn resolve_action(
        &mut self,
        event: &InputEvent,
        editor: &mut LineEditor<'_>,
    ) -> Option<Action> {
        let (line, _cursor) = editor.get_line_and_cursor();
        // Allow Escape to cancel when the line is empty — mirrors prompt.rs.
        if line.is_empty()
            && matches!(
                event,
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Escape,
                    ..
                })
            )
        {
            Some(Action::Cancel)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Show the AI command overlay.
///
/// Renders a header prompt, collects a natural-language query via `LineEditor`,
/// calls the LLM backend, strips markdown formatting, applies a destructive
/// warning if needed, then waits for the user to confirm (Enter) or dismiss
/// (Escape).
///
/// Returns `Ok(Some(command))` when the user confirms, `Ok(None)` when the user
/// dismisses, and `Ok(None)` when the LLM is unavailable.
pub fn show_command_overlay(mut term: TermWizTerminal) -> anyhow::Result<Option<String>> {
    term.no_grab_mouse_in_raw_mode();

    // --- Header ---
    term.render(&[Change::Text(
        "ArcTerm Command Overlay \u{2014} type your question, press Enter\r\n".to_string(),
    )])?;

    // --- Collect query ---
    let mut host = CommandOverlayHost::new();
    let mut editor = LineEditor::new(&mut term);
    editor.set_prompt("> ");
    let query = match editor.read_line(&mut host)? {
        Some(q) if !q.trim().is_empty() => q,
        // User cancelled or submitted empty input — dismiss silently.
        _ => return Ok(None),
    };

    // --- Thinking indicator ---
    term.render(&[Change::Text("\r\nThinking...\r\n".to_string())])?;

    // --- Build backend from default config ---
    let config = AiConfig::default();
    let backend = create_backend(&config);

    if !backend.is_available() {
        term.render(&[Change::Text(
            format!("LLM unavailable — {} is not reachable\r\n", backend.name()),
        )])?;
        return Ok(None);
    }

    // --- Generate command ---
    let reader = match backend.generate(&query, COMMAND_OVERLAY_SYSTEM_PROMPT) {
        Ok(r) => r,
        Err(err) => {
            term.render(&[Change::Text(format!(
                "LLM unavailable: {}\r\n",
                err
            ))])?;
            return Ok(None);
        }
    };

    // Collect NDJSON streaming tokens into a single string.
    let raw = collect_streaming_response(reader);

    // --- Strip markdown formatting (backticks, code fences) ---
    let command = strip_markdown(&raw);

    // --- Destructive check (uses shared warning format) ---
    let display = maybe_warn(&command);

    // --- Render result ---
    term.render(&[Change::Text(format!("\r\n{}\r\n", display))])?;
    term.render(&[Change::Text(
        "\r\nPress Enter to paste, Escape to dismiss\r\n".to_string(),
    )])?;

    // --- Wait for confirmation ---
    loop {
        match term.poll_input(None)? {
            Some(InputEvent::Key(KeyEvent {
                key: KeyCode::Enter,
                modifiers: Modifiers::NONE,
            })) => return Ok(Some(command)),
            Some(InputEvent::Key(KeyEvent {
                key: KeyCode::Escape,
                ..
            })) => return Ok(None),
            // Ignore all other events (mouse, resize, etc.).
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read all NDJSON lines from the Ollama streaming response and concatenate
/// the `message.content` tokens into a single string.
///
/// Each line is a JSON object of the form:
/// `{"model":"...","message":{"role":"assistant","content":"..."},"done":false}`
///
/// Lines that fail to parse (empty lines, non-JSON) are silently skipped.
fn collect_streaming_response(reader: Box<dyn std::io::Read + Send>) -> String {
    let buf = std::io::BufReader::new(reader);
    let mut result = String::new();

    for line in buf.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(token) = value
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
            {
                result.push_str(token);
            }
        }
    }

    result.trim().to_string()
}

/// Remove markdown formatting that an LLM sometimes emits despite being told
/// not to: triple backtick fences and inline backticks.
fn strip_markdown(input: &str) -> String {
    // Remove triple-backtick code fences (``` ... ```)
    let without_fences = {
        let mut s = input.to_string();
        // Strip opening fence (optionally with a language tag, e.g. ```bash)
        if let Some(pos) = s.find("```") {
            let after_open = pos + 3;
            // Skip the optional language identifier on the same line.
            let after_lang = s[after_open..]
                .find('\n')
                .map(|n| after_open + n + 1)
                .unwrap_or(after_open);
            // Find the closing fence.
            if let Some(close) = s[after_lang..].find("```") {
                s = s[after_lang..after_lang + close].trim().to_string();
            } else {
                s = s[after_lang..].trim().to_string();
            }
        }
        s
    };

    // Strip surrounding inline backticks.
    let trimmed = without_fences.trim();
    if trimmed.starts_with('`') && trimmed.ends_with('`') && trimmed.len() > 1 {
        trimmed[1..trimmed.len() - 1].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_markdown_plain() {
        assert_eq!(strip_markdown("ls -la"), "ls -la");
    }

    #[test]
    fn test_strip_markdown_inline_backticks() {
        assert_eq!(strip_markdown("`ls -la`"), "ls -la");
    }

    #[test]
    fn test_strip_markdown_triple_fence_no_lang() {
        let input = "```\nls -la\n```";
        assert_eq!(strip_markdown(input), "ls -la");
    }

    #[test]
    fn test_strip_markdown_triple_fence_with_lang() {
        let input = "```bash\nls -la\n```";
        assert_eq!(strip_markdown(input), "ls -la");
    }

    #[test]
    fn test_strip_markdown_whitespace() {
        assert_eq!(strip_markdown("  ls -la  "), "ls -la");
    }

    #[test]
    fn test_collect_streaming_response_valid_ndjson() {
        let ndjson = r#"{"message":{"role":"assistant","content":"ls"},"done":false}
{"message":{"role":"assistant","content":" -la"},"done":true}
"#;
        let reader: Box<dyn std::io::Read + Send> =
            Box::new(std::io::Cursor::new(ndjson.as_bytes().to_vec()));
        let result = collect_streaming_response(reader);
        assert_eq!(result, "ls -la");
    }

    #[test]
    fn test_collect_streaming_response_skips_bad_lines() {
        let ndjson = "not json\n{\"message\":{\"role\":\"assistant\",\"content\":\"pwd\"},\"done\":true}\n";
        let reader: Box<dyn std::io::Read + Send> =
            Box::new(std::io::Cursor::new(ndjson.as_bytes().to_vec()));
        let result = collect_streaming_response(reader);
        assert_eq!(result, "pwd");
    }

    #[test]
    fn test_collect_streaming_response_empty() {
        let reader: Box<dyn std::io::Read + Send> =
            Box::new(std::io::Cursor::new(Vec::<u8>::new()));
        let result = collect_streaming_response(reader);
        assert_eq!(result, "");
    }
}
