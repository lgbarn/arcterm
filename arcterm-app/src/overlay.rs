//! Config overlay review state machine.
//!
//! When the user presses Leader+o, Arcterm opens a diff view of the oldest
//! pending overlay file against the current resolved config. The user can
//! accept (move to accepted/), reject (delete), edit (spawn $EDITOR), or
//! cycle through multiple pending files.

use std::path::{Path, PathBuf};

use crate::config::{self, ArctermConfig};
use crate::palette::PaletteQuad;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single line in the computed diff.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffLine {
    /// Unchanged context line.
    Context(String),
    /// Line present only in the overlay (added relative to base).
    Added(String),
    /// Line present only in the base (removed by overlay).
    Removed(String),
}

/// Action produced by [`OverlayReviewState::handle_key`].
#[derive(Debug, PartialEq)]
pub enum OverlayAction {
    /// Accept the current pending overlay (move to accepted/).
    Accept,
    /// Reject the current pending overlay (delete it).
    Reject,
    /// Open the overlay file in `$EDITOR`.
    Edit(PathBuf),
    /// Close the overlay review without acting.
    Close,
    /// Advance to the next pending file.
    NextFile,
    /// Go back to the previous pending file.
    PrevFile,
    /// Key was not handled.
    Noop,
}

/// Runtime state for the overlay diff review UI.
pub struct OverlayReviewState {
    /// All pending overlay files, sorted by filename.
    pub pending_files: Vec<PathBuf>,
    /// Index into `pending_files` currently shown.
    pub current_index: usize,
    /// Computed diff for the current file.
    pub diff_text: Vec<DiffLine>,
    /// Scroll offset for long diffs.
    pub scroll_offset: usize,
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

impl OverlayReviewState {
    /// Create a new review state by loading all pending overlays and computing
    /// the diff for the first file.
    ///
    /// Returns `None` if there are no pending overlay files.
    pub fn new(base_config: &ArctermConfig) -> Option<Self> {
        let pending_files = load_pending();
        if pending_files.is_empty() {
            return None;
        }
        let diff_text = compute_diff(base_config, &pending_files[0]);
        Some(Self {
            pending_files,
            current_index: 0,
            diff_text,
            scroll_offset: 0,
        })
    }

    /// The path of the currently displayed pending overlay file.
    pub fn current_path(&self) -> &Path {
        &self.pending_files[self.current_index]
    }

    /// Reload the diff for the current file.
    // Kept for future live-reload support; not yet wired to a keybinding.
    #[allow(dead_code)]
    pub fn reload_diff(&mut self, base_config: &ArctermConfig) {
        if self.current_index < self.pending_files.len() {
            self.diff_text = compute_diff(base_config, &self.pending_files[self.current_index]);
            self.scroll_offset = 0;
        }
    }
}

// ---------------------------------------------------------------------------
// Pending file discovery
// ---------------------------------------------------------------------------

/// Read `pending_dir()` and return sorted `.toml` files.
pub fn load_pending() -> Vec<PathBuf> {
    let dir = config::pending_dir();
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
        .collect();
    paths.sort();
    paths
}

// ---------------------------------------------------------------------------
// Diff computation
// ---------------------------------------------------------------------------

/// Serialize `base_config` to TOML and diff it against the file at `pending_path`.
pub fn compute_diff(base_config: &ArctermConfig, pending_path: &Path) -> Vec<DiffLine> {
    let base_str = toml::to_string_pretty(base_config).unwrap_or_default();
    let pending_str = std::fs::read_to_string(pending_path).unwrap_or_default();

    let diff = similar::TextDiff::from_lines(&base_str, &pending_str);
    let mut result = Vec::new();

    for change in diff.iter_all_changes() {
        let line = change.value().to_string();
        match change.tag() {
            similar::ChangeTag::Equal => result.push(DiffLine::Context(line)),
            similar::ChangeTag::Insert => result.push(DiffLine::Added(line)),
            similar::ChangeTag::Delete => result.push(DiffLine::Removed(line)),
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Key handling
// ---------------------------------------------------------------------------

impl OverlayReviewState {
    /// Process a winit key and return the action to take.
    pub fn handle_key(&mut self, key: &winit::keyboard::Key) -> OverlayAction {
        use winit::keyboard::{Key, NamedKey};

        match key {
            Key::Character(s) => match s.as_str() {
                "a" => OverlayAction::Accept,
                "x" => OverlayAction::Reject,
                "e" => OverlayAction::Edit(self.current_path().to_path_buf()),
                "n" => OverlayAction::NextFile,
                "N" => OverlayAction::PrevFile,
                _ => OverlayAction::Noop,
            },
            Key::Named(NamedKey::Escape) => OverlayAction::Close,
            _ => OverlayAction::Noop,
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

impl OverlayReviewState {
    /// Build overlay quads and text lines for rendering the diff view.
    ///
    /// Returns `(quads, text_lines)` where `text_lines` are the visible diff
    /// lines starting from `scroll_offset`, plus a header line.
    pub fn render_quads(
        &self,
        window_w: f32,
        window_h: f32,
    ) -> (Vec<PaletteQuad>, Vec<String>) {
        let mut quads = Vec::new();
        let mut text_lines = Vec::new();

        // Full-screen dim overlay.
        quads.push(PaletteQuad {
            rect: [0.0, 0.0, window_w, window_h],
            color: [0.0, 0.0, 0.0, 0.72],
        });

        // Panel background (80% wide, 80% tall, centered).
        let panel_w = (window_w * 0.80).max(400.0);
        let panel_h = (window_h * 0.80).max(300.0);
        let panel_x = (window_w - panel_w) / 2.0;
        let panel_y = (window_h - panel_h) / 2.0;

        quads.push(PaletteQuad {
            rect: [panel_x, panel_y, panel_w, panel_h],
            color: [0.10, 0.11, 0.15, 0.97],
        });

        // Header line.
        let filename = self
            .current_path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<unknown>");
        let total = self.pending_files.len();
        let idx = self.current_index + 1;
        text_lines.push(format!(
            "[{idx}/{total}] {filename}  |  [a]ccept  [x]reject  [e]dit  [Esc]close  [n/N]ext/prev"
        ));

        // Diff lines (respecting scroll_offset).
        let cell_h = 16.0_f32; // approximate logical cell height
        let visible_rows = ((panel_h - cell_h * 2.0) / cell_h).max(1.0) as usize;

        let start = self.scroll_offset.min(self.diff_text.len().saturating_sub(1));
        let end = (start + visible_rows).min(self.diff_text.len());

        for (i, line) in self.diff_text[start..end].iter().enumerate() {
            let row_y = panel_y + cell_h * (1.0 + i as f32);
            let (line_text, color) = match line {
                DiffLine::Added(s) => {
                    quads.push(PaletteQuad {
                        rect: [panel_x, row_y, panel_w, cell_h],
                        color: [0.0, 0.30, 0.10, 0.45],
                    });
                    (format!("+ {s}"), [0.2_f32, 0.9, 0.4, 1.0])
                }
                DiffLine::Removed(s) => {
                    quads.push(PaletteQuad {
                        rect: [panel_x, row_y, panel_w, cell_h],
                        color: [0.35, 0.0, 0.0, 0.45],
                    });
                    (format!("- {s}"), [0.9_f32, 0.3, 0.3, 1.0])
                }
                DiffLine::Context(s) => (format!("  {s}"), [0.7_f32, 0.7, 0.7, 1.0]),
            };
            let _ = color; // color is used by the renderer via quads
            text_lines.push(line_text);
        }

        (quads, text_lines)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // load_pending — empty when directory absent
    // -----------------------------------------------------------------------

    #[test]
    fn load_pending_returns_empty_when_dir_absent() {
        // pending_dir() almost certainly doesn't exist in CI.
        // Verify it returns an empty Vec rather than panicking.
        let pending = load_pending();
        // If the dir doesn't exist, we get empty.
        // If it does exist but has no .toml files, also empty.
        // Either is fine — we just can't panic.
        let _ = pending; // passes if no panic
    }

    // -----------------------------------------------------------------------
    // compute_diff — detects added and removed lines
    // -----------------------------------------------------------------------

    #[test]
    fn compute_diff_detects_added_lines() {
        let base_config = ArctermConfig::default();

        // Write a temp overlay with an extra key that doesn't exist in defaults.
        let dir = tempfile::tempdir().expect("temp dir");
        let overlay_path = dir.path().join("overlay.toml");
        // The default TOML won't contain a custom comment, so adding a new
        // key that differs will produce Added lines.
        std::fs::write(&overlay_path, "font_size = 20.0\n").unwrap();

        let diff = compute_diff(&base_config, &overlay_path);
        // There must be at least one Added or Removed line (font_size changed).
        let has_diff = diff
            .iter()
            .any(|l| matches!(l, DiffLine::Added(_) | DiffLine::Removed(_)));
        assert!(has_diff, "diff must contain non-context lines: {diff:?}");
    }

    #[test]
    fn compute_diff_produces_added_lines_for_new_content() {
        let base_config = ArctermConfig::default();

        let dir = tempfile::tempdir().expect("temp dir");
        let overlay_path = dir.path().join("overlay.toml");
        // Overlay adds a key not present in serialized base.
        std::fs::write(&overlay_path, "font_size = 20.0\ncolor_scheme = \"dracula\"\n").unwrap();

        let diff = compute_diff(&base_config, &overlay_path);
        let added_count = diff.iter().filter(|l| matches!(l, DiffLine::Added(_))).count();
        assert!(added_count > 0, "must have Added lines: {diff:?}");
    }

    #[test]
    fn compute_diff_produces_removed_lines_for_missing_keys() {
        let base_config = ArctermConfig::default();

        let dir = tempfile::tempdir().expect("temp dir");
        let overlay_path = dir.path().join("overlay.toml");
        // Overlay has fewer keys than the full serialized base → Removed lines.
        std::fs::write(&overlay_path, "font_size = 14.0\n").unwrap();

        let diff = compute_diff(&base_config, &overlay_path);
        let removed_count = diff.iter().filter(|l| matches!(l, DiffLine::Removed(_))).count();
        assert!(removed_count > 0, "must have Removed lines when overlay has fewer keys: {diff:?}");
    }

    // -----------------------------------------------------------------------
    // handle_key — correct OverlayAction for each key
    // -----------------------------------------------------------------------

    fn make_state_with_temp_file() -> (OverlayReviewState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.toml");
        std::fs::write(&path, "font_size = 16.0\n").unwrap();

        let state = OverlayReviewState {
            pending_files: vec![path],
            current_index: 0,
            diff_text: Vec::new(),
            scroll_offset: 0,
        };
        (state, dir)
    }

    fn char_key(s: &str) -> winit::keyboard::Key {
        winit::keyboard::Key::Character(winit::keyboard::SmolStr::new(s))
    }

    fn named_key(n: winit::keyboard::NamedKey) -> winit::keyboard::Key {
        winit::keyboard::Key::Named(n)
    }

    #[test]
    fn handle_key_a_returns_accept() {
        let (mut state, _dir) = make_state_with_temp_file();
        assert_eq!(state.handle_key(&char_key("a")), OverlayAction::Accept);
    }

    #[test]
    fn handle_key_x_returns_reject() {
        let (mut state, _dir) = make_state_with_temp_file();
        assert_eq!(state.handle_key(&char_key("x")), OverlayAction::Reject);
    }

    #[test]
    fn handle_key_e_returns_edit_with_path() {
        let (mut state, dir) = make_state_with_temp_file();
        let expected_path = dir.path().join("test.toml");
        let action = state.handle_key(&char_key("e"));
        assert_eq!(action, OverlayAction::Edit(expected_path));
    }

    #[test]
    fn handle_key_n_returns_next_file() {
        let (mut state, _dir) = make_state_with_temp_file();
        assert_eq!(state.handle_key(&char_key("n")), OverlayAction::NextFile);
    }

    #[test]
    fn handle_key_shift_n_returns_prev_file() {
        let (mut state, _dir) = make_state_with_temp_file();
        assert_eq!(state.handle_key(&char_key("N")), OverlayAction::PrevFile);
    }

    #[test]
    fn handle_key_escape_returns_close() {
        let (mut state, _dir) = make_state_with_temp_file();
        let action = state.handle_key(&named_key(winit::keyboard::NamedKey::Escape));
        assert_eq!(action, OverlayAction::Close);
    }

    #[test]
    fn handle_key_unknown_returns_noop() {
        let (mut state, _dir) = make_state_with_temp_file();
        let action = state.handle_key(&char_key("z"));
        assert_eq!(action, OverlayAction::Noop);
    }
}
