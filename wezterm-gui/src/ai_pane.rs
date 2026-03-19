//! AI pane for ArcTerm — conversational AI assistant as a split pane.
//!
//! This module provides `open_ai_pane`, which runs an interactive event loop
//! inside a `TermWizTerminal`.  It connects to the configured LLM backend
//! (default: Ollama at localhost:11434), maintains conversation history, and
//! streams tokens back to the terminal as they arrive.

use arcterm_ai::backend::{create_backend, Message};
use arcterm_ai::config::AiConfig;
use arcterm_ai::destructive;
use arcterm_ai::prompts::AI_PANE_SYSTEM_PROMPT;
use mux::termwiztermtab::TermWizTerminal;
use std::io::{BufRead, BufReader};
use std::time::Duration;
use termwiz::input::{InputEvent, KeyCode, KeyEvent, Modifiers};
use termwiz::surface::Change;
use termwiz::terminal::Terminal;

/// Open and run the AI assistant pane.
///
/// This function owns the `TermWizTerminal` for its lifetime.  It blocks the
/// calling thread (which is a dedicated TermWiz thread spawned by
/// `mux::termwiztermtab::run`) until the user exits with Escape or Ctrl-C.
pub fn open_ai_pane(mut term: TermWizTerminal) -> anyhow::Result<()> {
    let config = AiConfig::default();
    let backend = create_backend(&config);

    // --- Availability check --------------------------------------------------
    if !backend.is_available() {
        term.render(&[Change::Text(
            "\u{26a0} LLM unavailable \u{2014} is Ollama running at localhost:11434?\n\
             Press Enter to retry or Escape / Ctrl-C to quit.\n"
                .to_string(),
        )])?;

        // Wait for retry or quit
        loop {
            match term.poll_input(Some(Duration::from_millis(200))) {
                Ok(Some(InputEvent::Key(KeyEvent {
                    key: KeyCode::Enter,
                    ..
                }))) => {
                    if backend.is_available() {
                        break;
                    }
                    term.render(&[Change::Text(
                        "Still unavailable. Press Enter to retry or Escape / Ctrl-C to quit.\n"
                            .to_string(),
                    )])?;
                }
                Ok(Some(InputEvent::Key(KeyEvent {
                    key: KeyCode::Escape,
                    ..
                }))) => return Ok(()),
                Ok(Some(InputEvent::Key(KeyEvent {
                    key: KeyCode::Char('C'),
                    modifiers: Modifiers::CTRL,
                    ..
                }))) => return Ok(()),
                _ => {}
            }
        }
    }

    // --- Welcome message -----------------------------------------------------
    term.render(&[Change::Text(format!(
        "ArcTerm AI Assistant (model: {})\nType your question and press Enter.\n\n> ",
        config.model
    ))])?;

    // --- Conversation state --------------------------------------------------
    const MAX_HISTORY_PAIRS: usize = 20;
    let mut history: Vec<Message> = Vec::new();
    let mut input_buf = String::new();

    // --- Event loop ----------------------------------------------------------
    loop {
        match term.poll_input(Some(Duration::from_millis(50))) {
            // --- Printable character: accumulate into input buffer -----------
            Ok(Some(InputEvent::Key(KeyEvent {
                key: KeyCode::Char(c),
                modifiers: Modifiers::NONE,
                ..
            }))) => {
                input_buf.push(c);
                term.render(&[Change::Text(c.to_string())])?;
            }

            // --- Backspace: remove last character ---------------------------
            Ok(Some(InputEvent::Key(KeyEvent {
                key: KeyCode::Backspace,
                ..
            }))) => {
                if input_buf.pop().is_some() {
                    // Move cursor left, overwrite with space, move left again
                    term.render(&[Change::Text("\x08 \x08".to_string())])?;
                }
            }

            // --- Enter: send to LLM -----------------------------------------
            Ok(Some(InputEvent::Key(KeyEvent {
                key: KeyCode::Enter,
                ..
            }))) => {
                let user_text = input_buf.trim().to_string();
                input_buf.clear();
                term.render(&[Change::Text("\n".to_string())])?;

                if user_text.is_empty() {
                    term.render(&[Change::Text("> ".to_string())])?;
                    continue;
                }

                // Build full messages list: system + history + new user turn
                let mut messages: Vec<Message> =
                    Vec::with_capacity(1 + history.len() + 1);
                messages.push(Message::system(AI_PANE_SYSTEM_PROMPT));
                messages.extend(history.iter().cloned());
                messages.push(Message::user(&user_text));

                // Call the backend and stream the response
                let response_text = match stream_response(&mut term, &messages, &*backend) {
                    Ok(text) => text,
                    Err(err) => {
                        let msg = if is_connection_error(&err) {
                            "[Connection lost]\n".to_string()
                        } else {
                            format!("[Error: {}]\n", err)
                        };
                        term.render(&[Change::Text(msg)])?;
                        String::new()
                    }
                };

                // Scan response for destructive commands and warn
                if !response_text.is_empty() && destructive::is_destructive(&response_text) {
                    term.render(&[Change::Text(
                        "\n\x1b[1;31m⚠ DESTRUCTIVE COMMAND detected in response above\x1b[0m\n".to_string(),
                    )])?;
                }

                // Commit both turns to history
                history.push(Message::user(user_text));
                if !response_text.is_empty() {
                    history.push(Message::assistant(response_text));
                }

                // Cap history to prevent context window overflow
                // Each pair is 2 entries (user + assistant)
                while history.len() > MAX_HISTORY_PAIRS * 2 {
                    history.remove(0);
                }

                term.render(&[Change::Text("\n\n> ".to_string())])?;
            }

            // --- Escape: quit -----------------------------------------------
            Ok(Some(InputEvent::Key(KeyEvent {
                key: KeyCode::Escape,
                ..
            }))) => break,

            // --- Ctrl-C: quit -----------------------------------------------
            Ok(Some(InputEvent::Key(KeyEvent {
                key: KeyCode::Char('C'),
                modifiers: Modifiers::CTRL,
                ..
            }))) => break,

            // --- Ignore everything else (resize events, mouse, etc.) --------
            _ => {}
        }
    }

    Ok(())
}

/// Stream a response from the LLM backend, rendering each token to the
/// terminal as it arrives.  Returns the full concatenated response text.
fn stream_response(
    term: &mut TermWizTerminal,
    messages: &[Message],
    backend: &dyn arcterm_ai::backend::LlmBackend,
) -> anyhow::Result<String> {
    let reader = backend.chat(messages)?;
    let buf_reader = BufReader::new(reader);
    let mut full_response = String::new();

    for line in buf_reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(err) => {
                log::warn!("ai_pane: error reading stream line: {}", err);
                break;
            }
        };

        if line.is_empty() {
            continue;
        }

        // Parse the NDJSON line from Ollama:
        // {"model":"...","message":{"role":"assistant","content":"token"},"done":false}
        match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(json) => {
                // Check for "done" signal
                let done = json.get("done").and_then(|v| v.as_bool()).unwrap_or(false);

                // Extract content token
                if let Some(token) = json
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                {
                    if !token.is_empty() {
                        term.render(&[Change::Text(token.to_string())])?;
                        full_response.push_str(token);
                    }
                }

                if done {
                    break;
                }
            }
            Err(err) => {
                log::warn!("ai_pane: malformed NDJSON line ({}): {:?}", err, line);
                // Continue — do not abort the session for one bad line
            }
        }
    }

    Ok(full_response)
}

/// Returns `true` when `err` looks like a connection-refused / IO error.
fn is_connection_error(err: &anyhow::Error) -> bool {
    let msg = format!("{}", err);
    msg.contains("Connection refused")
        || msg.contains("connection refused")
        || msg.contains("Failed to connect")
        || msg.contains("Ollama request failed")
}
