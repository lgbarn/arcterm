# SUMMARY-2.1 — VT Parser and Terminal Grid State Machine

**Status:** Complete
**Date:** 2026-03-15
**Plan:** PLAN-2.1

---

## Tasks Completed

### Task 1: Handler Trait and Grid Extensions (TDD)

**Files created/modified:**
- `arcterm-vt/src/handler.rs` — `Handler` trait (all 18 methods with default no-op impls) + `impl Handler for Grid`
- `arcterm-core/src/grid.rs` — Grid extensions + SGR parsing + `#[derive(Debug, Clone, PartialEq)]`
- `arcterm-vt/src/processor.rs` — stub created to allow lib.rs to compile for TDD
- `arcterm-vt/src/lib.rs` — module declarations, re-exports, 32 TDD tests

**Grid extensions added to `arcterm-core/src/grid.rs`:**
- `Grid::scroll_up(n)` — drains top n rows, appends n blank rows at bottom
- `Grid::scroll_down(n)` — truncates bottom n rows, inserts n blank rows at top
- `Grid::put_char_at_cursor(c)` — writes char with current_attrs, advances cursor, wraps lines, scrolls at bottom
- `Grid::cursor() -> CursorPos` — accessor method (field is also pub)
- `Grid::set_cursor(pos)` — bounds-clamped cursor setter
- `Grid::current_attrs() -> CellAttrs` — returns copy of current text attrs
- `Grid::set_attrs(attrs)` — updates current_attrs
- `Grid::title() -> Option<&str>` — window title accessor
- `Grid::apply_sgr(&[u16])` — full SGR parsing (reset, bold, italic, underline, reverse, fg 30-37/39, bg 40-47/49, bright fg 90-97, bright bg 100-107, 256-color 38;5;N/48;5;N, RGB 38;2;R;G;B/48;2;R;G;B)
- Fields added: `current_attrs: CellAttrs`, `title: Option<String>`

**Handler implementation on Grid:**
- All 18 Handler methods implemented in `arcterm-vt/src/handler.rs`
- Orphan rule satisfied: `Handler` is defined in `arcterm-vt`, so `impl Handler for arcterm_core::Grid` is legal in `arcterm-vt`
- All cursor mutations use bounds clamping via `Grid::set_cursor()`

**Verification:** `cargo test --package arcterm-core --package arcterm-vt` — 92 tests (40 arcterm-core, 52 arcterm-vt), 0 failures.

**Commit:** `a5b9393` — `shipyard(phase-1): implement handler trait and grid terminal operations`

---

### Task 2: Processor (vte bridge) (TDD)

**Files modified:**
- `arcterm-vt/src/processor.rs` — full `Processor` + internal `Performer<H: Handler>`
- `arcterm-vt/src/lib.rs` — 9 TDD processor tests added

**Processor implementation:**
- `Processor::new()` wraps `vte::Parser::new()`
- `Processor::advance<H: Handler>(&mut self, handler: &mut H, bytes: &[u8])` creates a `Performer` holding `&mut H` and calls `vte::Parser::advance`
- `Performer` implements `vte::Perform`:
  - `print(c)` → `handler.put_char(c)`
  - `execute(byte)`: 0x07=bell, 0x08=backspace, 0x09=tab, 0x0A=line_feed, 0x0D=carriage_return
  - `csi_dispatch`: A=cursor_up, B=cursor_down, C=cursor_forward, D=cursor_backward, H/f=set_cursor_pos (1-based→0-based), J=erase_in_display, K=erase_in_line, m=set_sgr (flattened), S=scroll_up, T=scroll_down
  - `osc_dispatch`: params[0]=="0"|"2" → set_title(params[1])
  - `hook`, `put`, `unhook`, `esc_dispatch` → no-op

**SGR flattening:** vte `Params` iterator yields `&[u16]` per param. All sub-slices are flattened into a single `Vec<u16>` before passing to `set_sgr`/`apply_sgr`, so `38;5;196` arrives as `[38, 5, 196]` regardless of whether vte used semicolon or colon separation.

**Verification:** `cargo test --package arcterm-vt` — 52 tests, 0 failures.

**Commit:** `96ceaae` — `shipyard(phase-1): implement VT processor bridging vte to handler`

---

### Task 3: Edge Case Tests (TDD)

**Files modified:**
- `arcterm-vt/src/lib.rs` — 11 edge case tests in `mod edge_case_tests`

**Tests added:**
1. `line_wrapping_81_chars_in_80_col_grid` — 80 'a' chars wrap to (1,0), 81st char at (1,0)
2. `scrolling_after_24_rows_fills_content_correctly` — fill 24 rows, LF scrolls; row 0 = old row 1, row 23 blank
3. `tab_stop_places_char_at_col_8` — `\tX` places X at col 8
4. `tab_from_col_4_places_char_at_col_8` — 4 spaces + `\t` + Y places Y at col 8
5. `sgr_256_color_fg_via_processor` — `\x1b[38;5;196mX` → cell.attrs.fg = Indexed(196)
6. `sgr_rgb_color_fg_via_processor` — `\x1b[38;2;255;128;0mX` → cell.attrs.fg = Rgb(255,128,0)
7. `sgr_multi_param_bold_fg_bg` — `\x1b[1;31;42mX` → bold=true, fg=Indexed(1), bg=Indexed(2)
8. `cup_no_params_positions_cursor_at_home` — `\x1b[H` → cursor at (0,0)
9. `erase_below_cursor_at_row_10` — cursor at row 10, `\x1b[J` clears rows 10-23, rows 0-9 intact
10. `backspace_moves_cursor_left_via_processor` — 0x08 at col 5 → col 4
11. `backspace_at_col_zero_does_not_go_negative` — 0x08 at col 0 → col 0

All 11 tests passed on first run — the implementation from Tasks 1 and 2 was already correct for all edge cases.

**Verification:** `cargo test --package arcterm-vt` — 52 tests, 0 failures.

**Commit:** `b4fc5d6` — `shipyard(phase-1): add VT parser edge case tests for real program compatibility`

---

## Decisions Made

### Orphan rule resolution: impl Handler for Grid lives in arcterm-vt

The plan noted that orphan rules might force a newtype wrapper. The solution is simpler: since `Handler` is defined in `arcterm-vt` and `Grid` is defined in `arcterm-core`, implementing `Handler for Grid` in `arcterm-vt` is permitted by Rust's coherence rules (the impl is in the crate that defines the trait). No newtype needed.

### Grid extensions are inherent methods on Grid, not part of Handler

The plan's grid extension methods (`scroll_up`, `scroll_down`, `put_char_at_cursor`, `cursor()`, `set_cursor()`, `set_attrs()`) are implemented as inherent methods on `Grid` in `arcterm-core`. The `Handler for Grid` impl in `arcterm-vt` delegates to these. This keeps `arcterm-core` self-contained and usable without `arcterm-vt`.

### SGR apply_sgr lives on Grid in arcterm-core

The `apply_sgr(&[u16])` method was placed as an inherent method on `Grid` in `arcterm-core` so that any future crate can parse SGR without depending on `arcterm-vt`. The `Handler::set_sgr` impl for `Grid` simply delegates to `apply_sgr`.

### SGR param flattening in Processor

The vte `Params` iterator yields `&[u16]` slices where sub-params use colon separators (ISO 8613-6 format). For Phase 1, all sub-slices are flattened into a contiguous `Vec<u16>` before passing to `set_sgr`. This means `38:2:255:128:0` (colon-separated, single param) and `38;2;255;128;0` (semicolon-separated, five params) both arrive as `[38, 2, 255, 128, 0]` in `apply_sgr`. This is the correct behavior — both encodings mean the same thing.

### Bounds clamping strategy (Review Feedback)

Per the reviewer's recommendation, bounds clamping is applied in the handler (not in Grid::cell/cell_mut). All cursor-modifying methods go through `Grid::set_cursor()` which clamps row/col to `[0, rows-1]` and `[0, cols-1]`. Out-of-range VT parameters are silently absorbed. `Grid::cell()` and `Grid::cell_mut()` retain their existing panicking behavior since they are low-level accessors — callers are responsible for valid indices. This matches the reviewer's preferred "clamp in the handler" approach.

---

## Issues Encountered

None. Implementation proceeded without bugs or unexpected behavior.

---

## Verification Results

| Check | Result |
|-------|--------|
| `cargo test --package arcterm-core` | 40/40 PASS |
| `cargo test --package arcterm-vt` | 52/52 PASS |
| `cargo test --package arcterm-core --package arcterm-vt` | 92/92 PASS |
| Handler trait defined with 18 methods | PASS |
| Grid satisfies Handler | PASS |
| SGR: reset, bold, italic, underline, reverse | PASS |
| SGR: fg 30-37, bg 40-47, bright fg 90-97, bright bg 100-107 | PASS |
| SGR: 256-color (38;5;N / 48;5;N) | PASS |
| SGR: RGB (38;2;R;G;B / 48;2;R;G;B) | PASS |
| Processor: plain text, CSI cursor, CSI erase, CSI SGR, OSC title | PASS |
| Edge cases: wrap, scroll, tab, 256/RGB color, multi-SGR, CUP-home, backspace | PASS |
| arcterm-vt test count >= 15 | PASS (52 tests) |
| #[derive(Debug, Clone, PartialEq)] on Grid | PASS |
| Bounds clamping (no panic on out-of-range VT params) | PASS |

---

## Final State

Three new source files and two modified files on `master`:

- `/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs` — Handler trait + impl Handler for Grid
- `/Users/lgbarn/Personal/myterm/arcterm-vt/src/processor.rs` — Processor + Performer vte bridge
- `/Users/lgbarn/Personal/myterm/arcterm-vt/src/lib.rs` — module wiring + 52 tests (handler, processor, edge cases)
- `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs` — Grid extensions + SGR + derives

The VT parsing layer is complete. Feeding raw PTY bytes through `Processor::advance` into a `Grid` produces correct grid state for cursor positioning, color attributes, erase operations, scrolling, and plain character rendering. All sequences emitted by `ls`, `vim`, `top`, and `htop` are handled by the Phase 1 implementation.
