//! Leader-key state machine and pane-navigation keybindings.
//!
//! # Design
//!
//! [`KeymapHandler`] sits between the winit keyboard events and PTY writes.
//! It maintains a two-state machine:
//!
//! - **Normal** — most keys are forwarded to the PTY as raw bytes.
//! - **LeaderPending** — the leader chord (Ctrl+a) was pressed; the *next*
//!   key selects a multiplexer action.
//!
//! The handler is intentionally free of I/O side-effects; callers inspect the
//! returned [`KeyAction`] and perform the actual work.

use std::time::{Duration, Instant};

use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::input::translate_key_event;
use crate::layout::{Axis, Direction};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// State of the leader-key state machine.
#[derive(Debug, Clone)]
pub enum KeymapState {
    /// Normal typing mode — keys are forwarded to the PTY.
    Normal,
    /// The leader chord was received; waiting for the action key.
    LeaderPending {
        /// Wall-clock time at which the leader was entered.
        entered_at: Instant,
    },
}

/// Action produced by [`KeymapHandler::handle_key`].
#[derive(Debug, Clone, PartialEq)]
pub enum KeyAction {
    /// Forward raw bytes to the PTY.
    Forward(Vec<u8>),
    /// Move focus to the pane in the given direction.
    NavigatePane(Direction),
    /// Split the active pane along the given axis.
    Split(Axis),
    /// Close the active pane.
    ClosePane,
    /// Toggle zoom (maximise/restore) on the active pane.
    ToggleZoom,
    /// Resize the active pane in the given direction.
    ResizePane(Direction),
    /// Open a new tab.
    NewTab,
    /// Switch to the 1-indexed tab number (1–9).
    SwitchTab(usize),
    /// Close the active tab.
    CloseTab,
    /// Open the workspace switcher overlay (Leader+w).
    OpenWorkspaceSwitcher,
    /// Save the current session to a timestamped workspace file (Leader+s).
    SaveWorkspace,
    /// Open the command palette.
    OpenPalette,
    /// Jump to the most recently active AI pane (Leader+a).
    JumpToAiPane,
    /// Toggle the plan status strip / expanded plan view (Leader+p).
    TogglePlanView,
    /// Open the config overlay review (Leader+o).
    ReviewOverlay,
    /// Open cross-pane search (Leader+/).
    CrossPaneSearch,
    /// Open the command overlay (Ctrl+Space).
    OpenCommandOverlay,
    /// Open a new AI chat pane (Leader+i).
    OpenAiPane,
    /// Refresh sibling context in the active AI pane (Leader+c).
    RefreshAiContext,
    // ---- Menu-only actions (no leader-key binding) ----
    /// Spawn a new arcterm window as a separate OS process (Cmd+N).
    NewWindow,
    /// Copy selected text to clipboard (Cmd+C).
    Copy,
    /// Paste from clipboard (Cmd+V).
    Paste,
    /// Select all text in the active pane's scrollback + visible buffer.
    SelectAll,
    /// Navigate to the next search match (Cmd+G).
    SearchNext,
    /// Navigate to the previous search match (Cmd+Shift+G).
    SearchPrevious,
    /// Clear the scrollback buffer of the active pane (Cmd+K).
    ClearScrollback,
    /// Increase font size by 1pt (Cmd+=).
    IncreaseFontSize,
    /// Decrease font size by 1pt (Cmd+-).
    DecreaseFontSize,
    /// Reset font size to config default (Cmd+0).
    ResetFontSize,
    /// Toggle native fullscreen (Ctrl+Cmd+F).
    ToggleFullScreen,
    /// Minimize the window (Cmd+M).
    Minimize,
    /// Reset all split ratios to 0.5.
    EqualizeSplits,
    /// Next tab (Cmd+Shift+]).
    NextTab,
    /// Previous tab (Cmd+Shift+[).
    PreviousTab,
    /// Reset terminal emulation state.
    ResetTerminal,
    /// Show debug info overlay (version, GPU, config path, pane count).
    ShowDebugInfo,
    /// Open Arcterm help URL in browser.
    OpenHelp,
    /// Open GitHub issues URL in browser.
    ReportIssue,
    /// Quit the application (Cmd+Q). Saves session before exiting.
    Quit,
    /// The key was consumed by the state machine (no PTY bytes).
    Consumed,
}

/// Leader-key state machine.
pub struct KeymapHandler {
    pub state: KeymapState,
    /// How long (ms) to wait for an action key after the leader.
    pub leader_timeout_ms: u64,
}

impl KeymapHandler {
    /// Create a new [`KeymapHandler`] with the given leader timeout.
    pub fn new(leader_timeout_ms: u64) -> Self {
        Self {
            state: KeymapState::Normal,
            leader_timeout_ms,
        }
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Process a winit [`KeyEvent`] (assumed pressed) and return the action.
    ///
    /// `modifiers` — current keyboard modifier state.
    /// `app_cursor_keys` — forwarded to the byte-translation layer.
    pub fn handle_key(
        &mut self,
        event: &KeyEvent,
        modifiers: ModifiersState,
        app_cursor_keys: bool,
    ) -> KeyAction {
        self.handle_key_with_time(event, modifiers, app_cursor_keys, Instant::now())
    }

    /// Like [`handle_key`] but accepts an explicit timestamp for deterministic
    /// testing of timeout behaviour.
    pub fn handle_key_with_time(
        &mut self,
        event: &KeyEvent,
        modifiers: ModifiersState,
        app_cursor_keys: bool,
        now: Instant,
    ) -> KeyAction {
        self.handle_logical_key_with_time(
            &event.logical_key.clone(),
            modifiers,
            app_cursor_keys,
            now,
            Some(event),
        )
    }

    /// Returns `true` when the state machine is currently in `LeaderPending`.
    #[allow(dead_code)]
    pub fn is_leader_pending(&self) -> bool {
        matches!(self.state, KeymapState::LeaderPending { .. })
    }

    // -----------------------------------------------------------------------
    // Core state-machine logic
    // -----------------------------------------------------------------------

    /// Core state-machine logic, factored out so tests can call it with
    /// synthetic keys without constructing a full [`KeyEvent`].
    ///
    /// `event` is `None` in tests that inject synthetic keys; when `Some` it
    /// is passed to `translate_key_event` so dead-key composition works.
    fn handle_logical_key_with_time(
        &mut self,
        logical_key: &Key,
        modifiers: ModifiersState,
        app_cursor_keys: bool,
        now: Instant,
        event: Option<&KeyEvent>,
    ) -> KeyAction {
        let ctrl = modifiers.control_key();

        match &self.state {
            // ----------------------------------------------------------------
            // Normal mode
            // ----------------------------------------------------------------
            KeymapState::Normal => {
                if ctrl {
                    if let Key::Character(s) = logical_key {
                        let lower = s.as_str().to_ascii_lowercase();
                        match lower.as_str() {
                            // Ctrl+a → enter leader-pending state.
                            "a" => {
                                self.state = KeymapState::LeaderPending { entered_at: now };
                                return KeyAction::Consumed;
                            }
                            // Ctrl+h/j/k/l → navigate panes.
                            "h" => return KeyAction::NavigatePane(Direction::Left),
                            "j" => return KeyAction::NavigatePane(Direction::Down),
                            "k" => return KeyAction::NavigatePane(Direction::Up),
                            "l" => return KeyAction::NavigatePane(Direction::Right),
                            _ => {}
                        }
                    }
                    // Ctrl+Space → open command overlay.
                    if let Key::Named(NamedKey::Space) = logical_key {
                        return KeyAction::OpenCommandOverlay;
                    }
                }

                // All other keys — translate and forward.
                self.forward(logical_key, modifiers, app_cursor_keys, event)
            }

            // ----------------------------------------------------------------
            // LeaderPending mode
            // ----------------------------------------------------------------
            KeymapState::LeaderPending { entered_at } => {
                let elapsed = now.duration_since(*entered_at);
                let timeout = Duration::from_millis(self.leader_timeout_ms);
                let is_expired = elapsed >= timeout;

                if is_expired {
                    // Timeout expired: emit 0x01 for the original Ctrl+a, then
                    // process the new key as if in Normal mode.
                    self.state = KeymapState::Normal;
                    let mut bytes = vec![0x01u8]; // SOH = Ctrl+a
                    // Process the triggering key in Normal mode.
                    let next = self.handle_logical_key_with_time(
                        logical_key,
                        modifiers,
                        app_cursor_keys,
                        now,
                        event,
                    );
                    if let KeyAction::Forward(extra) = next {
                        bytes.extend(extra);
                    }
                    return KeyAction::Forward(bytes);
                }

                // Check for double-tap leader (Ctrl+a again while pending).
                if ctrl
                    && let Key::Character(s) = logical_key
                    && s.as_str().eq_ignore_ascii_case("a")
                {
                    self.state = KeymapState::Normal;
                    return KeyAction::Forward(vec![0x01]);
                }

                // --- Leader action keys ---
                let action = if let Key::Character(s) = logical_key {
                    match s.as_str() {
                        "n" => Some(KeyAction::Split(Axis::Horizontal)),
                        "v" => Some(KeyAction::Split(Axis::Vertical)),
                        "q" => Some(KeyAction::ClosePane),
                        "z" => Some(KeyAction::ToggleZoom),
                        "t" => Some(KeyAction::NewTab),
                        // Leader+a jumps to the most recently active AI pane.
                        "a" => Some(KeyAction::JumpToAiPane),
                        // Leader+p toggles the plan status strip / expanded plan view.
                        "p" => Some(KeyAction::TogglePlanView),
                        // Leader+o opens the config overlay review.
                        "o" => Some(KeyAction::ReviewOverlay),
                        // Leader+/ opens cross-pane search.
                        "/" => Some(KeyAction::CrossPaneSearch),
                        // Leader+w opens the workspace switcher (CONTEXT-5: Leader+w).
                        "w" => Some(KeyAction::OpenWorkspaceSwitcher),
                        // Leader+s saves the current session to a timestamped workspace file.
                        "s" => Some(KeyAction::SaveWorkspace),
                        // Leader+i opens a new AI chat pane.
                        "i" => Some(KeyAction::OpenAiPane),
                        // Leader+c refreshes sibling context in the active AI pane.
                        "c" => Some(KeyAction::RefreshAiContext),
                        // Leader+W (shift) closes the active tab.
                        "W" => Some(KeyAction::CloseTab),
                        d @ ("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9") => {
                            let n = d.parse::<usize>().unwrap();
                            Some(KeyAction::SwitchTab(n))
                        }
                        _ => None,
                    }
                } else if let Key::Named(named) = logical_key {
                    match named {
                        NamedKey::ArrowLeft => Some(KeyAction::ResizePane(Direction::Left)),
                        NamedKey::ArrowRight => Some(KeyAction::ResizePane(Direction::Right)),
                        NamedKey::ArrowUp => Some(KeyAction::ResizePane(Direction::Up)),
                        NamedKey::ArrowDown => Some(KeyAction::ResizePane(Direction::Down)),
                        _ => None,
                    }
                } else {
                    None
                };

                if let Some(a) = action {
                    self.state = KeymapState::Normal;
                    return a;
                }

                // Unknown key in leader-pending: reset and forward the key bytes.
                self.state = KeymapState::Normal;
                self.forward(logical_key, modifiers, app_cursor_keys, event)
            }
        }
    }

    /// Translate a key to bytes and wrap as [`KeyAction::Forward`].
    ///
    /// Returns [`KeyAction::Consumed`] if the key produces no bytes
    /// (dead key, unknown key, etc.).
    fn forward(
        &self,
        logical_key: &Key,
        modifiers: ModifiersState,
        app_cursor_keys: bool,
        event: Option<&KeyEvent>,
    ) -> KeyAction {
        if let Some(event) = event {
            // Full translation path — handles dead-key composition via event.text.
            if let Some(bytes) = translate_key_event(event, modifiers, app_cursor_keys) {
                return KeyAction::Forward(bytes);
            }
            return KeyAction::Consumed;
        }

        // Synthetic path used by tests (no KeyEvent available).
        let bytes: Option<Vec<u8>> =
            self.translate_synthetic(logical_key, modifiers, app_cursor_keys);
        match bytes {
            Some(b) => KeyAction::Forward(b),
            None => KeyAction::Consumed,
        }
    }

    /// Minimal key translation used when no [`KeyEvent`] is available (tests).
    fn translate_synthetic(
        &self,
        logical_key: &Key,
        modifiers: ModifiersState,
        app_cursor_keys: bool,
    ) -> Option<Vec<u8>> {
        let ctrl = modifiers.control_key();
        match logical_key {
            Key::Character(s) if ctrl => {
                let ch = s.chars().next()?;
                let lower = ch.to_ascii_lowercase();
                if lower.is_ascii_alphabetic() {
                    Some(vec![lower as u8 - b'a' + 1])
                } else {
                    None
                }
            }
            Key::Character(s) => Some(s.as_str().as_bytes().to_vec()),
            Key::Named(named) => match named {
                NamedKey::Enter => Some(b"\r".to_vec()),
                NamedKey::Backspace => Some(vec![0x7f]),
                NamedKey::Tab => Some(b"\t".to_vec()),
                NamedKey::Escape => Some(b"\x1b".to_vec()),
                NamedKey::Space => Some(b" ".to_vec()),
                NamedKey::ArrowUp => Some(if app_cursor_keys {
                    b"\x1bOA".to_vec()
                } else {
                    b"\x1b[A".to_vec()
                }),
                NamedKey::ArrowDown => Some(if app_cursor_keys {
                    b"\x1bOB".to_vec()
                } else {
                    b"\x1b[B".to_vec()
                }),
                NamedKey::ArrowRight => Some(if app_cursor_keys {
                    b"\x1bOC".to_vec()
                } else {
                    b"\x1b[C".to_vec()
                }),
                NamedKey::ArrowLeft => Some(if app_cursor_keys {
                    b"\x1bOD".to_vec()
                } else {
                    b"\x1b[D".to_vec()
                }),
                _ => None,
            },
            Key::Dead(_) | Key::Unidentified(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod test_helpers {
    use winit::keyboard::{Key, ModifiersState, SmolStr};

    /// Build a `Key::Character` from a string slice.
    pub fn char_key(s: &str) -> Key {
        Key::Character(SmolStr::new(s))
    }

    /// `ModifiersState` with only Ctrl set.
    pub fn ctrl() -> ModifiersState {
        ModifiersState::CONTROL
    }

    /// Empty `ModifiersState`.
    pub fn no_mods() -> ModifiersState {
        ModifiersState::empty()
    }
}

// ---------------------------------------------------------------------------
// Tests — Tasks 1 & 2
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use winit::keyboard::{Key, ModifiersState, NamedKey};

    use super::test_helpers::*;
    use super::{KeyAction, KeymapHandler};
    use crate::layout::{Axis, Direction};

    // --- Driver helpers ---

    fn press(handler: &mut KeymapHandler, key: Key, mods: ModifiersState) -> KeyAction {
        press_at(handler, key, mods, Instant::now())
    }

    fn press_at(
        handler: &mut KeymapHandler,
        key: Key,
        mods: ModifiersState,
        now: Instant,
    ) -> KeyAction {
        handler.handle_logical_key_with_time(&key, mods, false, now, None)
    }

    // -----------------------------------------------------------------------
    // Task 1: Normal mode — leader detection
    // -----------------------------------------------------------------------

    #[test]
    fn ctrl_a_enters_leader_pending() {
        let mut h = KeymapHandler::new(500);
        let action = press(&mut h, char_key("a"), ctrl());
        assert_eq!(action, KeyAction::Consumed);
        assert!(h.is_leader_pending());
    }

    #[test]
    fn regular_char_forwarded_in_normal_mode() {
        let mut h = KeymapHandler::new(500);
        let action = press(&mut h, char_key("x"), no_mods());
        assert_eq!(action, KeyAction::Forward(b"x".to_vec()));
        assert!(!h.is_leader_pending());
    }

    // -----------------------------------------------------------------------
    // Task 1: Normal mode — Ctrl+h/j/k/l navigation
    // -----------------------------------------------------------------------

    #[test]
    fn ctrl_h_navigates_left() {
        let mut h = KeymapHandler::new(500);
        assert_eq!(
            press(&mut h, char_key("h"), ctrl()),
            KeyAction::NavigatePane(Direction::Left)
        );
    }

    #[test]
    fn ctrl_j_navigates_down() {
        let mut h = KeymapHandler::new(500);
        assert_eq!(
            press(&mut h, char_key("j"), ctrl()),
            KeyAction::NavigatePane(Direction::Down)
        );
    }

    #[test]
    fn ctrl_k_navigates_up() {
        let mut h = KeymapHandler::new(500);
        assert_eq!(
            press(&mut h, char_key("k"), ctrl()),
            KeyAction::NavigatePane(Direction::Up)
        );
    }

    #[test]
    fn ctrl_l_navigates_right() {
        let mut h = KeymapHandler::new(500);
        assert_eq!(
            press(&mut h, char_key("l"), ctrl()),
            KeyAction::NavigatePane(Direction::Right)
        );
    }

    // -----------------------------------------------------------------------
    // Normal mode — Ctrl+Space → OpenCommandOverlay
    // -----------------------------------------------------------------------

    #[test]
    fn ctrl_space_opens_command_overlay() {
        let mut h = KeymapHandler::new(500);
        let action = press(&mut h, Key::Named(NamedKey::Space), ctrl());
        assert_eq!(action, KeyAction::OpenCommandOverlay);
    }

    // -----------------------------------------------------------------------
    // Task 1: LeaderPending — split actions
    // -----------------------------------------------------------------------

    #[test]
    fn leader_then_n_splits_horizontal() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        let action = press(&mut h, char_key("n"), no_mods());
        assert_eq!(action, KeyAction::Split(Axis::Horizontal));
        assert!(!h.is_leader_pending());
    }

    #[test]
    fn leader_then_v_splits_vertical() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        assert_eq!(
            press(&mut h, char_key("v"), no_mods()),
            KeyAction::Split(Axis::Vertical)
        );
    }

    // -----------------------------------------------------------------------
    // Task 1: LeaderPending — pane management
    // -----------------------------------------------------------------------

    #[test]
    fn leader_then_q_closes_pane() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        assert_eq!(
            press(&mut h, char_key("q"), no_mods()),
            KeyAction::ClosePane
        );
    }

    #[test]
    fn leader_then_z_toggles_zoom() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        assert_eq!(
            press(&mut h, char_key("z"), no_mods()),
            KeyAction::ToggleZoom
        );
    }

    // -----------------------------------------------------------------------
    // Task 1: LeaderPending — tab management
    // -----------------------------------------------------------------------

    #[test]
    fn leader_then_t_new_tab() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        assert_eq!(press(&mut h, char_key("t"), no_mods()), KeyAction::NewTab);
    }

    #[test]
    fn leader_then_w_opens_workspace_switcher() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        assert_eq!(
            press(&mut h, char_key("w"), no_mods()),
            KeyAction::OpenWorkspaceSwitcher
        );
    }

    #[test]
    fn leader_then_shift_w_closes_tab() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        assert_eq!(press(&mut h, char_key("W"), no_mods()), KeyAction::CloseTab);
    }

    #[test]
    fn leader_then_digit_switches_tab() {
        let mut h = KeymapHandler::new(500);
        for n in 1usize..=9 {
            press(&mut h, char_key("a"), ctrl());
            let action = press(&mut h, char_key(&n.to_string()), no_mods());
            assert_eq!(action, KeyAction::SwitchTab(n), "digit {n}");
        }
    }

    // -----------------------------------------------------------------------
    // Task 1: LeaderPending — resize with arrow keys
    // -----------------------------------------------------------------------

    #[test]
    fn leader_then_arrow_left_resizes() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        assert_eq!(
            press(&mut h, Key::Named(NamedKey::ArrowLeft), no_mods()),
            KeyAction::ResizePane(Direction::Left),
        );
    }

    #[test]
    fn leader_then_arrow_right_resizes() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        assert_eq!(
            press(&mut h, Key::Named(NamedKey::ArrowRight), no_mods()),
            KeyAction::ResizePane(Direction::Right),
        );
    }

    #[test]
    fn leader_then_arrow_up_resizes() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        assert_eq!(
            press(&mut h, Key::Named(NamedKey::ArrowUp), no_mods()),
            KeyAction::ResizePane(Direction::Up),
        );
    }

    #[test]
    fn leader_then_arrow_down_resizes() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        assert_eq!(
            press(&mut h, Key::Named(NamedKey::ArrowDown), no_mods()),
            KeyAction::ResizePane(Direction::Down),
        );
    }

    // -----------------------------------------------------------------------
    // Task 1: LeaderPending — unknown key resets and forwards
    // -----------------------------------------------------------------------

    #[test]
    fn leader_then_unknown_resets_and_forwards() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        let action = press(&mut h, char_key("x"), no_mods());
        assert!(!h.is_leader_pending(), "state must reset to Normal");
        assert_eq!(action, KeyAction::Forward(b"x".to_vec()));
    }

    // -----------------------------------------------------------------------
    // Task 1: Regular forwarding in normal mode
    // -----------------------------------------------------------------------

    #[test]
    fn enter_forwarded_as_cr() {
        let mut h = KeymapHandler::new(500);
        assert_eq!(
            press(&mut h, Key::Named(NamedKey::Enter), no_mods()),
            KeyAction::Forward(b"\r".to_vec()),
        );
    }

    #[test]
    fn backspace_forwarded() {
        let mut h = KeymapHandler::new(500);
        assert_eq!(
            press(&mut h, Key::Named(NamedKey::Backspace), no_mods()),
            KeyAction::Forward(vec![0x7f]),
        );
    }

    // -----------------------------------------------------------------------
    // Task 2: Timeout edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn leader_timeout_expired_emits_0x01_then_forwards_key() {
        let mut h = KeymapHandler::new(500);
        let t0 = Instant::now();
        // Enter leader at t0.
        press_at(&mut h, char_key("a"), ctrl(), t0);
        // Send next key 600 ms later (past 500 ms timeout).
        let t1 = t0 + Duration::from_millis(600);
        let action = press_at(&mut h, char_key("b"), no_mods(), t1);
        // Should produce 0x01 (Ctrl+a) followed by 'b'.
        assert_eq!(action, KeyAction::Forward(vec![0x01, b'b']));
        assert!(!h.is_leader_pending());
    }

    #[test]
    fn leader_within_timeout_executes_action() {
        let mut h = KeymapHandler::new(500);
        let t0 = Instant::now();
        press_at(&mut h, char_key("a"), ctrl(), t0);
        // 200 ms — well within the 500 ms window.
        let t1 = t0 + Duration::from_millis(200);
        let action = press_at(&mut h, char_key("n"), no_mods(), t1);
        assert_eq!(action, KeyAction::Split(Axis::Horizontal));
    }

    #[test]
    fn leader_at_exact_timeout_boundary_is_expired() {
        let mut h = KeymapHandler::new(500);
        let t0 = Instant::now();
        press_at(&mut h, char_key("a"), ctrl(), t0);
        // Exactly at the timeout boundary (>= means expired).
        let t1 = t0 + Duration::from_millis(500);
        let action = press_at(&mut h, char_key("b"), no_mods(), t1);
        assert!(matches!(action, KeyAction::Forward(_)));
        if let KeyAction::Forward(bytes) = action {
            assert_eq!(bytes[0], 0x01, "first byte must be SOH (0x01)");
        }
    }

    #[test]
    fn double_tap_leader_sends_0x01() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl()); // first Ctrl+a → leader
        let action = press(&mut h, char_key("a"), ctrl()); // second Ctrl+a
        assert_eq!(action, KeyAction::Forward(vec![0x01]));
        assert!(!h.is_leader_pending());
    }

    // -----------------------------------------------------------------------
    // Task 2: is_leader_pending()
    // -----------------------------------------------------------------------

    #[test]
    fn is_leader_pending_false_when_normal() {
        let h = KeymapHandler::new(500);
        assert!(!h.is_leader_pending());
    }

    #[test]
    fn is_leader_pending_true_after_leader() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        assert!(h.is_leader_pending());
    }

    #[test]
    fn is_leader_pending_false_after_action() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        press(&mut h, char_key("n"), no_mods());
        assert!(!h.is_leader_pending());
    }

    // -----------------------------------------------------------------------
    // PLAN-3.2 Task 1: Leader+s → SaveWorkspace
    // -----------------------------------------------------------------------

    #[test]
    fn leader_then_s_saves_workspace() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl());
        let action = press(&mut h, char_key("s"), no_mods());
        assert_eq!(action, KeyAction::SaveWorkspace);
        assert!(
            !h.is_leader_pending(),
            "state must reset to Normal after SaveWorkspace"
        );
    }

    // -----------------------------------------------------------------------
    // PLAN-7.2 Task 2: Leader+a → JumpToAiPane, Leader+p → TogglePlanView
    // -----------------------------------------------------------------------

    #[test]
    fn leader_then_a_jumps_to_ai_pane() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl()); // enter leader
        let action = press(&mut h, char_key("a"), no_mods()); // 'a' without ctrl
        assert_eq!(action, KeyAction::JumpToAiPane);
        assert!(
            !h.is_leader_pending(),
            "state must reset to Normal after JumpToAiPane"
        );
    }

    #[test]
    fn leader_then_p_toggles_plan_view() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl()); // enter leader
        let action = press(&mut h, char_key("p"), no_mods());
        assert_eq!(action, KeyAction::TogglePlanView);
        assert!(
            !h.is_leader_pending(),
            "state must reset to Normal after TogglePlanView"
        );
    }

    // -----------------------------------------------------------------------
    // PLAN-8.1 Task 2: Leader+o → ReviewOverlay, Leader+/ → CrossPaneSearch
    // -----------------------------------------------------------------------

    #[test]
    fn leader_then_o_opens_overlay_review() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl()); // enter leader
        let action = press(&mut h, char_key("o"), no_mods());
        assert_eq!(action, KeyAction::ReviewOverlay);
        assert!(
            !h.is_leader_pending(),
            "state must reset to Normal after ReviewOverlay"
        );
    }

    #[test]
    fn leader_then_slash_opens_search() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl()); // enter leader
        let action = press(&mut h, char_key("/"), no_mods());
        assert_eq!(action, KeyAction::CrossPaneSearch);
        assert!(
            !h.is_leader_pending(),
            "state must reset to Normal after CrossPaneSearch"
        );
    }

    // -----------------------------------------------------------------------
    // PLAN-16 Phase 3 Task 2: Leader+i → OpenAiPane, Leader+c → RefreshAiContext
    // -----------------------------------------------------------------------

    #[test]
    fn leader_then_i_opens_ai_pane() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl()); // enter leader
        let action = press(&mut h, char_key("i"), no_mods());
        assert_eq!(action, KeyAction::OpenAiPane);
        assert!(
            !h.is_leader_pending(),
            "state must reset to Normal after OpenAiPane"
        );
    }

    #[test]
    fn leader_then_c_refreshes_ai_context() {
        let mut h = KeymapHandler::new(500);
        press(&mut h, char_key("a"), ctrl()); // enter leader
        let action = press(&mut h, char_key("c"), no_mods());
        assert_eq!(action, KeyAction::RefreshAiContext);
        assert!(
            !h.is_leader_pending(),
            "state must reset to Normal after RefreshAiContext"
        );
    }
}
