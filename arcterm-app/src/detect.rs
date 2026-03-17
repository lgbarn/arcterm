//! Auto-detection engine for structured content in terminal output.
//!
//! Scans rows of terminal grid output for patterns indicating structured
//! content (code blocks, diffs, JSON, markdown) without requiring the
//! OSC 7770 protocol. Detection is conservative: false negatives are
//! preferred over false positives.

use arcterm_render::{ContentType, RenderSnapshot};

// ---------------------------------------------------------------------------
// DetectionResult
// ---------------------------------------------------------------------------

/// A detected structured content block within a range of terminal rows.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectionResult {
    pub content_type: ContentType,
    pub start_row: usize,
    pub end_row: usize,
    pub content: String,
    pub attrs: Vec<(String, String)>,
}

// ---------------------------------------------------------------------------
// AutoDetector
// ---------------------------------------------------------------------------

/// Heuristic auto-detector that scans terminal grid rows for structured
/// content patterns.
pub struct AutoDetector {
    enabled: bool,
    last_scanned_row: usize,
}

impl AutoDetector {
    /// Create a new detector in the enabled state.
    pub fn new() -> Self {
        Self {
            enabled: true,
            last_scanned_row: 0,
        }
    }

    /// Enable or disable detection. When disabled, `scan_rows` always returns
    /// an empty `Vec`.
    #[allow(dead_code)]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Reset the scan cursor to row 0. Call on terminal clear or alt-screen toggle.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.last_scanned_row = 0;
    }

    /// Scan the visible rows of `snapshot` for structured content and return
    /// any detections found.
    ///
    /// Only rows from `self.last_scanned_row` up to (and including)
    /// `cursor_row` are examined. After scanning, `last_scanned_row` is
    /// advanced so the same rows are not re-scanned on the next call.
    ///
    /// The scan window is capped at 200 rows per call to avoid expensive
    /// full-scrollback scans on the first frame.
    pub fn scan_rows(
        &mut self,
        snapshot: &RenderSnapshot,
        cursor_row: usize,
    ) -> Vec<DetectionResult> {
        if !self.enabled {
            return Vec::new();
        }

        let num_rows = snapshot.rows;

        // If the cursor moved backwards (screen clear / alt-screen toggle),
        // reset to avoid scanning negative ranges.
        if cursor_row < self.last_scanned_row {
            self.last_scanned_row = 0;
        }

        let start = self.last_scanned_row;
        let end = cursor_row.min(num_rows.saturating_sub(1));

        // Cap window at 200 rows.
        let window_start = if end.saturating_sub(start) > 200 {
            end.saturating_sub(200)
        } else {
            start
        };

        // Update last_scanned_row for next call.
        self.last_scanned_row = end + 1;

        if window_start > end {
            return Vec::new();
        }

        // Extract text from each row, trimming trailing whitespace.
        let text_rows: Vec<(usize, String)> = (window_start..=end)
            .map(|r| {
                let row = snapshot.row(r);
                let text: String = row.iter().map(|c| c.c).collect();
                (r, text.trim_end().to_string())
            })
            .collect();

        self.detect_all(&text_rows)
    }

    /// Run all detectors over the text rows and return all results, respecting
    /// priority order (code block > diff > JSON > markdown) and preventing
    /// overlapping row assignments.
    fn detect_all(&self, text_rows: &[(usize, String)]) -> Vec<DetectionResult> {
        let mut results: Vec<DetectionResult> = Vec::new();
        // Track which row indices are already claimed by a detection.
        let mut claimed: std::collections::HashSet<usize> = std::collections::HashSet::new();

        // Run each detector in priority order. Each detector gets the full
        // row list; we filter out already-claimed rows before returning.
        type Detector = fn(&[(usize, String)]) -> Option<DetectionResult>;
        let detectors: &[Detector] = &[
            detect_fenced_code_block,
            detect_diff,
            detect_json,
            detect_markdown,
        ];

        // For multi-block detection we run the detectors repeatedly until no
        // new detections are found.
        loop {
            // Build the unclaimed subset.
            let unclaimed_rows: Vec<(usize, String)> = text_rows
                .iter()
                .filter(|(r, _)| !claimed.contains(r))
                .cloned()
                .collect();

            if unclaimed_rows.is_empty() {
                break;
            }

            let mut found_any = false;
            for detector in detectors {
                if let Some(result) = detector(&unclaimed_rows) {
                    // Mark those rows as claimed.
                    for row in result.start_row..=result.end_row {
                        claimed.insert(row);
                    }
                    results.push(result);
                    found_any = true;
                    // Restart detector sweep so higher-priority detectors
                    // get first pick on the newly remaining rows.
                    break;
                }
            }

            if !found_any {
                break;
            }
        }

        // Sort results by start_row for predictable ordering.
        results.sort_by_key(|r| r.start_row);
        results
    }
}

// ---------------------------------------------------------------------------
// Detection functions (private)
// ---------------------------------------------------------------------------

/// Detect a fenced code block delimited by ``` markers.
///
/// Requires both an opening ``` line and a closing ``` line.  Extracts an
/// optional language hint from the opening fence.
fn detect_fenced_code_block(text_rows: &[(usize, String)]) -> Option<DetectionResult> {
    // Find the opening fence: a line matching ^\s*```\w*$
    let open_idx = text_rows.iter().position(|(_, line)| {
        let trimmed = line.trim();
        trimmed.starts_with("```")
            && trimmed[3..]
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    })?;

    let (open_row, open_line) = &text_rows[open_idx];
    let lang = open_line.trim()[3..].trim().to_string();

    // Find the closing fence: a subsequent line that is exactly ``` (no lang).
    let close_idx = text_rows[open_idx + 1..]
        .iter()
        .position(|(_, line)| line.trim() == "```")?;
    // close_idx is relative to open_idx+1.
    let close_row_tuple = &text_rows[open_idx + 1 + close_idx];
    let close_row = close_row_tuple.0;

    // Extract content between fences.
    let content_lines: Vec<&str> = text_rows[open_idx + 1..open_idx + 1 + close_idx]
        .iter()
        .map(|(_, l)| l.as_str())
        .collect();
    let content = content_lines.join("\n");

    let mut attrs = Vec::new();
    if !lang.is_empty() {
        attrs.push(("lang".to_string(), lang));
    }

    Some(DetectionResult {
        content_type: ContentType::CodeBlock,
        start_row: *open_row,
        end_row: close_row,
        content,
        attrs,
    })
}

/// Detect a unified diff.
///
/// Requires `--- a/` AND `+++ b/` on the next line, followed by `@@ `
/// within a few lines.
fn detect_diff(text_rows: &[(usize, String)]) -> Option<DetectionResult> {
    let n = text_rows.len();

    for i in 0..n.saturating_sub(2) {
        let (row_a, line_a) = &text_rows[i];
        if !line_a.starts_with("--- ") {
            continue;
        }

        // Next line must start with +++
        let (_, line_b) = &text_rows[i + 1];
        if !line_b.starts_with("+++ ") {
            continue;
        }

        // Find @@ within the next few lines (up to 5 after the +++ line).
        let hunk_search_end = (i + 2 + 5).min(n);
        let hunk_pos = text_rows[i + 2..hunk_search_end]
            .iter()
            .position(|(_, l)| l.starts_with("@@ "))?;

        // Find the end of the diff: last line starting with +, -, space, or @.
        let content_start = i + 2 + hunk_pos;
        let mut end_idx = content_start;
        for (j, (_, l)) in text_rows.iter().enumerate().take(n).skip(content_start) {
            if l.starts_with('+') || l.starts_with('-') || l.starts_with(' ') || l.starts_with("@@")
            {
                end_idx = j;
            } else {
                break;
            }
        }

        let content_lines: Vec<&str> = text_rows[i..=end_idx]
            .iter()
            .map(|(_, l)| l.as_str())
            .collect();
        let content = content_lines.join("\n");

        return Some(DetectionResult {
            content_type: ContentType::Diff,
            start_row: *row_a,
            end_row: text_rows[end_idx].0,
            content,
            attrs: Vec::new(),
        });
    }

    None
}

/// Detect a JSON object or array.
///
/// Requires the block to start with `{` or `[` at the beginning of a line,
/// span at least 2 lines, and parse successfully via `serde_json::from_str`.
fn detect_json(text_rows: &[(usize, String)]) -> Option<DetectionResult> {
    let n = text_rows.len();

    for i in 0..n {
        let (start_row, start_line) = &text_rows[i];
        let first_char = start_line.chars().next()?;
        if first_char != '{' && first_char != '[' {
            continue;
        }

        // Accumulate lines until we find a balanced closing brace/bracket.
        let mut depth: i32 = 0;
        let mut end_idx = i;

        for (j, (_, line)) in text_rows.iter().enumerate().take(n).skip(i) {
            for ch in line.chars() {
                if ch == first_char || ch == if first_char == '{' { '[' } else { '{' } {
                    // only track the outermost type for depth
                }
                if ch == '{' || ch == '[' {
                    depth += 1;
                } else if ch == '}' || ch == ']' {
                    depth -= 1;
                }
            }
            end_idx = j;
            if depth <= 0 {
                break;
            }
        }

        // Require at least 2 lines (single-line `{}` is too common in shell).
        if end_idx == i {
            continue;
        }

        // Require the block closes properly.
        if depth > 0 {
            continue;
        }

        let block_lines: Vec<&str> = text_rows[i..=end_idx]
            .iter()
            .map(|(_, l)| l.as_str())
            .collect();
        let block_text = block_lines.join("\n");

        // Validate with serde_json.
        let last_line = block_lines.last()?;
        let last_trimmed = last_line.trim();
        if !(last_trimmed.ends_with('}') || last_trimmed.ends_with(']')) {
            continue;
        }

        if serde_json::from_str::<serde_json::Value>(&block_text).is_err() {
            continue;
        }

        return Some(DetectionResult {
            content_type: ContentType::Json,
            start_row: *start_row,
            end_row: text_rows[end_idx].0,
            content: block_text,
            attrs: Vec::new(),
        });
    }

    None
}

/// Detect markdown content.
///
/// Requires at least one heading line (`^#{1,6} `) AND at least 3 total
/// non-empty lines.
fn detect_markdown(text_rows: &[(usize, String)]) -> Option<DetectionResult> {
    // Find the first heading.
    let heading_idx = text_rows.iter().position(|(_, line)| is_heading(line))?;

    let (start_row, _) = text_rows[heading_idx];

    // Count non-empty lines from the heading onwards.
    let non_empty_count = text_rows[heading_idx..]
        .iter()
        .filter(|(_, l)| !l.trim().is_empty())
        .count();

    if non_empty_count < 3 {
        return None;
    }

    // Find the last non-empty line as the end of the block.
    let end_idx = text_rows[heading_idx..]
        .iter()
        .rposition(|(_, l)| !l.trim().is_empty())
        .map(|rel| heading_idx + rel)
        .unwrap_or(heading_idx);

    let end_row = text_rows[end_idx].0;

    let content_lines: Vec<&str> = text_rows[heading_idx..=end_idx]
        .iter()
        .map(|(_, l)| l.as_str())
        .collect();
    let content = content_lines.join("\n");

    Some(DetectionResult {
        content_type: ContentType::Markdown,
        start_row,
        end_row,
        content,
        attrs: Vec::new(),
    })
}

/// Returns true if `line` is a markdown heading (1-6 `#` chars followed by a space).
fn is_heading(line: &str) -> bool {
    let trimmed = line.trim_start();
    let hashes: String = trimmed.chars().take_while(|&c| c == '#').collect();
    let len = hashes.len();
    (1..=6).contains(&len) && trimmed[len..].starts_with(' ')
}

// ---------------------------------------------------------------------------
// Helper: build a RenderSnapshot from string rows (for tests)
// ---------------------------------------------------------------------------

#[cfg(test)]
fn rows_from_strings(lines: &[&str]) -> RenderSnapshot {
    use alacritty_terminal::vte::ansi::CursorShape;
    use arcterm_render::SnapshotCell;

    let num_rows = lines.len();
    let num_cols = lines.iter().map(|l| l.len()).max().unwrap_or(1);
    let mut cells = vec![SnapshotCell::default(); num_rows * num_cols];
    for (r, line) in lines.iter().enumerate() {
        for (c, ch) in line.chars().enumerate() {
            if c < num_cols {
                cells[r * num_cols + c].c = ch;
            }
        }
    }
    RenderSnapshot {
        cells,
        cols: num_cols,
        rows: num_rows,
        cursor_row: 0,
        cursor_col: 0,
        cursor_visible: false,
        cursor_shape: CursorShape::Block,
    }
}

// ---------------------------------------------------------------------------
// Tests — Task 1: basic detection
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Fenced code block ---

    #[test]
    fn detects_fenced_code_block_with_lang() {
        let rows = rows_from_strings(&["```rust", "fn main() {}", "```"]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 2);
        assert_eq!(results.len(), 1, "expected one detection");
        let r = &results[0];
        assert_eq!(r.content_type, ContentType::CodeBlock);
        assert_eq!(r.attrs, vec![("lang".to_string(), "rust".to_string())]);
        assert_eq!(r.start_row, 0);
        assert_eq!(r.end_row, 2);
        assert!(r.content.contains("fn main()"), "content must include body");
    }

    #[test]
    fn no_detection_for_unclosed_code_fence() {
        // Opening ``` without closing: conservative, no detection.
        let rows = rows_from_strings(&["```python", "print('hello')"]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 1);
        assert!(results.is_empty(), "unclosed fence must not be detected");
    }

    #[test]
    fn no_false_positive_single_backtick_triple() {
        // A single ``` line with no content and no closing — no detection.
        let rows = rows_from_strings(&["```"]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 0);
        assert!(results.is_empty(), "single ``` must not trigger detection");
    }

    // --- Unified diff ---

    #[test]
    fn detects_unified_diff() {
        let rows = rows_from_strings(&[
            "--- a/file.rs",
            "+++ b/file.rs",
            "@@ -1 +1 @@",
            "-old",
            "+new",
        ]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 4);
        assert_eq!(results.len(), 1, "expected one diff detection");
        assert_eq!(results[0].content_type, ContentType::Diff);
    }

    #[test]
    fn single_dash_dash_dash_not_detected_as_diff() {
        // A markdown horizontal rule or section separator must not trigger diff.
        let rows = rows_from_strings(&["---", "Some text", "More text"]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 2);
        assert!(
            results.is_empty(),
            "bare '---' must not be detected as diff"
        );
    }

    // --- JSON ---

    #[test]
    fn detects_valid_json_object() {
        let rows = rows_from_strings(&[r#"{"#, r#"  "key": "value""#, r#"}"#]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 2);
        assert_eq!(results.len(), 1, "expected one JSON detection");
        assert_eq!(results[0].content_type, ContentType::Json);
    }

    #[test]
    fn invalid_json_not_detected() {
        // Shell brace expansion like `{foo}` must not be detected.
        let rows = rows_from_strings(&["{foo}", "  bar", "  baz"]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 2);
        assert!(results.is_empty(), "invalid JSON must not be detected");
    }

    #[test]
    fn single_line_empty_json_not_detected() {
        // `{}` on a single line is too common in shell output.
        let rows = rows_from_strings(&["{ }"]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 0);
        assert!(results.is_empty(), "single-line JSON must not be detected");
    }

    // --- Markdown ---

    #[test]
    fn detects_markdown_with_heading_and_body() {
        let rows = rows_from_strings(&["# Title", "", "Some paragraph text", "", "## Section"]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 4);
        assert_eq!(results.len(), 1, "expected one markdown detection");
        assert_eq!(results[0].content_type, ContentType::Markdown);
    }

    #[test]
    fn single_heading_line_not_detected_as_markdown() {
        // Need 3+ non-empty lines; a lone heading is not enough.
        let rows = rows_from_strings(&["# Just a title"]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 0);
        assert!(
            results.is_empty(),
            "single heading must not be detected as markdown"
        );
    }

    // --- Non-interference: shell output ---

    #[test]
    fn ls_output_produces_no_detections() {
        let rows = rows_from_strings(&[
            "total 64",
            "drwxr-xr-x  5 user group  160 Mar 15 10:00 .",
            "drwxr-xr-x 12 user group  384 Mar 15 09:00 ..",
            "-rw-r--r--  1 user group 1234 Mar 15 10:00 Cargo.toml",
            "-rw-r--r--  1 user group 5678 Mar 15 10:00 src",
        ]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 4);
        assert!(
            results.is_empty(),
            "ls output must not trigger any detection"
        );
    }

    #[test]
    fn shell_prompt_produces_no_detections() {
        let rows = rows_from_strings(&[
            "user@host:~$ ls",
            "file1.txt  file2.txt",
            "user@host:~$ echo hello",
            "hello",
            "user@host:~$ ",
        ]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 4);
        assert!(
            results.is_empty(),
            "shell prompts must not trigger any detection"
        );
    }

    // --- Disabled detector ---

    #[test]
    fn disabled_detector_returns_empty() {
        let rows = rows_from_strings(&["```rust", "fn main() {}", "```"]);
        let mut det = AutoDetector::new();
        det.set_enabled(false);
        let results = det.scan_rows(&rows, 2);
        assert!(
            results.is_empty(),
            "disabled detector must return empty results"
        );
    }
}

// ---------------------------------------------------------------------------
// Tests — Task 2: edge cases and scan boundary management
// ---------------------------------------------------------------------------

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn reset_sets_last_scanned_row_to_zero() {
        let mut det = AutoDetector::new();
        // Advance by scanning some rows.
        let rows = rows_from_strings(&["a", "b", "c"]);
        det.scan_rows(&rows, 2);
        assert_eq!(det.last_scanned_row, 3);
        det.reset();
        assert_eq!(det.last_scanned_row, 0);
    }

    #[test]
    fn cursor_row_less_than_last_scanned_triggers_reset() {
        let rows = rows_from_strings(&["a", "b", "c", "d", "e"]);
        let mut det = AutoDetector::new();
        det.scan_rows(&rows, 4); // advances to 5
        // Now scan with cursor_row=1 (< last_scanned_row=5): should reset and scan 0..=1
        let rows2 = rows_from_strings(&["```rust", "fn foo() {}", "```", "x", "y"]);
        det.scan_rows(&rows2, 1); // cursor < last, triggers reset; scans rows 0-1 (no complete block)
        assert_eq!(det.last_scanned_row, 2);
    }

    #[test]
    fn two_blocks_in_different_row_ranges_both_detected() {
        // Code block in rows 0-2, diff in rows 4-8.
        let rows = rows_from_strings(&[
            "```rust",      // 0
            "fn foo() {}",  // 1
            "```",          // 2
            "plain text",   // 3
            "--- a/foo.rs", // 4
            "+++ b/foo.rs", // 5
            "@@ -1 +1 @@",  // 6
            "-old",         // 7
            "+new",         // 8
        ]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 8);
        assert_eq!(results.len(), 2, "both blocks must be detected");
        assert_eq!(results[0].content_type, ContentType::CodeBlock);
        assert_eq!(results[1].content_type, ContentType::Diff);
    }

    #[test]
    fn code_block_containing_diff_lines_detected_as_code_block() {
        // A diff inside a code fence: the code block has priority.
        let rows = rows_from_strings(&[
            "```diff",       // 0
            "--- a/file.rs", // 1
            "+++ b/file.rs", // 2
            "@@ -1 +1 @@",   // 3
            "-old",          // 4
            "+new",          // 5
            "```",           // 6
        ]);
        let mut det = AutoDetector::new();
        let results = det.scan_rows(&rows, 6);
        assert_eq!(results.len(), 1, "only one detection (code block wins)");
        assert_eq!(results[0].content_type, ContentType::CodeBlock);
    }

    #[test]
    fn scan_window_capped_at_200_rows() {
        // Build 300 rows of plain content.
        let lines: Vec<&str> = (0..300).map(|_| "plain text line").collect();
        let rows = rows_from_strings(&lines);
        let mut det = AutoDetector::new();

        // First scan: cursor at row 299.  Window should be capped to rows 99-299 (200 rows).
        // last_scanned_row starts at 0, end=299, diff=299 > 200, so window_start=99.
        det.scan_rows(&rows, 299);
        // last_scanned_row must be updated to 300 (end+1).
        assert_eq!(det.last_scanned_row, 300);
    }
}
