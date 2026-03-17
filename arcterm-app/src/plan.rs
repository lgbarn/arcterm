//! Plan status layer — parses `.shipyard/` plan files and provides
//! ambient and modal UI state for the plan strip and plan view overlay.
//!
//! # Design
//!
//! [`PlanStripState`] holds summarised plan data for the ambient status bar
//! at the bottom of the window.  [`PlanViewState`] is the expanded modal
//! overlay shown when Leader+p is pressed while the strip is visible.
//!
//! [`discover_plan_files`] scans the workspace root for plan files in a
//! prioritised order.  [`parse_plan_summary`] extracts checkbox counts and
//! an optional phase identifier from YAML frontmatter.

use std::path::{Path, PathBuf};
use std::time::Instant;

// ---------------------------------------------------------------------------
// PlanSummary — a parsed summary of one plan file
// ---------------------------------------------------------------------------

/// A parsed summary of one plan file.
#[derive(Debug, Clone)]
pub struct PlanSummary {
    /// Phase identifier extracted from YAML frontmatter `phase:` field, if any.
    pub phase: Option<String>,
    /// Number of completed checkboxes (`[x]`).
    pub completed: usize,
    /// Total number of checkboxes (`[x]` + `[ ]`).
    pub total: usize,
    /// Path to the source file.
    pub file_path: PathBuf,
}

// ---------------------------------------------------------------------------
// parse_plan_summary — read a file and count checkboxes
// ---------------------------------------------------------------------------

/// Parse a plan file at `path` and return a [`PlanSummary`].
///
/// Returns `None` when the file does not exist, cannot be read, or contains
/// no checkbox patterns at all.
pub fn parse_plan_summary(path: &Path) -> Option<PlanSummary> {
    let text = std::fs::read_to_string(path).ok()?;
    parse_plan_summary_from_str(&text, path)
}

/// Parse plan summary from a string (used by unit tests and the public API).
pub fn parse_plan_summary_from_str(text: &str, path: &Path) -> Option<PlanSummary> {
    let mut completed = 0usize;
    let mut total = 0usize;
    let mut phase: Option<String> = None;

    // --- YAML frontmatter extraction ---
    // Frontmatter is delimited by `---` on its own line at the start of the file.
    let mut in_frontmatter = false;
    let mut frontmatter_done = false;
    let mut lines = text.lines().peekable();

    // Check for frontmatter opener.
    if lines.peek().copied() == Some("---") {
        in_frontmatter = true;
        lines.next(); // consume the opening ---
    }

    for line in lines {
        if in_frontmatter && !frontmatter_done {
            if line.trim() == "---" {
                frontmatter_done = true;
                in_frontmatter = false;
                continue;
            }
            // Simple `key: value` extraction for `phase:`.
            if let Some(val) = line.trim().strip_prefix("phase:") {
                let trimmed = val.trim().trim_matches('"').trim_matches('\'');
                if !trimmed.is_empty() {
                    phase = Some(trimmed.to_string());
                }
            }
            continue;
        }

        // Count checkboxes in body text.
        // Match `[x]` / `[X]` (completed) and `[ ]` (incomplete).
        let mut search = line;
        while !search.is_empty() {
            if let Some(rest) = search
                .strip_prefix("[x]")
                .or_else(|| search.strip_prefix("[X]"))
            {
                completed += 1;
                total += 1;
                search = rest;
            } else if let Some(rest) = search.strip_prefix("[ ]") {
                total += 1;
                search = rest;
            } else {
                // Advance one character.
                let mut chars = search.char_indices();
                chars.next(); // skip current char
                search = chars.next().map(|(i, _)| &search[i..]).unwrap_or("");
            }
        }
    }

    if total == 0 {
        return None;
    }

    Some(PlanSummary {
        phase,
        completed,
        total,
        file_path: path.to_path_buf(),
    })
}

// ---------------------------------------------------------------------------
// discover_plan_files — locate plan files in the workspace
// ---------------------------------------------------------------------------

/// Discover all plan files under `workspace_root`.
///
/// Search order (highest priority first):
/// 1. `.shipyard/` directory: all `PLAN-*.md` files (sorted by name).
/// 2. `PLAN.md` in the workspace root.
/// 3. `TODO.md` in the workspace root.
///
/// Returns all found paths in the order described.
pub fn discover_plan_files(workspace_root: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();

    // 1. .shipyard/ PLAN-*.md files.
    let shipyard_dir = workspace_root.join(".shipyard");
    if shipyard_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&shipyard_dir) {
            let mut plan_files: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.starts_with("PLAN-") && n.ends_with(".md"))
                            .unwrap_or(false)
                })
                .collect();
            plan_files.sort();
            paths.extend(plan_files);
        }

        // Recurse into phases/ subdirectory for nested PLAN-*.md files.
        let phases_dir = shipyard_dir.join("phases");
        if phases_dir.is_dir() {
            collect_plan_files_recursive(&phases_dir, &mut paths);
        }
    }

    // 2. PLAN.md.
    let plan_md = workspace_root.join("PLAN.md");
    if plan_md.is_file() {
        paths.push(plan_md);
    }

    // 3. TODO.md.
    let todo_md = workspace_root.join("TODO.md");
    if todo_md.is_file() {
        paths.push(todo_md);
    }

    paths
}

/// Recursively collect `PLAN-*.md` files from `dir` (sorted at each level).
fn collect_plan_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    items.sort();
    for item in items {
        if item.is_dir() {
            collect_plan_files_recursive(&item, out);
        } else if item.is_file()
            && item
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("PLAN-") && n.ends_with(".md"))
                .unwrap_or(false)
        {
            out.push(item);
        }
    }
}

// ---------------------------------------------------------------------------
// PlanStripState — ambient status bar data
// ---------------------------------------------------------------------------

/// Ambient status bar data, refreshed from file-system events.
pub struct PlanStripState {
    /// All plan summaries discovered in the workspace.
    pub summaries: Vec<PlanSummary>,
    /// Wall-clock time of the last refresh.
    pub last_updated: Instant,
}

impl PlanStripState {
    /// Build a new [`PlanStripState`] by scanning `workspace_root`.
    pub fn discover(workspace_root: &Path) -> Self {
        let files = discover_plan_files(workspace_root);
        let summaries: Vec<PlanSummary> =
            files.iter().filter_map(|p| parse_plan_summary(p)).collect();
        Self {
            summaries,
            last_updated: Instant::now(),
        }
    }

    /// Refresh summaries from disk.
    pub fn refresh(&mut self, workspace_root: &Path) {
        let files = discover_plan_files(workspace_root);
        self.summaries = files.iter().filter_map(|p| parse_plan_summary(p)).collect();
        self.last_updated = Instant::now();
    }

    /// Build the status bar text: `"Phase {phase} | {completed}/{total}"`.
    ///
    /// When multiple summaries exist, reports the aggregate across all plans.
    /// If no phase label is available, omits the "Phase" prefix.
    pub fn strip_text(&self) -> String {
        if self.summaries.is_empty() {
            return String::new();
        }
        let total: usize = self.summaries.iter().map(|s| s.total).sum();
        let completed: usize = self.summaries.iter().map(|s| s.completed).sum();
        // Use the first non-None phase label.
        let phase = self.summaries.iter().find_map(|s| s.phase.as_deref());
        if let Some(ph) = phase {
            format!("Phase {} | {}/{}", ph, completed, total)
        } else {
            format!("{}/{}", completed, total)
        }
    }
}

// ---------------------------------------------------------------------------
// PlanViewState — expanded modal overlay
// ---------------------------------------------------------------------------

/// Runtime state of the expanded plan view overlay.
///
/// Follows the [`PaletteState`] / [`WorkspaceSwitcherState`] pattern:
/// created when the overlay opens, dropped when closed.
pub struct PlanViewState {
    /// All plan summaries shown in the overlay.
    pub entries: Vec<PlanSummary>,
    /// Index into `entries` that is currently highlighted.
    pub selected: usize,
}

impl PlanViewState {
    /// Create a new [`PlanViewState`] with all summaries listed.
    pub fn new(entries: Vec<PlanSummary>) -> Self {
        Self {
            entries,
            selected: 0,
        }
    }

    /// Move selection up (wraps to last entry).
    // Keyboard navigation wired up in Phase 8 plan-view interactions.
    #[allow(dead_code)]
    pub fn select_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move selection down (clamps at last entry).
    // Keyboard navigation wired up in Phase 8 plan-view interactions.
    #[allow(dead_code)]
    pub fn select_down(&mut self) {
        if !self.entries.is_empty() && self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    /// Build quads for the plan view overlay.
    ///
    /// Returns `(dim_rect, box_rect, highlight_rect_opt)` as `[x,y,w,h]` arrays.
    pub fn render_quads(
        &self,
        window_width: f32,
        window_height: f32,
        cell_h: f32,
    ) -> Vec<crate::palette::PaletteQuad> {
        use crate::palette::PaletteQuad;
        let mut quads = Vec::new();

        // Full-screen dim.
        quads.push(PaletteQuad {
            rect: [0.0, 0.0, window_width, window_height],
            color: [0.0, 0.0, 0.0, 0.55],
        });

        let box_w = (window_width * 0.6).max(300.0);
        let row_count = self.entries.len().max(1) as f32;
        let box_h = cell_h * (row_count + 2.0);
        let box_x = (window_width - box_w) / 2.0;
        let box_y = (window_height - box_h) / 3.0;

        // Box background.
        quads.push(PaletteQuad {
            rect: [box_x, box_y, box_w, box_h],
            color: [0.13, 0.14, 0.18, 0.97],
        });

        // Title bar.
        quads.push(PaletteQuad {
            rect: [box_x, box_y, box_w, cell_h * 1.5],
            color: [0.18, 0.19, 0.24, 1.0],
        });

        // Selected-row highlight.
        if !self.entries.is_empty() {
            let row_y = box_y + cell_h * 1.5 + self.selected as f32 * cell_h;
            quads.push(PaletteQuad {
                rect: [box_x, row_y, box_w, cell_h],
                color: [0.30, 0.25, 0.55, 0.85],
            });
        }

        quads
    }

    /// Build text items for the plan view overlay.
    pub fn render_text(
        &self,
        window_width: f32,
        window_height: f32,
        cell_w: f32,
        cell_h: f32,
    ) -> Vec<crate::palette::PaletteText> {
        use crate::palette::PaletteText;
        let mut items = Vec::new();

        let box_w = (window_width * 0.6).max(300.0);
        let row_count = self.entries.len().max(1) as f32;
        let box_h = cell_h * (row_count + 2.0);
        let box_x = (window_width - box_w) / 2.0;
        let box_y = (window_height - box_h) / 3.0;
        let padding_x = cell_w;

        // Title.
        items.push(PaletteText {
            text: "Plan Status".to_string(),
            x: box_x + padding_x,
            y: box_y + (cell_h * 1.5 - cell_h) / 2.0,
        });

        // One row per plan summary.
        for (row, entry) in self.entries.iter().enumerate() {
            let label = if let Some(ref phase) = entry.phase {
                format!(
                    "Phase {} — {}/{} tasks — {}",
                    phase,
                    entry.completed,
                    entry.total,
                    entry
                        .file_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("?"),
                )
            } else {
                format!(
                    "{}/{} tasks — {}",
                    entry.completed,
                    entry.total,
                    entry
                        .file_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("?"),
                )
            };
            items.push(PaletteText {
                text: label,
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
    use std::path::PathBuf;

    use super::*;

    fn dummy_path() -> PathBuf {
        PathBuf::from("/tmp/PLAN.md")
    }

    // -----------------------------------------------------------------------
    // parse_plan_summary_from_str
    // -----------------------------------------------------------------------

    #[test]
    fn parses_completed_and_incomplete_checkboxes() {
        let text = "- [x] task one\n- [ ] task two\n- [x] task three\n";
        let summary = parse_plan_summary_from_str(text, &dummy_path()).unwrap();
        assert_eq!(summary.completed, 2, "two completed");
        assert_eq!(summary.total, 3, "three total");
    }

    #[test]
    fn no_checkboxes_returns_none() {
        let text = "# No checkboxes here\nJust some text.\n";
        let result = parse_plan_summary_from_str(text, &dummy_path());
        assert!(result.is_none(), "no checkboxes → None");
    }

    #[test]
    fn empty_file_returns_none() {
        let result = parse_plan_summary_from_str("", &dummy_path());
        assert!(result.is_none(), "empty file → None");
    }

    #[test]
    fn all_completed_counts_correctly() {
        let text = "- [x] one\n- [x] two\n- [x] three\n";
        let summary = parse_plan_summary_from_str(text, &dummy_path()).unwrap();
        assert_eq!(summary.completed, 3);
        assert_eq!(summary.total, 3);
    }

    #[test]
    fn all_incomplete_counts_correctly() {
        let text = "- [ ] one\n- [ ] two\n";
        let summary = parse_plan_summary_from_str(text, &dummy_path()).unwrap();
        assert_eq!(summary.completed, 0);
        assert_eq!(summary.total, 2);
    }

    #[test]
    fn extracts_phase_from_frontmatter() {
        let text = "---\nphase: ai-integration\n---\n- [x] task\n- [ ] other\n";
        let summary = parse_plan_summary_from_str(text, &dummy_path()).unwrap();
        assert_eq!(summary.phase.as_deref(), Some("ai-integration"));
        assert_eq!(summary.completed, 1);
        assert_eq!(summary.total, 2);
    }

    #[test]
    fn no_frontmatter_yields_no_phase() {
        let text = "- [x] one\n- [ ] two\n";
        let summary = parse_plan_summary_from_str(text, &dummy_path()).unwrap();
        assert!(summary.phase.is_none());
    }

    #[test]
    fn uppercase_x_counts_as_completed() {
        let text = "- [X] done\n- [ ] not done\n";
        let summary = parse_plan_summary_from_str(text, &dummy_path()).unwrap();
        assert_eq!(summary.completed, 1);
        assert_eq!(summary.total, 2);
    }

    #[test]
    fn multiple_checkboxes_on_same_line() {
        // Edge case: multiple checkboxes on one line.
        let text = "- [x] a [x] b [ ] c\n";
        let summary = parse_plan_summary_from_str(text, &dummy_path()).unwrap();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.completed, 2);
    }

    // -----------------------------------------------------------------------
    // PlanStripState
    // -----------------------------------------------------------------------

    #[test]
    fn strip_text_with_phase() {
        let strip = PlanStripState {
            summaries: vec![PlanSummary {
                phase: Some("7.2".to_string()),
                completed: 3,
                total: 5,
                file_path: dummy_path(),
            }],
            last_updated: std::time::Instant::now(),
        };
        assert_eq!(strip.strip_text(), "Phase 7.2 | 3/5");
    }

    #[test]
    fn strip_text_without_phase() {
        let strip = PlanStripState {
            summaries: vec![PlanSummary {
                phase: None,
                completed: 2,
                total: 4,
                file_path: dummy_path(),
            }],
            last_updated: std::time::Instant::now(),
        };
        assert_eq!(strip.strip_text(), "2/4");
    }

    #[test]
    fn strip_text_empty_when_no_summaries() {
        let strip = PlanStripState {
            summaries: vec![],
            last_updated: std::time::Instant::now(),
        };
        assert!(strip.strip_text().is_empty());
    }
}
