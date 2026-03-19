//! Inline AI suggestion ghost text overlay.
//!
//! Renders dimmed completion text after the cursor when the user
//! is typing at a shell prompt. Uses the CopyOverlay pattern:
//! wraps the real pane via `assign_overlay_for_pane` and overrides
//! `with_lines_mut` to inject ghost text at the cursor position.
//!
//! The full Pane overlay wrapper requires deep integration with
//! WezTerm's Pane trait and overlay system. The state management
//! and logic below is complete and tested; the GUI wiring is
//! documented as TODO integration points.

use arcterm_ai::suggestions::{self, SuggestionConfig};

/// Keys that should dismiss the current suggestion.
const DISMISS_KEYS: &[&str] = &[
    "Escape", "Enter", "Left", "Right", "Up", "Down", "Home", "End",
    "PageUp", "PageDown",
];

/// Full state for the inline suggestion system.
pub struct SuggestionState {
    /// Current suggestion text (None = no suggestion visible)
    pub suggestion: Option<String>,
    /// Monotone counter for debounce cookie matching
    pub typing_cookie: u64,
    /// Whether the feature is enabled
    pub enabled: bool,
    /// Configuration
    pub config: SuggestionConfig,
    /// Whether we're currently waiting for an LLM response
    pub querying: bool,
}

impl SuggestionState {
    pub fn new(config: SuggestionConfig) -> Self {
        let enabled = config.enabled;
        Self {
            suggestion: None,
            typing_cookie: 0,
            enabled,
            config,
            querying: false,
        }
    }

    /// Increment the typing cookie (called on each keystroke).
    /// Dismisses current suggestion and invalidates pending queries.
    pub fn keystroke(&mut self) {
        self.typing_cookie = self.typing_cookie.wrapping_add(1);
        self.suggestion = None;
    }

    /// Check if a key name should dismiss the suggestion.
    pub fn should_dismiss_key(key_name: &str) -> bool {
        DISMISS_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key_name))
    }

    /// Check if Tab should accept (only when suggestion is visible).
    pub fn should_accept_tab(&self) -> bool {
        self.suggestion.is_some()
    }

    /// Set a new suggestion (called when LLM response arrives).
    /// Only accepts if cookie matches (input hasn't changed since query).
    pub fn set_suggestion(&mut self, text: String, cookie: u64) {
        self.querying = false;
        if cookie == self.typing_cookie && !text.is_empty() {
            self.suggestion = Some(text);
        }
    }

    /// Accept the current suggestion (returns text to inject into shell).
    pub fn accept(&mut self) -> Option<String> {
        self.suggestion.take()
    }

    /// Dismiss the current suggestion.
    pub fn dismiss(&mut self) {
        self.suggestion = None;
    }

    /// Check if prompt detection says we're at a shell prompt.
    pub fn check_prompt(
        &self,
        semantic_zones: &[(std::ops::Range<usize>, String)],
        cursor_row: usize,
        total_rows: usize,
        foreground_process: Option<&str>,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        suggestions::is_at_shell_prompt(
            semantic_zones,
            cursor_row,
            total_rows,
            foreground_process,
        )
    }

    /// Build the query for the current partial command.
    pub fn build_query(
        &self,
        partial_cmd: &str,
        context: &arcterm_ai::context::PaneContext,
    ) -> Vec<arcterm_ai::backend::Message> {
        suggestions::build_suggestion_query(partial_cmd, context)
    }

    /// Clean an LLM response into a usable suggestion.
    pub fn clean_response(&self, response: &str, partial_cmd: &str) -> String {
        suggestions::clean_suggestion(response, partial_cmd)
    }

    /// Mark that a query is in flight.
    pub fn start_query(&mut self) {
        self.querying = true;
    }

    /// Get the current debounce cookie for query matching.
    pub fn current_cookie(&self) -> u64 {
        self.typing_cookie
    }
}

// ── Integration Notes ──────────────────────────────────────────────────────
//
// To complete the GUI wiring, the following integration points need
// implementation:
//
// 1. GHOST TEXT RENDERING (assign_overlay_for_pane):
//    Create a struct implementing the Pane trait that wraps the real pane.
//    Override with_lines_mut to inject suggestion text with dimmed
//    ColorAttribute at the cursor position.
//
// 2. DEBOUNCE TIMER:
//    On each keystroke in the active pane:
//      state.keystroke();
//      let cookie = state.current_cookie();
//      smol::spawn(async move {
//          smol::Timer::after(Duration::from_millis(config.debounce_ms)).await;
//          if cookie == state.current_cookie() {
//              // Query LLM via smol::unblock
//          }
//      });
//
// 3. KEY TABLE:
//    When suggestion is visible, push "ai_suggestion" key table:
//      Tab -> AcceptAiSuggestion
//      Escape -> DismissAiSuggestion
//    When dismissed, pop the key table.
//
// 4. ACCEPT ACTION:
//    On AcceptAiSuggestion:
//      if let Some(text) = state.accept() {
//          pane.send_text(&text);
//      }
//      cancel_overlay_for_pane();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keystroke_dismisses_and_increments() {
        let mut state = SuggestionState::new(SuggestionConfig::default());
        state.suggestion = Some("test".to_string());
        state.keystroke();
        assert!(state.suggestion.is_none());
        assert_eq!(state.typing_cookie, 1);
    }

    #[test]
    fn test_set_suggestion_matching_cookie() {
        let mut state = SuggestionState::new(SuggestionConfig::default());
        let cookie = state.current_cookie();
        state.set_suggestion("completion".to_string(), cookie);
        assert_eq!(state.suggestion, Some("completion".to_string()));
        assert!(!state.querying);
    }

    #[test]
    fn test_set_suggestion_stale_cookie_rejected() {
        let mut state = SuggestionState::new(SuggestionConfig::default());
        let old_cookie = state.current_cookie();
        state.keystroke();
        state.set_suggestion("stale".to_string(), old_cookie);
        assert!(state.suggestion.is_none());
    }

    #[test]
    fn test_accept_returns_and_clears() {
        let mut state = SuggestionState::new(SuggestionConfig::default());
        state.suggestion = Some("text".to_string());
        assert!(state.should_accept_tab());
        let accepted = state.accept();
        assert_eq!(accepted, Some("text".to_string()));
        assert!(state.suggestion.is_none());
        assert!(!state.should_accept_tab());
    }

    #[test]
    fn test_dismiss_clears() {
        let mut state = SuggestionState::new(SuggestionConfig::default());
        state.suggestion = Some("text".to_string());
        state.dismiss();
        assert!(state.suggestion.is_none());
    }

    #[test]
    fn test_should_dismiss_keys() {
        assert!(SuggestionState::should_dismiss_key("Escape"));
        assert!(SuggestionState::should_dismiss_key("Enter"));
        assert!(SuggestionState::should_dismiss_key("Left"));
        assert!(SuggestionState::should_dismiss_key("Right"));
        assert!(!SuggestionState::should_dismiss_key("a"));
        assert!(!SuggestionState::should_dismiss_key("Tab"));
    }

    #[test]
    fn test_tab_only_accepts_when_suggestion_visible() {
        let mut state = SuggestionState::new(SuggestionConfig::default());
        assert!(!state.should_accept_tab()); // no suggestion
        state.suggestion = Some("test".to_string());
        assert!(state.should_accept_tab()); // suggestion visible
    }

    #[test]
    fn test_disabled_state_blocks_prompt_check() {
        let mut config = SuggestionConfig::default();
        config.enabled = false;
        let state = SuggestionState::new(config);
        // Even with a valid input zone, disabled returns false
        let zones = vec![(5..10, "Input".to_string())];
        assert!(!state.check_prompt(&zones, 7, 24, None));
    }

    #[test]
    fn test_query_lifecycle() {
        let mut state = SuggestionState::new(SuggestionConfig::default());
        assert!(!state.querying);
        state.start_query();
        assert!(state.querying);
        state.set_suggestion("result".to_string(), state.current_cookie());
        assert!(!state.querying);
    }
}
