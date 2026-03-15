//! Keyboard input translation: winit KeyEvent → PTY byte sequences.

use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// Translate a winit `KeyEvent` (assumed pressed) into a PTY byte sequence.
///
/// `ctrl` — whether the Ctrl modifier is currently held (tracked separately
/// via `WindowEvent::ModifiersChanged` in the `App`).
///
/// Returns `None` if the event should be ignored (e.g. dead keys, unknown keys).
pub fn translate_key_event(event: &KeyEvent, modifiers: ModifiersState) -> Option<Vec<u8>> {
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

        Key::Named(named) => translate_named(named),
        // Dead keys (e.g. dead acute) and unidentified keys are ignored.
        Key::Dead(_) | Key::Unidentified(_) => None,
    }
}

fn translate_named(key: &NamedKey) -> Option<Vec<u8>> {
    Some(match key {
        NamedKey::Enter => b"\r".to_vec(),
        NamedKey::Backspace => b"\x7f".to_vec(),
        NamedKey::Tab => b"\t".to_vec(),
        NamedKey::Escape => b"\x1b".to_vec(),
        // Arrow keys — ANSI cursor sequences.
        NamedKey::ArrowUp => b"\x1b[A".to_vec(),
        NamedKey::ArrowDown => b"\x1b[B".to_vec(),
        NamedKey::ArrowRight => b"\x1b[C".to_vec(),
        NamedKey::ArrowLeft => b"\x1b[D".to_vec(),
        // Navigation.
        NamedKey::Home => b"\x1b[H".to_vec(),
        NamedKey::End => b"\x1b[F".to_vec(),
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
