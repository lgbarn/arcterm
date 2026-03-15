//! Keyboard input translation: winit KeyEvent → PTY byte sequences.

use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// Translate a winit `KeyEvent` (assumed pressed) into a PTY byte sequence.
///
/// `modifiers` — current keyboard modifier state.
/// `app_cursor_keys` — when `true`, arrow/Home/End keys send SS3 sequences
///   (`ESC O A/B/C/D`, `ESC O H`, `ESC O F`) instead of the normal CSI sequences.
///
/// Returns `None` if the event should be ignored (e.g. dead keys, unknown keys).
pub fn translate_key_event(
    event: &KeyEvent,
    modifiers: ModifiersState,
    app_cursor_keys: bool,
) -> Option<Vec<u8>> {
    let ctrl = modifiers.control_key();

    match &event.logical_key {
        // Ctrl+a..z → 0x01..0x1a (control codes).
        Key::Character(s) if ctrl => {
            let ch = s.chars().next()?;
            let lower = ch.to_ascii_lowercase();
            if lower.is_ascii_alphabetic() {
                let byte = lower as u8 - b'a' + 1;
                return Some(vec![byte]);
            }
            // Ctrl+[ → ESC
            if lower == '[' {
                return Some(vec![0x1b]);
            }
            // Ctrl+\ → FS (0x1c, SIGQUIT in terminals)
            if lower == '\\' {
                return Some(vec![0x1c]);
            }
            // Ctrl+] → GS (0x1d, telnet escape)
            if lower == ']' {
                return Some(vec![0x1d]);
            }
            None
        }

        // Printable characters: prefer event.text (handles dead-key composition).
        Key::Character(s) => {
            let text = event.text.as_ref().map(|t| t.as_str()).unwrap_or(s.as_str());
            if text.is_empty() {
                None
            } else {
                Some(text.as_bytes().to_vec())
            }
        }

        Key::Named(named) => translate_named(named, app_cursor_keys),
        // Dead keys (e.g. dead acute) and unidentified keys are ignored.
        Key::Dead(_) | Key::Unidentified(_) => None,
    }
}

#[allow(dead_code)]
pub(crate) fn translate_named_key(key: &NamedKey, app_cursor_keys: bool) -> Option<Vec<u8>> {
    translate_named(key, app_cursor_keys)
}

fn translate_named(key: &NamedKey, app_cursor_keys: bool) -> Option<Vec<u8>> {
    Some(match key {
        NamedKey::Enter => b"\r".to_vec(),
        NamedKey::Backspace => b"\x7f".to_vec(),
        NamedKey::Tab => b"\t".to_vec(),
        NamedKey::Escape => b"\x1b".to_vec(),
        // Arrow keys — SS3 sequences in app cursor key mode, ANSI CSI otherwise.
        NamedKey::ArrowUp    => if app_cursor_keys { b"\x1bOA".to_vec() } else { b"\x1b[A".to_vec() },
        NamedKey::ArrowDown  => if app_cursor_keys { b"\x1bOB".to_vec() } else { b"\x1b[B".to_vec() },
        NamedKey::ArrowRight => if app_cursor_keys { b"\x1bOC".to_vec() } else { b"\x1b[C".to_vec() },
        NamedKey::ArrowLeft  => if app_cursor_keys { b"\x1bOD".to_vec() } else { b"\x1b[D".to_vec() },
        // Navigation — SS3 in app mode, CSI otherwise.
        NamedKey::Home => if app_cursor_keys { b"\x1bOH".to_vec() } else { b"\x1b[H".to_vec() },
        NamedKey::End  => if app_cursor_keys { b"\x1bOF".to_vec() } else { b"\x1b[F".to_vec() },
        NamedKey::PageUp => b"\x1b[5~".to_vec(),
        NamedKey::PageDown => b"\x1b[6~".to_vec(),
        NamedKey::Delete => b"\x1b[3~".to_vec(),
        // Function keys — VT220 sequences.
        NamedKey::F1 => b"\x1bOP".to_vec(),
        NamedKey::F2 => b"\x1bOQ".to_vec(),
        NamedKey::F3 => b"\x1bOR".to_vec(),
        NamedKey::F4 => b"\x1bOS".to_vec(),
        NamedKey::F5 => b"\x1b[15~".to_vec(),
        NamedKey::F6 => b"\x1b[17~".to_vec(),
        NamedKey::F7 => b"\x1b[18~".to_vec(),
        NamedKey::F8 => b"\x1b[19~".to_vec(),
        NamedKey::F9 => b"\x1b[20~".to_vec(),
        NamedKey::F10 => b"\x1b[21~".to_vec(),
        NamedKey::F11 => b"\x1b[23~".to_vec(),
        NamedKey::F12 => b"\x1b[24~".to_vec(),
        // Space (when it arrives as a Named key).
        NamedKey::Space => b" ".to_vec(),
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::translate_named_key;
    use winit::keyboard::NamedKey;

    // ---- Normal (non-app) cursor key mode ----

    #[test]
    fn arrow_up_normal_mode_sends_csi() {
        let bytes = translate_named_key(&NamedKey::ArrowUp, false).unwrap();
        assert_eq!(bytes, b"\x1b[A", "ArrowUp in normal mode must send CSI A");
    }

    #[test]
    fn arrow_down_normal_mode_sends_csi() {
        let bytes = translate_named_key(&NamedKey::ArrowDown, false).unwrap();
        assert_eq!(bytes, b"\x1b[B");
    }

    #[test]
    fn arrow_right_normal_mode_sends_csi() {
        let bytes = translate_named_key(&NamedKey::ArrowRight, false).unwrap();
        assert_eq!(bytes, b"\x1b[C");
    }

    #[test]
    fn arrow_left_normal_mode_sends_csi() {
        let bytes = translate_named_key(&NamedKey::ArrowLeft, false).unwrap();
        assert_eq!(bytes, b"\x1b[D");
    }

    #[test]
    fn home_normal_mode_sends_csi() {
        let bytes = translate_named_key(&NamedKey::Home, false).unwrap();
        assert_eq!(bytes, b"\x1b[H");
    }

    #[test]
    fn end_normal_mode_sends_csi() {
        let bytes = translate_named_key(&NamedKey::End, false).unwrap();
        assert_eq!(bytes, b"\x1b[F");
    }

    // ---- App cursor key mode ----

    #[test]
    fn arrow_up_app_mode_sends_ss3() {
        let bytes = translate_named_key(&NamedKey::ArrowUp, true).unwrap();
        assert_eq!(bytes, b"\x1bOA", "ArrowUp in app cursor mode must send SS3 A");
    }

    #[test]
    fn arrow_down_app_mode_sends_ss3() {
        let bytes = translate_named_key(&NamedKey::ArrowDown, true).unwrap();
        assert_eq!(bytes, b"\x1bOB");
    }

    #[test]
    fn arrow_right_app_mode_sends_ss3() {
        let bytes = translate_named_key(&NamedKey::ArrowRight, true).unwrap();
        assert_eq!(bytes, b"\x1bOC");
    }

    #[test]
    fn arrow_left_app_mode_sends_ss3() {
        let bytes = translate_named_key(&NamedKey::ArrowLeft, true).unwrap();
        assert_eq!(bytes, b"\x1bOD");
    }

    #[test]
    fn home_app_mode_sends_ss3() {
        let bytes = translate_named_key(&NamedKey::Home, true).unwrap();
        assert_eq!(bytes, b"\x1bOH", "Home in app cursor mode must send SS3 H");
    }

    #[test]
    fn end_app_mode_sends_ss3() {
        let bytes = translate_named_key(&NamedKey::End, true).unwrap();
        assert_eq!(bytes, b"\x1bOF", "End in app cursor mode must send SS3 F");
    }

    // ---- Mode-independent keys must not change ----

    #[test]
    fn pageup_unchanged_in_app_mode() {
        let normal = translate_named_key(&NamedKey::PageUp, false).unwrap();
        let app    = translate_named_key(&NamedKey::PageUp, true).unwrap();
        assert_eq!(normal, b"\x1b[5~");
        assert_eq!(app, b"\x1b[5~", "PageUp must be the same in both modes");
    }

    #[test]
    fn enter_unchanged_in_app_mode() {
        let normal = translate_named_key(&NamedKey::Enter, false).unwrap();
        let app    = translate_named_key(&NamedKey::Enter, true).unwrap();
        assert_eq!(normal, b"\r");
        assert_eq!(app, b"\r");
    }

    #[test]
    fn f1_unchanged_in_app_mode() {
        let normal = translate_named_key(&NamedKey::F1, false).unwrap();
        let app    = translate_named_key(&NamedKey::F1, true).unwrap();
        assert_eq!(normal, b"\x1bOP");
        assert_eq!(app, b"\x1bOP");
    }
}
