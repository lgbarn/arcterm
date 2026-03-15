//! Input event types for the terminal.

use crate::grid::GridSize;

/// High-level input events from the window system.
#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    Key(KeyCode, Modifiers),
    Resize(GridSize),
    Paste(String),
}

/// Logical key codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    F(u8),
}

/// Keyboard modifier bitmask.
///
/// Bits: SHIFT=1, CTRL=2, ALT=4, SUPER=8
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const SHIFT: u8 = 0b0001;
    pub const CTRL: u8 = 0b0010;
    pub const ALT: u8 = 0b0100;
    pub const SUPER: u8 = 0b1000;

    pub fn none() -> Self {
        Self(0)
    }

    pub fn shift() -> Self {
        Self(Self::SHIFT)
    }

    pub fn ctrl() -> Self {
        Self(Self::CTRL)
    }

    pub fn alt() -> Self {
        Self(Self::ALT)
    }

    pub fn has_shift(self) -> bool {
        self.0 & Self::SHIFT != 0
    }

    pub fn has_ctrl(self) -> bool {
        self.0 & Self::CTRL != 0
    }

    pub fn has_alt(self) -> bool {
        self.0 & Self::ALT != 0
    }

    pub fn has_super(self) -> bool {
        self.0 & Self::SUPER != 0
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_none_has_no_flags() {
        let m = Modifiers::none();
        assert!(!m.has_shift());
        assert!(!m.has_ctrl());
        assert!(!m.has_alt());
        assert!(!m.has_super());
    }

    #[test]
    fn modifiers_ctrl_is_ctrl_only() {
        let m = Modifiers::ctrl();
        assert!(!m.has_shift());
        assert!(m.has_ctrl());
        assert!(!m.has_alt());
        assert!(!m.has_super());
    }

    #[test]
    fn modifiers_shift_is_shift_only() {
        let m = Modifiers::shift();
        assert!(m.has_shift());
        assert!(!m.has_ctrl());
    }

    #[test]
    fn modifiers_alt_is_alt_only() {
        let m = Modifiers::alt();
        assert!(!m.has_shift());
        assert!(!m.has_ctrl());
        assert!(m.has_alt());
        assert!(!m.has_super());
    }

    #[test]
    fn modifiers_bitor_combines_flags() {
        let m = Modifiers::ctrl() | Modifiers::shift();
        assert!(m.has_ctrl());
        assert!(m.has_shift());
        assert!(!m.has_alt());
    }

    #[test]
    fn modifiers_super_flag() {
        let m = Modifiers(Modifiers::SUPER);
        assert!(m.has_super());
        assert!(!m.has_ctrl());
    }

    #[test]
    fn modifiers_default_is_none() {
        let m = Modifiers::default();
        assert_eq!(m, Modifiers::none());
    }

    #[test]
    fn input_event_key_variant() {
        let ev = InputEvent::Key(KeyCode::Char('a'), Modifiers::none());
        match ev {
            InputEvent::Key(KeyCode::Char('a'), m) => assert_eq!(m, Modifiers::none()),
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn input_event_resize_variant() {
        let ev = InputEvent::Resize(GridSize::new(24, 80));
        match ev {
            InputEvent::Resize(gs) => {
                assert_eq!(gs.rows, 24);
                assert_eq!(gs.cols, 80);
            }
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn input_event_paste_variant() {
        let ev = InputEvent::Paste("hello".to_string());
        match ev {
            InputEvent::Paste(s) => assert_eq!(s, "hello"),
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn keycode_function_keys() {
        let f1 = KeyCode::F(1);
        let f12 = KeyCode::F(12);
        assert_ne!(f1, f12);
    }
}
