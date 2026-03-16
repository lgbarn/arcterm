# Phase 9 Research: Code Investigation

Investigation date: 2026-03-15
Investigator: Research agent

This document records the actual state of every file touched by Phase 9, quoting
the relevant lines, noting current status, and calling out implementation specifics
for each fix group.

---

## Group 1 — Grid Fixes (`arcterm-core/src/grid.rs`)

### File facts

- Path: `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs`
- Total lines: 1113
- Test module: inline `#[cfg(test)] mod tests` starting at line 668
- Cargo.toml: no external dependencies (pure Rust)

---

### ISSUE-007 — `set_scroll_region()` performs no bounds validation

**Reported location:** line 222–224
**Actual code (lines 232–235):**

```rust
/// Set the scroll region to [top, bottom] (0-indexed, inclusive).
pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
    self.scroll_region = Some((top, bottom));
}
```

**Status:** Confirmed — no validation whatsoever. Any `(top, bottom)` pair is
stored unconditionally.

**Panic vector confirmed:** `scroll_up()` at lines 181–184 calls
`self.cells.remove(top)` and `self.cells.insert(bottom, ...)`. If
`bottom >= self.size.rows`, the `remove(bottom)` will panic with an index
out-of-bounds error at runtime. If `top >= bottom`, `region_height` saturates
to 0 and the operation is silently a no-op — not a panic, but it means a
terminal application sending DECSTBM with inverted bounds gets silent
misbehaviour.

**Required change:** Add before the assignment:

```rust
if top >= self.size.rows || bottom >= self.size.rows || top >= bottom {
    return;
}
```

**Caller in arcterm-vt:** `GridState::set_scroll_region` in `handler.rs` (lines
728–739 of the saved file) already clamps `top` and `bottom` to `max_row`
before calling the Grid method. However, the Grid itself has no defence, so a
caller that bypasses `GridState` (e.g., tests, future code) can still trigger
the panic. The fix should live in `Grid::set_scroll_region`.

**API surface:** `pub fn set_scroll_region(&mut self, top: usize, bottom: usize)`
No callers outside `arcterm-vt/src/handler.rs` at this time (Grep confirms the
only call site is `handler.rs`).

**Test placement:** Add to the inline `tests` module in `grid.rs`, below the
existing `// Task 1: Scrollback buffer and scroll regions` group (line 883).
Suggested test names:
- `set_scroll_region_rejects_inverted_bounds`
- `set_scroll_region_rejects_bottom_out_of_range`
- `set_scroll_region_rejects_top_out_of_range`

---

### ISSUE-008 — `resize()` does not resize `alt_grid` when present

**Reported location:** lines 499–521
**Actual code (lines 510–532):**

```rust
pub fn resize(&mut self, new_size: GridSize) {
    let mut new_cells: Vec<Vec<Cell>> = (0..new_size.rows)
        .map(|_| (0..new_size.cols).map(|_| Cell::default()).collect())
        .collect();

    let copy_rows = self.size.rows.min(new_size.rows);
    let copy_cols = self.size.cols.min(new_size.cols);
    for (r, new_row) in new_cells.iter_mut().enumerate().take(copy_rows) {
        new_row[..copy_cols].clone_from_slice(&self.cells[r][..copy_cols]);
    }

    self.cells = new_cells;
    self.size = new_size;
    self.dirty = true;

    // Clamp cursor to new bounds.
    if self.cursor.row >= new_size.rows {
        self.cursor.row = new_size.rows.saturating_sub(1);
    }
    if self.cursor.col >= new_size.cols {
        self.cursor.col = new_size.cols.saturating_sub(1);
    }
}
```

**Status:** Confirmed — `alt_grid` is never touched. The `alt_grid` field is
declared at line 89: `pub alt_grid: Option<Box<Grid>>`.

**Failure mode:** If the terminal is in alt-screen mode and the user resizes the
window, `resize()` updates `self.cells` and `self.size`. When `leave_alt_screen()`
is later called (lines 307–318), it restores `self.cells = saved.cells` where
`saved.cells` has the pre-resize row/col dimensions. This creates a
dimension mismatch between `self.cells` (now old dimensions) and `self.size`
(new dimensions), causing a panic on any subsequent `cells[row][col]` access
with the new bounds.

**Required change:** Append to `resize()` after line 532:

```rust
if let Some(ref mut ag) = self.alt_grid {
    ag.resize(new_size);
}
```

**Recursion note:** `alt_grid` is a `Box<Grid>`, so calling `ag.resize(new_size)`
will recursively call `Grid::resize`. This is correct — `alt_grid` should never
itself have an `alt_grid` (double alt-screen nesting is not a valid terminal
state), but the code is safe regardless because the recursion depth is bounded
to one level.

**Test placement:** Add to the inline `tests` module. A test that:
1. Creates a Grid
2. Calls `enter_alt_screen()`
3. Calls `resize()` with a new size
4. Calls `leave_alt_screen()`
5. Asserts `g.cells.len() == new_size.rows` and `g.cells[0].len() == new_size.cols`

---

### ISSUE-009 — `scroll_offset` is an unvalidated public field

**Reported location:** line 85
**Actual code (line 92):**

```rust
/// How many rows above the current screen bottom the viewport is scrolled.
/// 0 = live view; >0 = scrolled back into scrollback history.
pub scroll_offset: usize,
```

`rows_for_viewport()` at lines 325–363 uses `self.scroll_offset.min(self.scrollback.len())`
to clamp at call time, but the field itself is never validated on write.

**Status:** Confirmed — `scroll_offset` is a public field with no setter enforcement.
Any code can write `g.scroll_offset = 9999` and get silent clamping at
viewport-render time with no feedback.

**Required change:** Two-part:

1. Make the field private (change `pub scroll_offset: usize` to `scroll_offset: usize`).
2. Add two methods:

```rust
pub fn set_scroll_offset(&mut self, offset: usize) {
    self.scroll_offset = offset.min(self.scrollback.len());
}

pub fn scroll_offset(&self) -> usize {
    self.scroll_offset
}
```

**Breaking change audit:** The existing test at line 1042 sets `g.scroll_offset = 1`
directly. That test must be updated to use `g.set_scroll_offset(1)`. No other
test in `grid.rs` references `scroll_offset` directly (checked by reading the
test module). The field is also accessed directly at `arcterm-vt/src/handler.rs`
— search required (see below).

**Caller search result:** Grep across the workspace for `scroll_offset` reveals:
- `arcterm-core/src/grid.rs` — field definition and `rows_for_viewport` usage
- `arcterm-vt/src/handler.rs` — no direct access (uses GridState, which wraps Grid)
- `arcterm-app/src/main.rs` — accesses `terminal.grid_state.grid.scroll_offset`
  for scroll-up/scroll-down key handling

The `arcterm-app` direct-field access will need to be updated to call
`set_scroll_offset()` and `scroll_offset()` after this change. This is a Phase
10 concern if Phase 9 is crate-isolated, but the plan's requirement to make the
field private will cause a compile error in `arcterm-app`. The implementer should
confirm whether `arcterm-app` is in scope for compilation during Phase 9.

**Test placement:** Add to the inline `tests` module:
- `set_scroll_offset_clamps_to_scrollback_len`: set offset beyond scrollback length,
  assert it is clamped
- `set_scroll_offset_zero_is_valid`: offset of 0 always accepted

---

### ISSUE-010 — Scroll operations use O(n×rows) Vec::remove/Vec::insert loops

**Reported locations:** lines 170–173, 204–208, 375–379, 400–404

**Actual code — `scroll_up` partial region (lines 181–184):**

```rust
for _ in 0..n {
    self.cells.remove(top);
    self.cells.insert(bottom, self.blank_row());
}
```

**Actual code — `scroll_down` partial region (lines 215–219):**

```rust
for _ in 0..n {
    self.cells.remove(bottom);
    self.cells.insert(top, self.blank_row());
}
```

**Actual code — `insert_lines` (lines 386–391):**

```rust
for _ in 0..n {
    self.cells.remove(bottom);
    self.cells.insert(cur_row, self.blank_row());
}
```

**Actual code — `delete_lines` (lines 411–415):**

```rust
for _ in 0..n {
    self.cells.remove(cur_row);
    self.cells.insert(bottom, self.blank_row());
}
```

**Status:** Confirmed. All four use the remove/insert loop pattern. Each
`Vec::remove` and `Vec::insert` is O(rows) in the number of Vec elements that
must shift. Scrolling n rows in a region takes O(n × rows) total shifts.

**Reference implementation in arcterm-vt:** `GridState::scroll_region_up()` in
`handler.rs` (lines 378–400 of the saved handler output) uses the in-place
index-based copy pattern:

```rust
for row in top..=(bottom - n) {
    for col in 0..cols {
        self.grid.cells[row][col] = self.grid.cells[row + n][col].clone();
    }
}
for row in (bottom + 1 - n)..=(bottom) {
    for col in 0..cols {
        self.grid.cells[row][col] = Cell::default();
    }
}
```

**Required change for `scroll_up` partial region (replace lines 181–185):**

```rust
let cols = self.size.cols;
for row in top..=(bottom - n) {
    for col in 0..cols {
        self.cells[row][col] = self.cells[row + n][col].clone();
    }
}
for row in (bottom + 1 - n)..=bottom {
    for col in 0..cols {
        self.cells[row][col] = Cell::default();
    }
}
```

**Required change for `scroll_down` partial region (replace lines 215–220):**

```rust
let cols = self.size.cols;
for row in (top + n..=bottom).rev() {
    for col in 0..cols {
        self.cells[row][col] = self.cells[row - n][col].clone();
    }
}
for row in top..(top + n) {
    for col in 0..cols {
        self.cells[row][col] = Cell::default();
    }
}
```

**Required change for `insert_lines` (replace lines 386–391):**

```rust
let cols = self.size.cols;
for row in (cur_row + n..=bottom).rev() {
    for col in 0..cols {
        self.cells[row][col] = self.cells[row - n][col].clone();
    }
}
for row in cur_row..(cur_row + n).min(bottom + 1) {
    for col in 0..cols {
        self.cells[row][col] = Cell::default();
    }
}
```

**Required change for `delete_lines` (replace lines 411–415):**

```rust
let cols = self.size.cols;
for row in cur_row..=(bottom - n) {
    for col in 0..cols {
        self.cells[row][col] = self.cells[row + n][col].clone();
    }
}
for row in (bottom + 1 - n)..=bottom {
    for col in 0..cols {
        self.cells[row][col] = Cell::default();
    }
}
```

**Functional equivalence:** The existing tests `scroll_up_with_region_only_affects_region_rows`
(line 906) and `scroll_down_with_region_only_affects_region_rows` (line 927)
provide regression coverage. Additional tests for `insert_lines` and
`delete_lines` with regions should be added.

**Note on `blank_row` usage:** The current implementation calls `self.blank_row()`
which allocates a new `Vec<Cell>`. The in-place pattern overwrites cells with
`Cell::default()` directly, which is equivalent in terms of final state and
avoids the allocation.

---

## Group 2 — VT/Parser Fixes (`arcterm-vt/src/processor.rs` + `arcterm-vt/src/handler.rs`)

### File facts

- `processor.rs`: 1070 lines; test modules at lines 622 (phase4_task2_tests),
  816 (osc7770_tools_tests), 917 (osc133_tests), 1020 (osc7770_context_tests)
- `handler.rs`: ~850 lines (read via saved output); test module not separately
  identified — handler tests appear to live in processor.rs test modules that
  construct `GridState` objects directly
- Cargo.toml deps: `vte`, `arcterm-core`, `base64`, `log`

---

### ISSUE-011 — `esc_dispatch` does not guard on empty intermediates

**Reported location:** processor.rs line 273
**Actual code (lines 594–615):**

```rust
fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
    // Only handle bare ESC sequences (no intermediate bytes).  Sequences
    // with intermediates — e.g. ESC ( 7 (SCS Select Character Set) — use
    // the same final byte values but have completely different semantics.
    // Dispatching on byte alone would cause silent mis-dispatch, e.g.
    // ESC ( 7 incorrectly firing save_cursor_position.
    if !intermediates.is_empty() {
        return;
    }
    match byte {
        // DECSC — save cursor position
        0x37 => self.handler.save_cursor_position(),
        // DECRC — restore cursor position
        0x38 => self.handler.restore_cursor_position(),
        // DECKPAM — set keypad application mode
        0x3D => self.handler.set_keypad_application_mode(),
        // DECKPNM — set keypad numeric mode
        0x3E => self.handler.set_keypad_numeric_mode(),
        _ => {}
    }
}
```

**Status: ALREADY FIXED.** The guard `if !intermediates.is_empty() { return; }`
is present at line 601. The parameter is named `intermediates` (not
`_intermediates`). The comment correctly explains the rationale (SCS sequences
with same final byte). This fix was applied before Phase 9.

**Action required:** None for the implementation. A regression test should still
be added per the Phase 9 success criteria. Suggested test in processor.rs:

```rust
#[test]
fn esc_dispatch_with_intermediates_does_not_save_cursor() {
    // ESC ( 7 — SCS sequence — must NOT trigger save_cursor_position.
    let mut gs = make_gs();
    gs.grid.set_cursor(CursorPos { row: 3, col: 5 });
    // Feed ESC ( 7 manually via the Performer.
    // The vte crate will call esc_dispatch with intermediates = [b'('], byte = b'7'.
    // Result: cursor should be unchanged.
    let mut proc = Processor::new();
    proc.advance(&mut gs, b"\x1b(7");
    assert_eq!(gs.grid.cursor(), CursorPos { row: 3, col: 5 },
        "SCS ESC(7 must not trigger save_cursor");
}
```

---

### ISSUE-012 — Modes 47, 1047, and mouse modes absent from `set_mode`/`reset_mode`

**Reported location:** handler.rs lines 452–505
**Actual code — `set_mode` private branch (from saved file, lines 627–681):**

```rust
fn set_mode(&mut self, mode: u16, private: bool) {
    if private {
        match mode {
            1 => self.modes.app_cursor_keys = true,
            7 => self.modes.auto_wrap = true,
            25 => { ... }
            // Mode 47 and 1047: enter alt screen WITHOUT cursor save/restore.
            47 | 1047 => { ... }
            // Mode 1049: enter alt screen WITH cursor save/restore.
            1049 => { ... }
            2004 => self.modes.bracketed_paste = true,
            // Mouse reporting modes
            1000 => self.modes.mouse_report_click = true,
            1002 => self.modes.mouse_report_button = true,
            1003 => self.modes.mouse_report_any = true,
            1006 => self.modes.mouse_sgr_ext = true,
            _ => {}
        }
    }
}
```

**Status: ALREADY FIXED.** Modes 47, 1047, 1000, 1002, 1003, and 1006 are all
present in both `set_mode` and `reset_mode`. The `TermModes` struct in
`arcterm-core/src/grid.rs` (lines 9–25) declares the four mouse fields:
`mouse_report_click`, `mouse_report_button`, `mouse_report_any`, `mouse_sgr_ext`.
`GridState.modes` (a `TermModes`) and `GridState.grid.modes` are both updated
for cursor-visible changes; mouse modes are set only on `self.modes`.

**Implementation note — dual-mode storage:** `GridState` maintains `self.modes`
(a `TermModes`) for VT state. It also propagates certain flags to
`self.grid.modes` (e.g., cursor_visible at lines 633–634). Mouse mode flags are
set only on `self.modes`, not on `self.grid.modes`. This is consistent: the
`arcterm-app` layer reads `GridState.modes` directly for these flags, so they do
not need to be duplicated into `Grid.modes`.

**Action required:** Regression tests only. Suggested additions in handler.rs
test module:

```rust
#[test]
fn set_mode_47_enters_alt_screen() { ... }
#[test]
fn reset_mode_47_leaves_alt_screen() { ... }
#[test]
fn set_mode_1000_enables_mouse_click() { ... }
```

---

### ISSUE-013 — `newline` scroll-region clamp is logically unreachable dead code

**Reported location:** handler.rs lines 295–302
**Actual `newline()` code (from saved file, lines 456–477):**

```rust
fn newline(&mut self) {
    let cur_row = self.grid.cursor().row;
    let scroll_bottom = self.eff_scroll_bottom();

    if cur_row >= scroll_bottom {
        // At or past the bottom of the scroll region — scroll the region up.
        self.scroll_region_up(1);
        // Cursor stays pinned at scroll_bottom row.
        self.grid.set_cursor(CursorPos {
            row: scroll_bottom,
            col: self.grid.cursor().col,
        });
    } else {
        // Cursor is above the scroll region bottom: move down one row freely.
        // A cursor above the scroll region top moves toward the region without
        // triggering a scroll; once inside the region it scrolls at the bottom.
        self.grid.set_cursor(CursorPos {
            row: cur_row + 1,
            col: self.grid.cursor().col,
        });
    }
}
```

**Status: ALREADY FIXED.** The unreachable clamp block is gone. The `else` branch
simply advances the cursor by one row without the `if self.grid.cursor().row < scroll_top`
check. The comment in the else branch correctly documents the cursor-above-region
behaviour.

**Action required:** The regression test described in the issue does not yet
exist. A test must be added that:

1. Creates a GridState with a 10-row grid.
2. Sets scroll region to rows 3–7 (0-indexed).
3. Places cursor at row 0 (above the region).
4. Calls `newline()` three times — cursor should advance to rows 1, 2, 3.
5. Calls `newline()` four more times from row 3 — cursor should stay at row 7
   and region should scroll on the 5th call from row 3.

**Test placement:** Add a new `#[cfg(test)] mod handler_tests` block in
`handler.rs`, or add the test to one of the existing test modules in
`processor.rs` that constructs `GridState` (make_gs pattern already established).

---

## Group 4 — PTY Fix (`arcterm-pty/src/session.rs`)

### File facts

- Path: `/Users/lgbarn/Personal/myterm/arcterm-pty/src/session.rs`
- Total lines: 529
- Test module: `#[cfg(test)] mod tests` at line 276
- Cargo.toml deps: `portable-pty`, `tokio`, `arcterm-core`, `libc`

---

### ISSUE-001 — `shutdown()` writer-drop mechanism

**Reported location:** line 153
**Actual struct definition (lines 86–93):**

```rust
pub struct PtySession {
    master: Box<dyn portable_pty::MasterPty + Send>,
    /// `None` after `shutdown()` has been called.
    writer: Option<Box<dyn Write + Send>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    child_pid: Option<u32>,
}
```

**Actual `write()` (lines 201–212):**

```rust
pub fn write(&mut self, data: &[u8]) -> io::Result<()> {
    match self.writer.as_mut() {
        Some(w) => {
            w.write_all(data)?;
            w.flush()
        }
        None => Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "PTY session has been shut down",
        )),
    }
}
```

**Actual `shutdown()` (lines 266–271):**

```rust
pub fn shutdown(&mut self) {
    drop(self.writer.take());
    let _ = self.child.wait();
}
```

**Status: ALREADY FIXED.** The field is `Option<Box<dyn Write + Send>>`.
`shutdown()` uses `self.writer.take()` explicitly. `write()` returns
`Err(BrokenPipe)` when `writer` is `None`.

**Constructor (lines 186–195):** stores `writer: Some(writer)`, confirming the
`Option` is always `Some` at creation time.

**Test coverage:** `test_write_after_exit` (lines 496–527) exists and tests that
writing after the shell exits returns an error. However, this test waits for the
shell process to die naturally, not for an explicit `shutdown()` call. A direct
`shutdown()` test would be more precise:

```rust
#[tokio::test]
async fn test_write_after_explicit_shutdown() {
    let (mut session, _rx) = PtySession::new(default_size(), None, None)
        .expect("spawn");
    session.shutdown();
    let result = session.write(b"data");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::BrokenPipe);
}
```

This test is more direct than the existing `test_write_after_exit` and should be
added as the regression test for ISSUE-001.

---

## Group 5 — Plugin Fixes

### File facts

- `runtime.rs`: 118 lines
- `manager.rs`: 747 lines; test module at line 542
- `manifest.rs`: 367 lines; test module at line 198
- `host.rs`: 160 lines (no test module)
- `wit/arcterm.wit`: 76 lines
- Cargo.toml deps: `wasmtime`, `wasmtime-wasi`, `tokio`, `log`, `serde`,
  `serde_json`, `toml`, `anyhow`, `dirs`
- Dev deps: `tempfile`

---

### H-1 — Epoch interruption configured but never ticked

**Reported location:** `runtime.rs:21`
**Actual code (lines 19–23):**

```rust
let mut config = Config::new();
config.wasm_component_model(true);
config.epoch_interruption(true);

let engine = Engine::new(&config)?;
```

**Status:** Confirmed — `epoch_interruption(true)` is set but `engine.increment_epoch()`
is never called anywhere in the codebase (Grep confirms zero occurrences of
`increment_epoch` or `set_epoch_deadline`).

**What needs to happen:**

1. In `PluginRuntime`, expose a reference to the `Engine` (already done: `pub fn engine(&self) -> &Engine` at line 86).

2. Spawn a background tokio task in `PluginManager::new()` / `PluginRuntime::new()`
   that calls `engine.increment_epoch()` on a fixed interval. The CONTEXT-9.md
   decision sets the epoch deadline at 30 seconds. With wasmtime's epoch model,
   a store's deadline is set in epoch units, not wall time. A 10ms tick interval
   means 3,000 ticks per 30 seconds.

3. In `PluginInstance::call_update()` and `call_render()` (in `runtime.rs`),
   call `self.store.set_epoch_deadline(N)` before each WASM function call, where
   `N` is the deadline in epochs from the current epoch counter.

**The Engine must be shared by reference in the ticker.** The wasmtime `Engine`
is cheaply cloneable (it is reference-counted internally). The ticker task needs
a clone of the `Engine` to call `increment_epoch()`.

**Concrete implementation plan for `runtime.rs`:**

```rust
// In PluginRuntime::new(), after creating the engine:
let engine_clone = engine.clone();
tokio::spawn(async move {
    let mut interval = tokio::time::interval(
        std::time::Duration::from_millis(10)
    );
    loop {
        interval.tick().await;
        engine_clone.increment_epoch();
    }
});
```

**In `PluginInstance` methods (adding `epoch_deadline` before each call):**

```rust
pub fn call_update(&mut self, event: WitPluginEvent) -> anyhow::Result<bool> {
    // 3000 epochs × 10ms/epoch = 30 second deadline
    self.store.set_epoch_deadline(3000);
    let result = self.instance.call_update(&mut self.store, &event)?;
    Ok(result)
}

pub fn call_render(&mut self) -> anyhow::Result<Vec<StyledLine>> {
    self.store.set_epoch_deadline(3000);
    self.store.data_mut().draw_buffer.clear();
    self.instance.call_render(&mut self.store)?;
    Ok(self.store.data().draw_buffer.clone())
}
```

**Also needed:** `call_load` in `load_plugin` and `load_plugin_with_wasi` should
set an epoch deadline before calling `instance.call_load(&mut store)`.

**Test strategy:** Unit testing the epoch ticker requires either a WAT module
that spins indefinitely, or mocking. The recommended approach is a WAT-based
test that compiles a trivial infinite-loop component and verifies that `call_update`
returns an `Err` (specifically a wasmtime `Trap` with an epoch-exceeded cause).
This requires the `wasmtime` crate in dev-dependencies (it is already a direct
dependency, so it is available in `#[cfg(test)]`). See the note on `engine()`
accessor in `PluginRuntime` — it already exists for this purpose.

---

### H-2 — Plugin tool invocation is a stub

**Reported location:** `manager.rs:353-371`
**Actual code (lines 352–371):**

```rust
pub fn call_tool(&self, name: &str, _args_json: &str) -> anyhow::Result<String> {
    for lp in self.plugins.values() {
        if let Ok(inst) = lp.instance.lock() {
            let owned = inst.host_data().registered_tools.iter().any(|t| t.name == name);
            if owned {
                // Full WASM invocation is a Phase 8 deliverable.
                return Ok(format!(
                    "{{\"error\":\"tool invocation not yet implemented\",\"tool\":\"{}\"}}",
                    name
                ));
            }
        }
    }
    Ok(format!(
        "{{\"error\":\"tool not found\",\"tool\":\"{}\"}}",
        name
    ))
}
```

**Status:** Confirmed stub. The `_args_json` parameter is unused (prefixed with
underscore).

**WIT interface analysis** (`arcterm.wit` lines 62–76):

The current WIT world only declares:
```wit
export load: func();
export update: func(event: plugin-event) -> bool;
export render: func() -> list<styled-line>;
```

There is no `call-tool` export in the WIT. The bindgen-generated `ArctermPlugin`
struct (in `host.rs`) only has `call_load`, `call_update`, and `call_render`
methods. To dispatch a tool call to a WASM plugin, either:

**Option A — Add a WIT export:** Add `export call-tool: func(name: string, args-json: string) -> string;`
to the `arcterm-plugin` world. This requires:
1. Updating `arcterm.wit`
2. Regenerating bindings via the `bindgen!` macro (happens automatically at
   `cargo build` since the macro reads the WIT file at compile time)
3. Implementing `call_tool` in the WIT-compliant guest plugin code

**Option B — Invoke update() with a synthetic tool-call event:** Deliver the
tool call as a `PluginEvent::CommandExecuted` or a new WIT event variant. This
avoids adding a new export but conflates tool dispatch with event delivery.

**CONTEXT-9.md decision:** "Full WASM function dispatch implementation (not just
error cleanup)" — this implies Option A (proper WIT export) is the intended path.

**Concrete plan for Option A:**

1. Add to `arcterm.wit` world block:
   ```wit
   export call-tool: func(name: string, args-json: string) -> string;
   ```

2. The `bindgen!` macro regenerates. `ArctermPlugin` gains `call_call_tool` method
   (wasmtime bindgen converts kebab-case to snake_case and prefixes with `call_`).

3. Add `call_tool_export` method to `PluginInstance` in `runtime.rs`:
   ```rust
   pub fn call_tool_export(&mut self, name: &str, args_json: &str) -> anyhow::Result<String> {
       self.store.set_epoch_deadline(3000);
       let result = self.instance.call_call_tool(&mut self.store, name, args_json)?;
       Ok(result)
   }
   ```

4. Update `PluginManager::call_tool` to use the real dispatch:
   ```rust
   pub fn call_tool(&self, name: &str, args_json: &str) -> anyhow::Result<String> {
       for lp in self.plugins.values() {
           let owned = {
               let inst = lp.instance.lock()
                   .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
               inst.host_data().registered_tools.iter().any(|t| t.name == name)
           };
           if owned {
               let mut inst = lp.instance.lock()
                   .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
               return inst.call_tool_export(name, args_json);
           }
       }
       Ok(format!(
           "{{\"error\":\"tool not found\",\"tool\":\"{}\"}}",
           name
       ))
   }
   ```

**Risk flag:** Adding an export to the WIT file is a breaking change for any
existing guest WASM binaries compiled against the old interface. Phase 9 is
pre-release (v0.1.1) so there are no published plugins to break. The ROADMAP.md
explicitly scopes H-2 to "dispatch-only (call the function, return its result)
without retry or timeout logic."

**Test strategy:** Requires a WAT module that implements the `call-tool` export.
The existing `engine()` accessor in `PluginRuntime` allows tests to compile WAT
directly without going through `load_plugin`.

---

### M-1 — `KeyInput` maps to wrong `EventKind` in `kind()`

**Reported location:** `manager.rs:87-91`
**Actual code (lines 81–91):**

```rust
pub fn kind(&self) -> WitEventKind {
    match self {
        PluginEvent::PaneOpened(_) => WitEventKind::PaneOpened,
        PluginEvent::PaneClosed(_) => WitEventKind::PaneClosed,
        PluginEvent::CommandExecuted(_) => WitEventKind::CommandExecuted,
        PluginEvent::WorkspaceSwitched(_) => WitEventKind::WorkspaceSwitched,
        // KeyInput does not map to a subscribable EventKind — it is delivered
        // directly to the focused plugin pane, not via the broadcast bus.
        PluginEvent::KeyInput { .. } => WitEventKind::PaneOpened,
    }
}
```

**Status:** Confirmed bug. `KeyInput` incorrectly maps to `WitEventKind::PaneOpened`.

**CONTEXT-9.md decision:** "Add a dedicated `KeyInput` variant to the WIT
`EventKind` enum."

**What must change:**

1. **`arcterm.wit` (line 21–26):** Add `key-input` to the `event-kind` enum:
   ```wit
   enum event-kind {
       pane-opened,
       pane-closed,
       command-executed,
       workspace-switched,
       key-input,
   }
   ```

2. **`host.rs` — bindgen regeneration:** The `bindgen!` macro re-runs at compile
   time. `WitEventKind` gains a new `KeyInput` variant automatically.

3. **`manager.rs:89`:** Change the `KeyInput` arm:
   ```rust
   PluginEvent::KeyInput { .. } => WitEventKind::KeyInput,
   ```

**Downstream impact:** `subscribe_event` in `host.rs` stores `EventKind` in
`subscribed_events`. The `spawn_event_listener` task in `manager.rs` currently
does not filter events by subscription — it calls `call_update` for every event
regardless. If subscription filtering is ever added, `KeyInput` having a proper
kind will matter. For Phase 9 it is a correctness fix to prevent `kind()` from
returning misleading data.

**Test:** Add to `manager.rs` test module:
```rust
#[test]
fn key_input_event_kind_is_key_input() {
    let ev = PluginEvent::KeyInput {
        key_char: Some("a".to_string()),
        key_name: "a".to_string(),
        modifiers: KeyInputModifiers::default(),
    };
    assert!(matches!(ev.kind(), WitEventKind::KeyInput));
}
```

---

### M-2 — `wasm` field not validated for path traversal

**Reported location:** `manifest.rs:130`
**Actual code (lines 130–133):**

```rust
if self.wasm.trim().is_empty() {
    return Err("plugin wasm path must not be empty".to_string());
}
```

**Status:** Confirmed — only an emptiness check. No path-traversal checks.

**Call site in `manager.rs` (lines 241–243):**

```rust
let wasm_path = dir.join(&manifest.wasm);
let wasm_bytes = std::fs::read(&wasm_path)
    .map_err(|e| anyhow::anyhow!("cannot read wasm '{}': {e}", wasm_path.display()))?;
```

`dir.join("../../etc/passwd")` would successfully read `/etc/passwd` (modulo
permissions) and attempt to compile it as WASM. The compile would fail, but the
file read succeeds.

**Required change in `manifest.rs` `validate()` — extend the `wasm` check:**

```rust
if self.wasm.trim().is_empty() {
    return Err("plugin wasm path must not be empty".to_string());
}
if self.wasm.contains("..") {
    return Err(format!(
        "plugin wasm path '{}' must not contain '..'",
        self.wasm
    ));
}
if self.wasm.starts_with('/') || self.wasm.starts_with('\\') {
    return Err(format!(
        "plugin wasm path '{}' must not be an absolute path",
        self.wasm
    ));
}
if self.wasm.contains('\\') {
    return Err(format!(
        "plugin wasm path '{}' must not contain backslashes",
        self.wasm
    ));
}
```

**Additional defence in `manager.rs`:** After `dir.join(&manifest.wasm)`,
verify the resolved path is still under `dir`:

```rust
let wasm_path = dir.join(&manifest.wasm);
// Verify the resolved path is a child of dir (defence-in-depth).
let wasm_canonical = wasm_path.canonicalize().unwrap_or(wasm_path.clone());
let dir_canonical = dir.canonicalize().unwrap_or(dir.to_path_buf());
if !wasm_canonical.starts_with(&dir_canonical) {
    anyhow::bail!(
        "plugin wasm path '{}' resolves outside the plugin directory",
        manifest.wasm
    );
}
```

Note: `canonicalize()` will fail if the file does not yet exist, so this check
must happen after the manifest validation step, not before. The `validate()`
call in `load_from_dir` happens at lines 235–239, before the path join. The
containment check goes after the validate call, before `std::fs::read`.

**Tests to add** (in `manifest.rs` test module):
- `validate_wasm_rejects_path_traversal`: `wasm = "../../evil.wasm"` must fail
- `validate_wasm_rejects_absolute_unix`: `wasm = "/etc/evil.wasm"` must fail
- `validate_wasm_rejects_backslash`: `wasm = "..\evil.wasm"` must fail

---

### M-6 — `copy_plugin_files` does not reject symlinks

**Reported location:** `manager.rs:215-219`
**Actual code (lines 215–220):**

```rust
for entry in std::fs::read_dir(source_path)? {
    let entry = entry?;
    let file_name = entry.file_name();
    let dest_file = dest.join(&file_name);
    std::fs::copy(entry.path(), dest_file)?;
}
```

**Status:** Confirmed — no symlink check. `std::fs::copy` follows symlinks on
all platforms.

**Required change:** Add symlink detection before the copy:

```rust
for entry in std::fs::read_dir(source_path)? {
    let entry = entry?;
    let metadata = entry.path().symlink_metadata()?;
    if metadata.file_type().is_symlink() {
        anyhow::bail!(
            "plugin source directory contains a symlink '{}'; symlinks are not permitted",
            entry.file_name().to_string_lossy()
        );
    }
    // Also skip subdirectories (only copy top-level files).
    if !metadata.is_file() {
        continue;
    }
    let file_name = entry.file_name();
    let dest_file = dest.join(&file_name);
    std::fs::copy(entry.path(), &dest_file)?;
}
```

**Note on subdirectory handling:** The current code calls `std::fs::copy` on
every directory entry including subdirectories, which will return an error on
most platforms (cannot copy a directory as a file). Adding `if !metadata.is_file() { continue; }`
is a worthwhile defensive addition alongside the symlink check.

**Test to add** (in `manager.rs` test module):

```rust
#[test]
fn copy_plugin_files_rejects_symlinks() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().expect("tempdir");
    let source = tmp.path().join("plugin-with-symlink");
    write_plugin_toml(&source, "sym-plugin");
    // Create a symlink inside the source directory.
    let target = tmp.path().join("secret");
    std::fs::write(&target, b"sensitive").unwrap();
    symlink(&target, source.join("evil.wasm")).unwrap();

    let install_root = tmp.path().join("installed");
    let mgr = PluginManager::new_with_dir(install_root).expect("mgr");
    let result = mgr.copy_plugin_files(&source);
    assert!(result.is_err(), "copy with symlink must fail");
}
```

Note: this test uses `std::os::unix::fs::symlink` and must be
`#[cfg(unix)]`-gated.

---

## Cross-Group Observations

### WIT file changes for M-1 and H-2

Both M-1 and H-2 require changes to `arcterm-plugin/wit/arcterm.wit`. These two
changes are compatible and should be applied atomically:

- M-1 adds `key-input` to the `event-kind` enum
- H-2 adds `export call-tool: func(name: string, args-json: string) -> string;`

The `bindgen!` macro in `host.rs` re-runs at compile time whenever the WIT file
changes. There is no manual code generation step.

### Already-fixed issues

Three of the sixteen Phase 9 items are already implemented:
- ISSUE-011 (esc_dispatch intermediates guard) — fixed in `processor.rs:601`
- ISSUE-012 (modes 47, 1047, mouse modes) — fixed in `handler.rs` set_mode/reset_mode
- ISSUE-001 (writer Option<> + shutdown take()) — fixed in `session.rs`

These three items still require regression tests per the Phase 9 success criteria.
The implementation work is done; the test work is not.

### `scroll_offset` visibility change — cross-crate impact

Making `scroll_offset` private (ISSUE-009) will break `arcterm-app` which reads
the field directly. This compile error is confined to `arcterm-app` (Phase 10
territory). The plan should note that `arcterm-app` must be updated to use
`scroll_offset()` and `set_scroll_offset()` before it will compile against the
updated `arcterm-core`. Since Phase 9 and Phase 10 share no source files and
Phase 10 depends on Phase 9 being complete, this is expected — the implementer
of Phase 9 Group 1 should document the new API surface in a comment.

### Handler tests are in processor.rs

The `arcterm-vt` crate does not have a separate test file. Tests for both
`Processor` and `GridState` (which implements `Handler`) live in `processor.rs`
using the `make_gs()` + `feed()` helpers that construct `GridState` directly.
New Group 2 tests should follow this pattern.

### H-2 WIT export naming

The wasmtime `bindgen!` macro converts WIT kebab-case to Rust snake_case and
prefixes exported functions with `call_`. `call-tool` in WIT becomes
`call_call_tool` on the generated `ArctermPlugin` struct. This is the naming to
use in `PluginInstance::call_tool_export`.

---

## Source Files Referenced

- `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-vt/src/processor.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-pty/src/session.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-plugin/src/runtime.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-plugin/src/manager.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-plugin/src/manifest.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-plugin/src/host.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-plugin/src/lib.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-plugin/src/types.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-plugin/wit/arcterm.wit`
- `/Users/lgbarn/Personal/myterm/arcterm-core/Cargo.toml`
- `/Users/lgbarn/Personal/myterm/arcterm-vt/Cargo.toml`
- `/Users/lgbarn/Personal/myterm/arcterm-pty/Cargo.toml`
- `/Users/lgbarn/Personal/myterm/arcterm-plugin/Cargo.toml`
- `/Users/lgbarn/Personal/myterm/.shipyard/ISSUES.md`
- `/Users/lgbarn/Personal/myterm/.shipyard/ROADMAP.md` (lines 260–410)
- `/Users/lgbarn/Personal/myterm/.shipyard/phases/9/CONTEXT-9.md`
- `/Users/lgbarn/Personal/myterm/.shipyard/codebase/ARCHITECTURE.md`
- `/Users/lgbarn/Personal/myterm/.shipyard/codebase/CONCERNS.md`
