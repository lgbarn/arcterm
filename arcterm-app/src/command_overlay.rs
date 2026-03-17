//! Command overlay: quick-invoke LLM query for one-shot shell commands.
//!
//! Triggered by Ctrl+Space. User types a question, Ollama returns a single
//! shell command. Enter accepts (pastes into active pane), Escape dismisses.

use winit::keyboard::{Key, NamedKey};

/// Actions produced by the command overlay's key handler.
#[derive(Debug, PartialEq)]
pub enum OverlayAction {
    /// Query string was updated (typing or backspace).
    UpdateQuery,
    /// User pressed Enter while typing — send to Ollama.
    Submit,
    /// User pressed Enter on the result — accept (paste into active pane).
    Accept(String),
    /// User pressed Escape — close the overlay.
    Close,
    /// Key consumed, no state change.
    Noop,
}

/// Which phase the overlay is in.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayPhase {
    /// User is typing their question.
    Input,
    /// Waiting for Ollama response.
    Loading,
    /// Showing the returned command.
    Result(String),
    /// Ollama returned an error.
    Error(String),
}

/// Runtime state for the command overlay.
pub struct CommandOverlayState {
    /// Current query string.
    pub query: String,
    /// Current phase.
    pub phase: OverlayPhase,
}

impl CommandOverlayState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            phase: OverlayPhase::Input,
        }
    }

    /// Handle a key press. Returns the action to take.
    pub fn handle_key(&mut self, logical_key: &Key) -> OverlayAction {
        match &self.phase {
            OverlayPhase::Input => match logical_key {
                Key::Named(NamedKey::Escape) => OverlayAction::Close,
                Key::Named(NamedKey::Enter) => {
                    if self.query.is_empty() {
                        OverlayAction::Noop
                    } else {
                        self.phase = OverlayPhase::Loading;
                        OverlayAction::Submit
                    }
                }
                Key::Named(NamedKey::Backspace) => {
                    self.query.pop();
                    OverlayAction::UpdateQuery
                }
                Key::Character(s) => {
                    self.query.push_str(s.as_str());
                    OverlayAction::UpdateQuery
                }
                _ => OverlayAction::Noop,
            },
            OverlayPhase::Loading => match logical_key {
                Key::Named(NamedKey::Escape) => OverlayAction::Close,
                _ => OverlayAction::Noop,
            },
            OverlayPhase::Result(cmd) => match logical_key {
                Key::Named(NamedKey::Escape) => OverlayAction::Close,
                Key::Named(NamedKey::Enter) => OverlayAction::Accept(cmd.clone()),
                _ => OverlayAction::Noop,
            },
            OverlayPhase::Error(_) => match logical_key {
                Key::Named(NamedKey::Escape) => OverlayAction::Close,
                _ => OverlayAction::Noop,
            },
        }
    }

    /// Set the result from Ollama.
    pub fn set_result(&mut self, command: String) {
        self.phase = OverlayPhase::Result(command);
    }

    /// Set an error message.
    pub fn set_error(&mut self, msg: String) {
        self.phase = OverlayPhase::Error(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_char(s: &str) -> Key {
        Key::Character(s.into())
    }
    fn key_named(k: NamedKey) -> Key {
        Key::Named(k)
    }

    #[test]
    fn new_overlay_starts_in_input_phase() {
        let state = CommandOverlayState::new();
        assert_eq!(state.phase, OverlayPhase::Input);
        assert!(state.query.is_empty());
    }

    #[test]
    fn typing_appends_to_query() {
        let mut state = CommandOverlayState::new();
        assert_eq!(state.handle_key(&key_char("h")), OverlayAction::UpdateQuery);
        assert_eq!(state.handle_key(&key_char("i")), OverlayAction::UpdateQuery);
        assert_eq!(state.query, "hi");
    }

    #[test]
    fn backspace_removes_last_char() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("a"));
        state.handle_key(&key_char("b"));
        state.handle_key(&key_named(NamedKey::Backspace));
        assert_eq!(state.query, "a");
    }

    #[test]
    fn enter_on_empty_query_is_noop() {
        let mut state = CommandOverlayState::new();
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Enter)),
            OverlayAction::Noop
        );
        assert_eq!(state.phase, OverlayPhase::Input);
    }

    #[test]
    fn enter_with_query_submits_and_transitions_to_loading() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("l"));
        state.handle_key(&key_char("s"));
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Enter)),
            OverlayAction::Submit
        );
        assert_eq!(state.phase, OverlayPhase::Loading);
    }

    #[test]
    fn escape_in_input_closes() {
        let mut state = CommandOverlayState::new();
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Escape)),
            OverlayAction::Close
        );
    }

    #[test]
    fn escape_in_loading_closes() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("x"));
        state.handle_key(&key_named(NamedKey::Enter));
        assert_eq!(state.phase, OverlayPhase::Loading);
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Escape)),
            OverlayAction::Close
        );
    }

    #[test]
    fn set_result_transitions_to_result_phase() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("q"));
        state.handle_key(&key_named(NamedKey::Enter));
        state.set_result("ls -la".to_string());
        assert_eq!(state.phase, OverlayPhase::Result("ls -la".to_string()));
    }

    #[test]
    fn enter_in_result_accepts_command() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("q"));
        state.handle_key(&key_named(NamedKey::Enter));
        state.set_result("ls -la".to_string());
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Enter)),
            OverlayAction::Accept("ls -la".to_string())
        );
    }

    #[test]
    fn escape_in_result_closes() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("q"));
        state.handle_key(&key_named(NamedKey::Enter));
        state.set_result("ls -la".to_string());
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Escape)),
            OverlayAction::Close
        );
    }

    #[test]
    fn set_error_transitions_to_error_phase() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("q"));
        state.handle_key(&key_named(NamedKey::Enter));
        state.set_error("connection refused".to_string());
        assert_eq!(
            state.phase,
            OverlayPhase::Error("connection refused".to_string())
        );
    }

    #[test]
    fn escape_in_error_closes() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("q"));
        state.handle_key(&key_named(NamedKey::Enter));
        state.set_error("timeout".to_string());
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Escape)),
            OverlayAction::Close
        );
    }
}
