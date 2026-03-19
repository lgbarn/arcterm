//! Inline AI suggestion ghost text overlay.
//!
//! Renders dimmed completion text after the cursor when the user
//! is typing at a shell prompt. Uses the CopyOverlay pattern:
//! wraps the real pane via `assign_overlay_for_pane` and overrides
//! `with_lines_mut` to inject ghost text at the cursor position.

// TODO: Full implementation requires:
// 1. Pane overlay wrapper (struct SuggestionOverlay implementing Pane trait)
// 2. Ghost text injection in with_lines_mut (dimmed ColorAttribute)
// 3. Debounce timer via smol::Timer + typing_cookie
// 4. Async LLM query via smol::unblock
// 5. Key table registration (Tab -> accept, Escape -> dismiss)
// 6. Result delivery via window.notify(TermWindowNotif::Apply)
//
// This requires deep integration with WezTerm's Pane trait,
// overlay system, and key table infrastructure. The arcterm-ai
// suggestion logic (prompt detection, query building, response
// cleaning) is complete and tested in arcterm-ai/src/suggestions.rs.

/// Placeholder for the suggestion overlay state.
pub struct SuggestionState {
    /// Current suggestion text (None = no suggestion visible)
    pub suggestion: Option<String>,
    /// Monotone counter for debounce cookie matching
    pub typing_cookie: u64,
    /// Whether the feature is enabled
    pub enabled: bool,
}

impl SuggestionState {
    pub fn new(enabled: bool) -> Self {
        Self {
            suggestion: None,
            typing_cookie: 0,
            enabled,
        }
    }

    /// Increment the typing cookie (called on each keystroke)
    pub fn keystroke(&mut self) {
        self.typing_cookie = self.typing_cookie.wrapping_add(1);
        self.suggestion = None; // dismiss current suggestion on new input
    }

    /// Set a new suggestion (called when LLM response arrives)
    pub fn set_suggestion(&mut self, text: String, cookie: u64) {
        // Only accept if cookie matches (input hasn't changed since query)
        if cookie == self.typing_cookie && !text.is_empty() {
            self.suggestion = Some(text);
        }
    }

    /// Accept the current suggestion (returns text to inject)
    pub fn accept(&mut self) -> Option<String> {
        self.suggestion.take()
    }

    /// Dismiss the current suggestion
    pub fn dismiss(&mut self) {
        self.suggestion = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keystroke_dismisses_suggestion() {
        let mut state = SuggestionState::new(true);
        state.suggestion = Some("test".to_string());
        state.keystroke();
        assert!(state.suggestion.is_none());
        assert_eq!(state.typing_cookie, 1);
    }

    #[test]
    fn test_set_suggestion_matching_cookie() {
        let mut state = SuggestionState::new(true);
        let cookie = state.typing_cookie;
        state.set_suggestion("completion".to_string(), cookie);
        assert_eq!(state.suggestion, Some("completion".to_string()));
    }

    #[test]
    fn test_set_suggestion_stale_cookie() {
        let mut state = SuggestionState::new(true);
        let old_cookie = state.typing_cookie;
        state.keystroke(); // cookie advances
        state.set_suggestion("stale".to_string(), old_cookie);
        assert!(state.suggestion.is_none()); // rejected
    }

    #[test]
    fn test_accept_returns_and_clears() {
        let mut state = SuggestionState::new(true);
        state.suggestion = Some("text".to_string());
        let accepted = state.accept();
        assert_eq!(accepted, Some("text".to_string()));
        assert!(state.suggestion.is_none());
    }

    #[test]
    fn test_dismiss_clears() {
        let mut state = SuggestionState::new(true);
        state.suggestion = Some("text".to_string());
        state.dismiss();
        assert!(state.suggestion.is_none());
    }
}
