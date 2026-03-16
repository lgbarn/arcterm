---
phase: phase-10
plan: "2.1"
wave: 2
dependencies: ["1.1"]
must_haves:
  - Regression tests for ISSUE-002, ISSUE-003, ISSUE-004, ISSUE-005
  - ISSUE-006 cursor visibility fix (U+2588 substitution for blank cells)
  - Regression test for ISSUE-006
  - cargo test -p arcterm-app passes
  - cargo test -p arcterm-render passes
files_touched:
  - arcterm-app/src/input.rs
  - arcterm-render/src/text.rs
tdd: true
---

# PLAN-2.1 — Regression Tests + ISSUE-006 Cursor Fix

**Why this is Wave 2:** Depends on PLAN-1.1 completing. `arcterm-app` must compile before any tests can run. The ISSUE-006 change is in `arcterm-render` (which compiles today) but its test depends on being able to run `cargo test` for the workspace.

**Scope:** ISSUE-002 through ISSUE-005 are already fixed in the codebase. The Phase 10 success criteria require "each fix includes at least one regression test." This plan adds those tests and implements the one remaining code change: ISSUE-006 cursor visibility via U+2588 block glyph substitution.

---

## Tasks

<task id="1" files="arcterm-app/src/input.rs" tdd="true">
  <action>Add two regression tests to the existing `mod tests` block in `input.rs` for ISSUE-003 (Ctrl+\ and Ctrl+]).

The existing tests use `translate_named_key` which bypasses the Ctrl modifier path. The Ctrl+\ and Ctrl+] logic lives in `translate_key_event` under the `Key::Character(s) if ctrl` arm. Two approaches:

**Preferred approach:** Extract a helper function `fn ctrl_char_byte(ch: char) -> Option<Vec<u8>>` from the ctrl arm of `translate_key_event` (lines 22-41). This function takes a character and returns the ctrl byte sequence. Move the alphabetic check, `[`, `\`, and `]` logic into it. Have `translate_key_event` call this helper. Then test the helper directly without needing to construct `winit::event::KeyEvent` structs.

**Tests to add:**
- `ctrl_backslash_sends_0x1c` — assert `ctrl_char_byte('\\')` returns `Some(vec![0x1c])`
- `ctrl_bracket_right_sends_0x1d` — assert `ctrl_char_byte(']')` returns `Some(vec![0x1d])`
- `ctrl_a_sends_0x01` — assert `ctrl_char_byte('a')` returns `Some(vec![0x01])` (verifies the alphabetic path still works after refactor)

Note: ISSUE-002, ISSUE-004, and ISSUE-005 are integration-level fixes (PTY creation error handling, request_redraw after input, shell exit banner). They cannot be unit-tested without windowing/PTY infrastructure. The regression coverage for those is provided by: (a) the code paths are verified in RESEARCH.md as present, and (b) manual verification per the Phase 10 success criteria checklist. If the builder finds a way to add lightweight unit tests for any of them, do so, but do not block the plan on it.</action>
  <verify>cargo test -p arcterm-app -- ctrl_ --nocapture 2>&1 | tail -10</verify>
  <done>Three new tests pass: `ctrl_backslash_sends_0x1c`, `ctrl_bracket_right_sends_0x1d`, `ctrl_a_sends_0x01`. The `ctrl_char_byte` helper is extracted and tested without winit `KeyEvent` construction. `translate_key_event` still delegates to it correctly.</done>
</task>

<task id="2" files="arcterm-render/src/text.rs" tdd="true">
  <action>Implement ISSUE-006: cursor renders as visible block on blank/space cells.

**Step 1 — Signature change:** Add two parameters to `shape_row_into_buffer`:
```
fn shape_row_into_buffer(
    buf: &mut Buffer,
    row: &[arcterm_core::Cell],
    font_system: &mut FontSystem,
    palette: &RenderPalette,
    cursor_col: Option<usize>,   // NEW: Some(col) if this is the cursor row
)
```

**Step 2 — Substitution logic:** Inside the `.map(|cell|` closure (line 652-662), change to an indexed `.enumerate()`. When `cursor_col == Some(i)` and the cell character is `' '` or `'\0'`, substitute the character string with `"\u{2588}"` (U+2588, FULL BLOCK). This makes the text layer render a solid block glyph at the cursor position, complementing the existing quad-layer cursor rectangle.

**Step 3 — Update call sites:** Both callers already have the cursor in scope.
- `prepare_grid` (line 189): Pass `if row_idx == cursor.row { Some(cursor.col) } else { None }` as the new `cursor_col` argument.
- `prepare_grid_at` (line 279): Same pattern using the local `cursor` variable.

**Risk note from RESEARCH.md:** The substitution must happen only in the render path (inside `shape_row_into_buffer`), NOT in the stored `Cell` data. The `hash_row` function already includes `cursor.col` when hashing the cursor row, so moving the cursor away from a blank cell will correctly invalidate the row hash and trigger a re-shape without the block substitution. No hash changes needed.</action>
  <verify>cargo test -p arcterm-render -- cursor --nocapture 2>&1 | tail -10</verify>
  <done>The `shape_row_into_buffer` function accepts `cursor_col: Option<usize>`. When the cursor is on a blank/space cell, the text layer renders U+2588 instead of whitespace. Both call sites (`prepare_grid`, `prepare_grid_at`) pass the cursor column correctly.</done>
</task>

<task id="3" files="arcterm-render/src/text.rs" tdd="true">
  <action>Add a regression test for ISSUE-006 in the existing `mod tests` block of `text.rs`.

**Test: `cursor_on_blank_substitutes_block_glyph`**
- Construct a row of 5 default `Cell` values (character = `' '`).
- Create a `CursorPos { row: 0, col: 2 }`.
- Call the substitution logic (either test `shape_row_into_buffer` directly if feasible, or extract the substitution into a testable helper `fn substitute_cursor_blank(row: &[Cell], cursor_col: Option<usize>) -> Vec<char>` that returns the effective character for each cell).
- Assert that position 2 yields `'\u{2588}'` and all other positions yield `' '`.

If constructing a `FontSystem` and `Buffer` for testing `shape_row_into_buffer` directly is impractical (it requires font discovery at test time), extract the substitution decision into a pure function that can be unit tested independently. The important thing is that the substitution logic has test coverage.

Also run the full test suites to confirm no regressions:</action>
  <verify>cargo test -p arcterm-render 2>&1 | tail -5 && cargo test -p arcterm-app 2>&1 | tail -5</verify>
  <done>Test `cursor_on_blank_substitutes_block_glyph` passes. All existing `hash_row_*` tests still pass. `cargo test -p arcterm-render` and `cargo test -p arcterm-app` both report zero failures. `cargo clippy -p arcterm-render -- -D warnings` is clean.</done>
</task>
