---
phase: structured-output
plan: "2.2"
wave: 2
dependencies: ["1.1"]
must_haves:
  - Auto-detection engine with regex heuristics for code blocks, diffs, JSON, and markdown
  - Conservative thresholds (false negatives preferred over false positives)
  - Per-pane opt-out capability
  - Non-protocol output (ls, top, vim) produces zero false positive detections
files_touched:
  - arcterm-app/src/detect.rs (new)
  - arcterm-app/src/main.rs (mod declaration only)
tdd: true
---

# PLAN-2.2 -- Auto-Detection Engine

## Goal

Implement the regex-based heuristic engine that detects structured content in plain terminal output (non-OSC-7770) and tags it for rich rendering. This enables structured rendering even with tools that do not emit the OSC 7770 protocol. The engine runs in the app layer after VT processing, inspecting the text content of grid rows.

## Why Wave 2

Auto-detection consumes the `ContentType` enum from PLAN-1.1 and produces `StructuredBlock` data consumed by PLAN-3.1. It runs in parallel with PLAN-2.1 (content renderers) since it only produces metadata, not rendered spans.

## Design Notes

Detection operates on a sliding window of recent grid output. When new PTY data arrives, the detector scans the most recently written rows for patterns. Detection rules:

1. **Fenced code blocks**: require both opening ` ``` ` and closing ` ``` ` markers. Extract language hint from opening marker.
2. **Unified diffs**: require at least `--- a/` AND `+++ b/` on consecutive lines followed by `@@ ` hunk headers.
3. **JSON**: line starts with `{` or `[`, and `serde_json::from_str` succeeds on the complete object/array.
4. **Markdown**: 3+ lines where at least one starts with `# ` (heading) and at least one other line is non-empty non-heading text.

All detection is conservative: require strong multi-line signals before committing. A single `{` at a shell prompt must NOT trigger JSON mode.

## Tasks

<task id="1" files="arcterm-app/src/detect.rs" tdd="true">
  <action>Create `arcterm-app/src/detect.rs` with the auto-detection engine:

1. Define `DetectionResult` struct:
   ```
   pub struct DetectionResult {
       pub content_type: ContentType,
       pub start_row: usize,
       pub end_row: usize,
       pub content: String,
       pub attrs: Vec<(String, String)>,
   }
   ```

2. Define `AutoDetector` struct:
   ```
   pub struct AutoDetector {
       enabled: bool,
       last_scanned_row: usize,
   }
   ```

3. Implement `AutoDetector::new() -> Self` with `enabled: true, last_scanned_row: 0`.

4. Implement `AutoDetector::set_enabled(&mut self, enabled: bool)`.

5. Implement `AutoDetector::scan_rows(&mut self, rows: &[Vec<arcterm_core::Cell>], cursor_row: usize) -> Vec<DetectionResult>`:
   - Only scan rows from `self.last_scanned_row` to `cursor_row` (avoid re-scanning).
   - Update `self.last_scanned_row` after scan.
   - Extract text content from Cell rows (join cell.c characters, trim trailing whitespace per row).
   - Apply detection functions in priority order: fenced code block > diff > JSON > markdown.
   - If a detection matches, add to results and skip those rows for subsequent detections.
   - Return Vec of DetectionResults.

6. Implement detection functions (private):
   - `detect_fenced_code_block(text_rows: &[(usize, String)]) -> Option<DetectionResult>`: scan for a line matching `^\s*```\w*$`, then find the closing ` ``` `. Require both markers present. Extract language hint from opening marker.
   - `detect_diff(text_rows: &[(usize, String)]) -> Option<DetectionResult>`: scan for `^--- a/` followed by `^+++ b/` on the next line, followed by `^@@ `. All three must be present.
   - `detect_json(text_rows: &[(usize, String)]) -> Option<DetectionResult>`: find a line starting with `{` or `[`. Accumulate lines until a balanced closing `}` or `]` is found. Validate with `serde_json::from_str`. Require at least 2 lines (single-line `{}` is too common in shell output).
   - `detect_markdown(text_rows: &[(usize, String)]) -> Option<DetectionResult>`: require at least one line matching `^#{1,6}\s` AND at least 3 total non-empty lines.

Write tests first:
- Fenced code block: rows containing ` ```rust\nfn main() {}\n``` ` detected as CodeBlock with lang=rust
- Fenced code block: opening ` ``` ` without closing returns no detection (conservative)
- Diff: `--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-old\n+new` detected as Diff
- Diff: single `---` line (e.g., markdown horizontal rule) NOT detected as diff
- JSON: `{\n  "key": "value"\n}` detected as Json
- JSON: `{foo}` (shell brace expansion) NOT detected (parse fails)
- JSON: single-line `{}` NOT detected (too short)
- Markdown: `# Title\n\nSome paragraph text\n\n## Section` detected as Markdown
- Markdown: single `# ` line alone NOT detected (need 3+ lines)
- Plain `ls -la` output: NO detection triggered
- Plain shell prompt with `$`: NO detection triggered
- Disabled detector returns empty results</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app -- detect --nocapture</verify>
  <done>All detection tests pass. Code blocks, diffs, JSON, and markdown are correctly detected with conservative thresholds. Shell output, single-line JSON, incomplete code blocks, and other false-positive scenarios correctly produce no detection.</done>
</task>

<task id="2" files="arcterm-app/src/detect.rs" tdd="true">
  <action>Add edge case handling and scan boundary management:

1. Implement `AutoDetector::reset(&mut self)`: reset `last_scanned_row` to 0. Called on terminal clear (ESC[2J) or alt screen toggle.

2. Handle scan window boundaries:
   - If `cursor_row < self.last_scanned_row` (screen was cleared or scrolled), reset to 0.
   - Cap scan window to at most 200 rows per call to avoid scanning the entire scrollback on first frame.

3. Handle multi-block detection in a single scan: if rows 5-10 contain a code block and rows 15-20 contain a diff, both should be returned.

4. Handle overlapping patterns: if a fenced code block contains `--- a/` lines (e.g., a diff inside a code block), the code block detection takes priority (first match wins for overlapping row ranges).

Write tests first:
- Reset sets last_scanned_row to 0
- Cursor row less than last_scanned_row triggers reset
- Two blocks in different row ranges both detected
- Code block containing diff-like content detected as code block, not diff
- Scan window cap: 300 rows of content, only last 200 scanned</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app -- detect --nocapture</verify>
  <done>Edge case tests pass. Reset works. Multi-block detection returns all blocks. Overlapping patterns resolved by priority. Scan window is bounded.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs" tdd="false">
  <action>Wire the detect module into the app:

1. Add `mod detect;` to `arcterm-app/src/main.rs` module declarations.

2. Run the full arcterm-app test suite and clippy to verify zero regressions.

Note: the actual integration of AutoDetector into the event loop is deferred to PLAN-3.1 (integration plan). This task only declares the module.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app -- --nocapture 2>&1 | tail -5 && cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -5</verify>
  <done>Module declared. All existing tests pass. Clippy clean.</done>
</task>
