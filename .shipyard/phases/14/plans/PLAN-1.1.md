---
phase: phase-14
plan: "1.1"
wave: 1
dependencies: []
must_haves:
  - ISSUE-019 window creation graceful error (last .expect() in resumed())
  - Regression tests for all app-level fixes
  - No .expect() on fallible operations in arcterm-app runtime paths
files_touched:
  - arcterm-app/src/main.rs
tdd: false
---

# PLAN-1.1 — App/Input Fixes

## Context

Phase 12 (alacritty_terminal engine migration) already resolved ISSUE-002 through
ISSUE-006 in the current `arcterm-app` codebase:

- ISSUE-002: `request_redraw()` after keyboard input — line 2743 of main.rs
- ISSUE-003: Ctrl+\ (0x1c) and Ctrl+] (0x1d) — input.rs lines 26-31
- ISSUE-004: PTY creation graceful exit — `unwrap_or_else` at main.rs line 350
- ISSUE-005: Shell exit indicator — main.rs lines 1676-1682 + 2076-2097
- ISSUE-006: Cursor on blank cells — text.rs `substitute_cursor_char` with U+2588

The single surviving app-level issue is **ISSUE-019**: the window creation call at
main.rs:1020 still uses `.expect("failed to create window")`, which panics instead
of producing a user-facing error message. This is the last panicking path in
`resumed()` after GPU init was already hardened (Renderer::new returns Result).

## Tasks

<task id="1" files="arcterm-app/src/main.rs" tdd="false">
  <action>Replace the `.expect("failed to create window")` at line 1020 of main.rs with a `match` that logs the error via `log::error!`, calls `event_loop.exit()`, and returns early — matching the pattern already used for `Renderer::new()` at lines 1023-1030. The window creation block currently reads:
```
let window = Arc::new(
    event_loop
        .create_window(window_attrs)
        .expect("failed to create window"),
);
```
Change it to:
```
let window = match event_loop.create_window(window_attrs) {
    Ok(w) => Arc::new(w),
    Err(e) => {
        log::error!("Window creation failed: {e}");
        event_loop.exit();
        return;
    }
};
```
  </action>
  <verify>cargo build -p arcterm-app 2>&1 | tail -5</verify>
  <done>Build succeeds. `grep -n '\.expect(' arcterm-app/src/main.rs` shows zero hits in the `resumed()` function body (lines ~990-1230). The only `.expect()` calls remaining in main.rs are in test code or one-time init paths outside `resumed()`.</done>
</task>

<task id="2" files="arcterm-app/src/main.rs, arcterm-app/src/input.rs" tdd="false">
  <action>Audit that all previously-fixed ISSUE-002 through ISSUE-006 items have regression tests in the current codebase. Specifically verify:

1. ISSUE-003: Tests `ctrl_backslash_sends_0x1c` and `ctrl_bracket_right_sends_0x1d` exist in `input.rs` (lines 229-239). **Already present — no action needed.**

2. ISSUE-006: Tests `cursor_on_blank_substitutes_block_glyph`, `no_cursor_no_substitution`, and `cursor_on_non_blank_no_substitution` exist in `text.rs` (lines 888-921). **Already present — no action needed.**

3. ISSUE-002: There is no unit test that `request_redraw()` is called after keyboard input, because this is a winit integration concern not unit-testable. **Document in a comment at the `KeyAction::Forward` handler (line 2710) that ISSUE-002 requires the `request_redraw()` call after `write_input`, referencing the line for future reviewers.**

4. ISSUE-005: The shell-exit banner at lines 2076-2097 writes directly into snapshot cells. **Add a comment at line 2076 noting this implements ISSUE-005 and any refactor must preserve the banner.**

No new test files are needed. This task adds documentation comments only.
  </action>
  <verify>cargo test -p arcterm-app -- --list 2>&1 | grep -c "test" && cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -3</verify>
  <done>All existing tests pass. Clippy is clean. Comments documenting ISSUE-002 and ISSUE-005 locations are present in main.rs.</done>
</task>
