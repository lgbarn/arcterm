//! Command palette state machine.
//!
//! Provides a modal command palette that captures all keyboard input when
//! open.  Callers inspect the returned [`PaletteEvent`] to determine what
//! action to take.

use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::layout::{Axis, Direction};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Actions that can be triggered from the command palette.
#[derive(Debug, Clone, PartialEq)]
pub enum PaletteAction {
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    ToggleZoom,
    NewTab,
    CloseTab,
    NavigateLeft,
    NavigateRight,
    NavigateUp,
    NavigateDown,
}

impl PaletteAction {
    /// Convert a [`PaletteAction`] into the equivalent [`crate::keymap::KeyAction`].
    pub fn to_key_action(&self) -> crate::keymap::KeyAction {
        use crate::keymap::KeyAction;
        match self {
            PaletteAction::SplitHorizontal => KeyAction::Split(Axis::Horizontal),
            PaletteAction::SplitVertical   => KeyAction::Split(Axis::Vertical),
            PaletteAction::ClosePane       => KeyAction::ClosePane,
            PaletteAction::ToggleZoom      => KeyAction::ToggleZoom,
            PaletteAction::NewTab          => KeyAction::NewTab,
            PaletteAction::CloseTab        => KeyAction::CloseTab,
            PaletteAction::NavigateLeft    => KeyAction::NavigatePane(Direction::Left),
            PaletteAction::NavigateRight   => KeyAction::NavigatePane(Direction::Right),
            PaletteAction::NavigateUp      => KeyAction::NavigatePane(Direction::Up),
            PaletteAction::NavigateDown    => KeyAction::NavigatePane(Direction::Down),
        }
    }
}

/// A single entry in the command palette.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PaletteCommand {
    pub label: &'static str,
    pub description: &'static str,
    pub action: PaletteAction,
}

/// Return the default set of commands available in the palette.
pub fn default_commands() -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            label: "Split Horizontal",
            description: "Split the active pane horizontally",
            action: PaletteAction::SplitHorizontal,
        },
        PaletteCommand {
            label: "Split Vertical",
            description: "Split the active pane vertically",
            action: PaletteAction::SplitVertical,
        },
        PaletteCommand {
            label: "Close Pane",
            description: "Close the active pane",
            action: PaletteAction::ClosePane,
        },
        PaletteCommand {
            label: "Toggle Zoom",
            description: "Maximise or restore the active pane",
            action: PaletteAction::ToggleZoom,
        },
        PaletteCommand {
            label: "New Tab",
            description: "Open a new tab",
            action: PaletteAction::NewTab,
        },
        PaletteCommand {
            label: "Close Tab",
            description: "Close the active tab",
            action: PaletteAction::CloseTab,
        },
        PaletteCommand {
            label: "Navigate Left",
            description: "Move focus to the pane on the left",
            action: PaletteAction::NavigateLeft,
        },
        PaletteCommand {
            label: "Navigate Right",
            description: "Move focus to the pane on the right",
            action: PaletteAction::NavigateRight,
        },
        PaletteCommand {
            label: "Navigate Up",
            description: "Move focus to the pane above",
            action: PaletteAction::NavigateUp,
        },
        PaletteCommand {
            label: "Navigate Down",
            description: "Move focus to the pane below",
            action: PaletteAction::NavigateDown,
        },
    ]
}

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

/// What the palette's `handle_input` call produced.
#[derive(Debug, PartialEq)]
pub enum PaletteEvent {
    /// The key was consumed; the caller should request a redraw.
    Consumed,
    /// The user dismissed the palette (Escape).
    Close,
    /// The user confirmed a selection; execute the associated action.
    Execute(PaletteAction),
}

/// Runtime state of the command palette.
pub struct PaletteState {
    /// Current text in the filter / search field.
    pub query: String,
    /// Full command list (immutable after construction).
    pub commands: Vec<PaletteCommand>,
    /// Indices into `commands` that match the current `query`.
    pub filtered: Vec<usize>,
    /// Index into `filtered` that is currently highlighted.
    pub selected: usize,
}

impl PaletteState {
    /// Create a new [`PaletteState`] with all commands visible.
    pub fn new() -> Self {
        let commands = default_commands();
        let filtered: Vec<usize> = (0..commands.len()).collect();
        Self {
            query: String::new(),
            commands,
            filtered,
            selected: 0,
        }
    }

    // -----------------------------------------------------------------------
    // Input handling
    // -----------------------------------------------------------------------

    /// Process a winit [`KeyEvent`] and return what happened.
    ///
    /// The palette is modal: every key press is consumed (either acted on or
    /// ignored).  The caller must NOT pass the event to the regular keymap
    /// when this returns anything other than [`PaletteEvent::Close`] /
    /// [`PaletteEvent::Execute`].
    pub fn handle_input(&mut self, event: &KeyEvent, modifiers: ModifiersState) -> PaletteEvent {
        if event.state != winit::event::ElementState::Pressed {
            return PaletteEvent::Consumed;
        }
        self.handle_key(&event.logical_key, event.text.as_deref(), modifiers)
    }

    /// Core key-handling logic, factored out so tests can call it without
    /// constructing a full [`KeyEvent`].
    pub fn handle_key(
        &mut self,
        logical_key: &Key,
        text: Option<&str>,
        _modifiers: ModifiersState,
    ) -> PaletteEvent {
        match logical_key {
            Key::Named(NamedKey::Escape) => PaletteEvent::Close,

            Key::Named(NamedKey::Enter) => {
                if let Some(&idx) = self.filtered.get(self.selected) {
                    let action = self.commands[idx].action.clone();
                    PaletteEvent::Execute(action)
                } else {
                    PaletteEvent::Close
                }
            }

            Key::Named(NamedKey::ArrowUp) => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                PaletteEvent::Consumed
            }

            Key::Named(NamedKey::ArrowDown) => {
                if !self.filtered.is_empty() && self.selected + 1 < self.filtered.len() {
                    self.selected += 1;
                }
                PaletteEvent::Consumed
            }

            Key::Named(NamedKey::Backspace) => {
                self.query.pop();
                self.update_filter();
                PaletteEvent::Consumed
            }

            Key::Character(s) => {
                // Prefer `text` (composed) if available, otherwise use the logical key string.
                let ch = text.unwrap_or(s.as_str());
                self.query.push_str(ch);
                self.update_filter();
                PaletteEvent::Consumed
            }

            _ => PaletteEvent::Consumed,
        }
    }

    // -----------------------------------------------------------------------
    // Filtering
    // -----------------------------------------------------------------------

    /// Recompute `filtered` and clamp `selected` to the new list length.
    pub fn update_filter(&mut self) {
        let query_lower = self.query.to_lowercase();
        self.filtered = self
            .commands
            .iter()
            .enumerate()
            .filter(|(_, cmd)| cmd.label.to_lowercase().contains(&query_lower))
            .map(|(i, _)| i)
            .collect();
        // Clamp selection so it never points past the end.
        if self.filtered.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.filtered.len() - 1);
        }
    }

    // -----------------------------------------------------------------------
    // Rendering helpers
    // -----------------------------------------------------------------------

    /// Slice of filtered command indices visible in the palette (at most 10).
    pub fn visible_commands(&self) -> &[usize] {
        let end = self.filtered.len().min(10);
        &self.filtered[..end]
    }
}

// ---------------------------------------------------------------------------
// Rendering data
// ---------------------------------------------------------------------------

/// A solid-color quad used to render the command palette overlay.
#[derive(Debug, Clone, Copy)]
pub struct PaletteQuad {
    /// Bounding rectangle in physical pixels: [x, y, width, height].
    pub rect: [f32; 4],
    /// RGBA color, components in [0, 1].
    pub color: [f32; 4],
}

/// Text label positioned in physical pixels.
#[derive(Debug, Clone)]
pub struct PaletteText {
    pub text: String,
    /// Physical-pixel x origin.
    pub x: f32,
    /// Physical-pixel y origin.
    pub y: f32,
}

impl PaletteState {
    /// Build the [`PaletteQuad`]s needed to render the palette overlay.
    ///
    /// Quads (back-to-front):
    /// 1. Full-screen dim overlay.
    /// 2. Palette box background (centered).
    /// 3. Input field background (top of palette box).
    /// 4. Selected-row highlight (if something is selected).
    pub fn render_quads(
        &self,
        window_width: f32,
        window_height: f32,
        cell_w: f32,
        cell_h: f32,
        _scale: f32,
    ) -> Vec<PaletteQuad> {
        let mut quads = Vec::new();

        // 1. Full-screen dim.
        quads.push(PaletteQuad {
            rect: [0.0, 0.0, window_width, window_height],
            color: [0.0, 0.0, 0.0, 0.55],
        });

        // Palette box dimensions: 60% wide, up to 14 rows tall.
        let box_w = (window_width * 0.6).max(300.0);
        let visible_count = self.visible_commands().len() as f32;
        let box_h = cell_h * (2.0 + visible_count + 1.0); // input row + commands + padding
        let box_x = (window_width - box_w) / 2.0;
        let box_y = (window_height - box_h) / 3.0; // position at 1/3 from top

        // 2. Palette box background.
        quads.push(PaletteQuad {
            rect: [box_x, box_y, box_w, box_h],
            color: [0.13, 0.14, 0.18, 0.97],
        });

        // 3. Input field background.
        quads.push(PaletteQuad {
            rect: [box_x, box_y, box_w, cell_h * 1.5],
            color: [0.18, 0.19, 0.24, 1.0],
        });

        // 4. Selected-row highlight.
        if !self.filtered.is_empty() {
            let row_y = box_y + cell_h * 1.5 + self.selected as f32 * cell_h;
            quads.push(PaletteQuad {
                rect: [box_x, row_y, box_w, cell_h],
                color: [0.30, 0.25, 0.55, 0.85],
            });
        }

        let _ = cell_w; // available for future use (e.g. padding)
        quads
    }

    /// Build the text labels needed to render the palette.
    ///
    /// Returns: input-field text, then each visible command label.
    pub fn render_text_content(
        &self,
        window_width: f32,
        window_height: f32,
        cell_w: f32,
        cell_h: f32,
        _scale: f32,
    ) -> Vec<PaletteText> {
        let mut items = Vec::new();

        let box_w = (window_width * 0.6).max(300.0);
        let visible_count = self.visible_commands().len() as f32;
        let box_h = cell_h * (2.0 + visible_count + 1.0);
        let box_x = (window_width - box_w) / 2.0;
        let box_y = (window_height - box_h) / 3.0;

        let padding_x = cell_w;

        // Input field prompt + query.
        items.push(PaletteText {
            text: format!("> {}", self.query),
            x: box_x + padding_x,
            y: box_y + (cell_h * 1.5 - cell_h) / 2.0,
        });

        // Command labels — up to 10.
        for (row, &cmd_idx) in self.visible_commands().iter().enumerate() {
            let cmd = &self.commands[cmd_idx];
            items.push(PaletteText {
                text: cmd.label.to_string(),
                x: box_x + padding_x,
                y: box_y + cell_h * 1.5 + row as f32 * cell_h,
            });
        }

        items
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use winit::keyboard::{Key, ModifiersState, NamedKey, SmolStr};

    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn no_mods() -> ModifiersState {
        ModifiersState::empty()
    }

    /// Press a named key through the palette's internal handler.
    fn press_named(palette: &mut PaletteState, named: NamedKey) -> PaletteEvent {
        palette.handle_key(&Key::Named(named), None, no_mods())
    }

    /// Press a character key through the palette's internal handler.
    fn press_char(palette: &mut PaletteState, s: &str) -> PaletteEvent {
        palette.handle_key(&Key::Character(SmolStr::new(s)), Some(s), no_mods())
    }

    // -----------------------------------------------------------------------
    // State machine tests
    // -----------------------------------------------------------------------

    #[test]
    fn all_commands_visible_initially() {
        let palette = PaletteState::new();
        assert_eq!(palette.filtered.len(), palette.commands.len());
    }

    #[test]
    fn filter_by_split_shows_two_commands() {
        let mut palette = PaletteState::new();
        palette.query = "split".to_string();
        palette.update_filter();
        assert_eq!(palette.filtered.len(), 2, "expected 'Split Horizontal' and 'Split Vertical'");
    }

    #[test]
    fn filter_by_zoom_shows_one_command() {
        let mut palette = PaletteState::new();
        palette.query = "zoom".to_string();
        palette.update_filter();
        assert_eq!(palette.filtered.len(), 1);
        assert_eq!(palette.commands[palette.filtered[0]].action, PaletteAction::ToggleZoom);
    }

    #[test]
    fn filter_is_case_insensitive() {
        let mut palette = PaletteState::new();
        palette.query = "SPLIT".to_string();
        palette.update_filter();
        assert_eq!(palette.filtered.len(), 2);
    }

    #[test]
    fn arrow_down_moves_selection() {
        let mut palette = PaletteState::new();
        assert_eq!(palette.selected, 0);
        press_named(&mut palette, NamedKey::ArrowDown);
        assert_eq!(palette.selected, 1);
    }

    #[test]
    fn arrow_up_moves_selection_back() {
        let mut palette = PaletteState::new();
        palette.selected = 2;
        press_named(&mut palette, NamedKey::ArrowUp);
        assert_eq!(palette.selected, 1);
    }

    #[test]
    fn arrow_up_clamps_at_zero() {
        let mut palette = PaletteState::new();
        press_named(&mut palette, NamedKey::ArrowUp);
        assert_eq!(palette.selected, 0, "selection should not go below 0");
    }

    #[test]
    fn arrow_down_clamps_at_last() {
        let mut palette = PaletteState::new();
        let last = palette.filtered.len() - 1;
        palette.selected = last;
        press_named(&mut palette, NamedKey::ArrowDown);
        assert_eq!(palette.selected, last, "selection should not exceed last index");
    }

    #[test]
    fn enter_executes_selected_command() {
        let mut palette = PaletteState::new();
        // First item is SplitHorizontal.
        let result = press_named(&mut palette, NamedKey::Enter);
        assert_eq!(result, PaletteEvent::Execute(PaletteAction::SplitHorizontal));
    }

    #[test]
    fn escape_closes_palette() {
        let mut palette = PaletteState::new();
        let result = press_named(&mut palette, NamedKey::Escape);
        assert_eq!(result, PaletteEvent::Close);
    }

    #[test]
    fn typing_updates_query_and_filter() {
        let mut palette = PaletteState::new();
        press_char(&mut palette, "s");
        assert_eq!(palette.query, "s");
        // Labels with 's' (case-insensitive): "Split Horizontal", "Split Vertical",
        // "Close Pane" has no 's', "Navigate*" has no 's' — at least 2
        assert!(palette.filtered.len() >= 2, "at least two commands should contain 's'");
    }

    #[test]
    fn backspace_removes_last_char_and_refilters() {
        let mut palette = PaletteState::new();
        palette.query = "split".to_string();
        palette.update_filter();
        let count_before = palette.filtered.len();

        press_named(&mut palette, NamedKey::Backspace);
        assert_eq!(palette.query, "spli");
        // "spli" still matches both split commands
        assert_eq!(palette.filtered.len(), count_before);
    }

    #[test]
    fn selection_clamped_after_filter_narrows() {
        let mut palette = PaletteState::new();
        palette.selected = 3;
        palette.query = "split".to_string();
        palette.update_filter();
        assert!(palette.selected <= 1, "selection must be within filtered range");
    }

    #[test]
    fn visible_commands_capped_at_ten() {
        let palette = PaletteState::new();
        assert!(palette.visible_commands().len() <= 10);
    }

    #[test]
    fn palette_action_maps_to_key_action() {
        use crate::keymap::KeyAction;
        use crate::layout::Axis;
        assert!(matches!(
            PaletteAction::SplitHorizontal.to_key_action(),
            KeyAction::Split(Axis::Horizontal)
        ));
        assert!(matches!(
            PaletteAction::ClosePane.to_key_action(),
            KeyAction::ClosePane
        ));
    }

    // -----------------------------------------------------------------------
    // Rendering data tests
    // -----------------------------------------------------------------------

    #[test]
    fn render_quads_includes_dim_overlay() {
        let palette = PaletteState::new();
        let quads = palette.render_quads(1200.0, 800.0, 8.0, 16.0, 1.0);
        // First quad should cover the full screen.
        assert!(!quads.is_empty());
        let dim = &quads[0];
        assert_eq!(dim.rect[0], 0.0);
        assert_eq!(dim.rect[1], 0.0);
        assert_eq!(dim.rect[2], 1200.0);
        assert_eq!(dim.rect[3], 800.0);
    }

    #[test]
    fn render_text_includes_input_and_commands() {
        let palette = PaletteState::new();
        let texts = palette.render_text_content(1200.0, 800.0, 8.0, 16.0, 1.0);
        // First entry = input prompt, rest = command labels.
        assert!(texts[0].text.starts_with('>'));
        assert!(texts.len() > 1, "should have command labels too");
    }

    #[test]
    fn render_text_capped_at_eleven_entries() {
        // 1 input + up to 10 commands = 11 max.
        let palette = PaletteState::new();
        let texts = palette.render_text_content(1200.0, 800.0, 8.0, 16.0, 1.0);
        assert!(texts.len() <= 11);
    }
}
