//! arcterm-core — shared types for the arcterm terminal emulator.

pub mod cell;
pub mod grid;
pub mod input;

pub use cell::{Cell, CellAttrs, Color};
pub use grid::{CursorPos, Grid, GridSize, TermModes};
pub use input::{InputEvent, KeyCode, Modifiers};
