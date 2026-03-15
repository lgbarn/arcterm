//! Terminal cell types.

/// Terminal cell foreground/background color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Color {
    /// Terminal default color.
    #[default]
    Default,
    /// Indexed 256-color palette entry.
    Indexed(u8),
    /// True-color RGB value.
    Rgb(u8, u8, u8),
}

/// Visual attributes for a terminal cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CellAttrs {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
}

/// A single character cell in the terminal grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub c: char,
    pub attrs: CellAttrs,
    pub dirty: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            attrs: CellAttrs::default(),
            dirty: true,
        }
    }
}

impl Cell {
    /// Reset cell to default state, marking it dirty.
    pub fn reset(&mut self) {
        *self = Cell::default();
    }

    /// Set the character, marking cell dirty.
    pub fn set_char(&mut self, c: char) {
        self.c = c;
        self.dirty = true;
    }
}
