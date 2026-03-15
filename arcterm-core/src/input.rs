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
