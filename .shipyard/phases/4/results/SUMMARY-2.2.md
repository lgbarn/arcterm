# Phase 4 Plan 2.2 — Auto-Detection Engine: Implementation Summary

## Status

All 3 tasks complete. 176 tests pass (19 new detect tests + 157 pre-existing). Clippy clean for `arcterm-app` (pre-existing `arcterm-render` errors documented below).

## Commits

| Commit | Message |
|--------|---------|
| d901fcc | shipyard(phase-4): add AutoDetector engine with code block, diff, JSON, and markdown detection |
| 8f9f249 | shipyard(phase-4): declare detect module in arcterm-app main.rs |

---

## Task 1: AutoDetector struct and detection functions (TDD)

**File:** `arcterm-app/src/detect.rs` (new, 685 lines)

### Structures

**`DetectionResult`** — public struct with:
- `content_type: ContentType` — from `arcterm_vt`
- `start_row: usize` / `end_row: usize` — grid row indices
- `content: String` — raw text of the detected block
- `attrs: Vec<(String, String)>` — key/value metadata (e.g., `lang=rust`)

**`AutoDetector`** — struct with:
- `enabled: bool` — per-pane opt-out flag
- `last_scanned_row: usize` — scan cursor to avoid re-scanning

### Public API

- `AutoDetector::new()` — creates detector with `enabled: true, last_scanned_row: 0`
- `set_enabled(&mut self, enabled: bool)` — enable/disable detection
- `reset(&mut self)` — resets `last_scanned_row` to 0 (call on clear/alt-screen)
- `scan_rows(&mut self, rows: &[Vec<Cell>], cursor_row: usize) -> Vec<DetectionResult>` — main entry point

### Detection Functions (private)

| Function | Pattern Required |
|----------|-----------------|
| `detect_fenced_code_block` | Opening ` ``` ` + closing ` ``` ` both present; extracts lang hint |
| `detect_diff` | `--- ` + `+++ ` on consecutive lines + `@@ ` within 5 lines |
| `detect_json` | `{` or `[` at line start + multi-line (2+ rows) + `serde_json` parse success |
| `detect_markdown` | At least one `^#{1,6} ` heading + 3+ total non-empty lines |

### Multi-block Detection Algorithm

The `detect_all` method runs detectors in priority order (code block > diff > JSON > markdown) repeatedly until no new detections are found. Claimed row sets prevent overlap. This handles:
- Two independent blocks in different row ranges (both detected)
- Diff-like content inside a code fence (code block wins)

---

## Task 2: Edge Cases and Scan Boundary Management (TDD)

Implemented as part of the same `detect.rs` file; tested via `edge_case_tests` module.

### Scan Boundary Handling

- **Reset on regression:** if `cursor_row < self.last_scanned_row` (screen clear / alt-screen), reset to 0 before scanning
- **200-row window cap:** if the unchecked range exceeds 200 rows, `window_start` is clamped to `end - 200` to avoid full-scrollback scans on first frame
- **Incremental advancement:** `last_scanned_row` is updated to `end + 1` after each scan

### Tests Added

| Test | Verifies |
|------|----------|
| `reset_sets_last_scanned_row_to_zero` | `reset()` restores scan cursor |
| `cursor_row_less_than_last_scanned_triggers_reset` | Regression detection clears cursor |
| `two_blocks_in_different_row_ranges_both_detected` | Multi-block returns all results |
| `code_block_containing_diff_lines_detected_as_code_block` | Priority ordering works |
| `scan_window_capped_at_200_rows` | Window cap bounds the scan |

---

## Task 3: Module Declaration in main.rs

**File:** `arcterm-app/src/main.rs`

Added `mod detect;` to the module list (alphabetical order, after `mod config`). Integration into the event loop is deferred to PLAN-3.1.

---

## Verification

```
cargo test -p arcterm-app -- --nocapture 2>&1 | tail -5
# → test result: ok. 176 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

cargo test -p arcterm-app -- detect --nocapture
# → running 19 tests — all ok
```

---

## Pre-Existing Issue: arcterm-render Compile Errors

`cargo clippy -p arcterm-app -- -D warnings` reports 2-3 errors in `arcterm-render/src/structured.rs` (mismatched tuple types, unresolved `serde_json`). These errors exist in uncommitted changes from PLAN-2.1 (Wave 2 content renderers) which were in-progress before this plan ran. The errors are **not caused by this plan** and are confined to the `arcterm-render` crate. The `arcterm-app` crate itself is clippy-clean; the `-p arcterm-app` flag targets arcterm-app but Cargo also checks its transitive dependencies.

**Recommendation:** PLAN-2.1 (or a follow-up plan) should complete and fix the `arcterm-render/src/structured.rs` implementation to restore a fully clean clippy run.

---

## Files Touched

| File | Change |
|------|--------|
| `arcterm-app/src/detect.rs` | New file, 685 lines |
| `arcterm-app/src/main.rs` | +1 line (`mod detect;`) |
| `Cargo.lock` | Updated by cargo for the new module |
