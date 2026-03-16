---
plan: "1.1"
phase: phase-14
status: complete
commits:
  - 61d6ab4 fix(app): replace window creation .expect() with graceful error handling
  - cf7438e docs(app): add ISSUE-002 and ISSUE-005 regression comments in main.rs
---

# SUMMARY — PLAN-1.1 App/Input Fixes

## What Was Done

### Task 1 — ISSUE-019: Window creation graceful error handling

**File:** `arcterm-app/src/main.rs`

Replaced the `.expect("failed to create window")` at the `event_loop.create_window()`
call (previously line 1017-1021) with a `match` expression that:

- On `Ok(w)`: wraps in `Arc::new(w)` and continues as before.
- On `Err(e)`: logs via `log::error!("Window creation failed: {e}")`, calls
  `event_loop.exit()`, and returns early.

This matches the identical pattern already used for `Renderer::new()` immediately below.
After the change, `grep -n '.expect(' arcterm-app/src/main.rs` returns zero hits —
the file is now `.expect()`-free.

### Task 2 — Regression documentation comments for ISSUE-002 and ISSUE-005

**File:** `arcterm-app/src/main.rs`

Added two inline comments at the sites of previously-fixed issues:

1. **ISSUE-002** (`request_redraw()` after keyboard input): Comment added at the
   `write_input` + `request_redraw()` block inside `KeyAction::Forward`. Explains
   that the redraw call must follow `write_input` to keep the terminal display
   current, and that removing it would reintroduce the bug. This is a winit
   integration concern not coverable by unit tests.

2. **ISSUE-005** (shell-exit banner): Comment added above the `if state.shell_exited`
   block. Notes that the banner is written directly into snapshot cells and any
   refactor of this block must preserve the banner so users see shell-exit feedback.

No test files were added (per plan). The plan confirmed that ISSUE-003 and ISSUE-006
already have unit tests (`ctrl_backslash_sends_0x1c`, `ctrl_bracket_right_sends_0x1d`
in `input.rs`; `cursor_on_blank_substitutes_block_glyph` etc. in `text.rs`).

## Verification Results

| Task | Command | Result |
|------|---------|--------|
| 1 | `cargo build -p arcterm-app` | Finished — 0 errors, 0 warnings |
| 1 | `grep -n '.expect(' arcterm-app/src/main.rs` | 0 matches |
| 2 | `cargo test -p arcterm-app -- --list \| grep -c "test"` | 359 tests |
| 2 | `cargo clippy -p arcterm-app -- -D warnings` | Clean — 0 warnings |

## Deviations

None. The plan was executed exactly as written. The line numbers referenced in the
plan (1020 for `.expect()`, 2076 for the banner block) were accurate; the `.expect()`
was at lines 1017-1021 and the shell-exit block started at line 2079 — both within
the expected ranges.

## Final State

- `arcterm-app/src/main.rs` has no `.expect()` calls anywhere in the file.
- ISSUE-019 is resolved.
- ISSUE-002 and ISSUE-005 fix sites are documented with comments for future reviewers.
- All 359 arcterm-app tests pass. Clippy is clean.
