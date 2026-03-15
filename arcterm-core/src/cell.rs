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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_default_is_space() {
        let cell = Cell::default();
        assert_eq!(cell.c, ' ');
        assert_eq!(cell.attrs, CellAttrs::default());
        assert!(cell.dirty, "default cell must be marked dirty");
    }

    #[test]
    fn cell_set_char_marks_dirty() {
        let mut cell = Cell { dirty: false, ..Cell::default() };
        cell.set_char('A');
        assert_eq!(cell.c, 'A');
        assert!(cell.dirty, "set_char must mark the cell dirty");
    }

    #[test]
    fn cell_reset_restores_defaults() {
        let mut cell = Cell::default();
        cell.set_char('Z');
        cell.attrs.bold = true;
        cell.dirty = false;
        cell.reset();
        assert_eq!(cell.c, ' ');
        assert_eq!(cell.attrs, CellAttrs::default());
        assert!(cell.dirty, "reset must mark the cell dirty");
    }

    #[test]
    fn color_default_variant() {
        let c: Color = Color::default();
        assert_eq!(c, Color::Default);
    }

    #[test]
    fn color_indexed_and_rgb() {
        let idx = Color::Indexed(42);
        let rgb = Color::Rgb(255, 128, 0);
        assert_ne!(idx, rgb);
        assert_ne!(idx, Color::Default);
    }

    #[test]
    fn cell_attrs_default_all_false() {
        let attrs = CellAttrs::default();
        assert!(!attrs.bold);
        assert!(!attrs.italic);
        assert!(!attrs.underline);
        assert!(!attrs.reverse);
        assert_eq!(attrs.fg, Color::Default);
        assert_eq!(attrs.bg, Color::Default);
    }
}
