# Plan 4.1: Reconnect AI Features, Delete Old Crates, Integration Tests

## Context

After Waves 1-3, arcterm runs on alacritty_terminal with the renderer reading from snapshots. The old crates (`arcterm-core`, `arcterm-vt`, `arcterm-pty`) are still in the workspace but no longer imported by any crate. This final plan reconnects AI-specific features that depend on the terminal (agent detection, context sharing), removes the old crates, and adds integration tests to verify end-to-end behavior.

## Dependencies

- Plan 3.1 (renderer rewired, entire app compiles against new types)

## Tasks

### Task 1: Verify and reconnect AI features
**Files:** `arcterm-app/src/ai_detect.rs`, `arcterm-app/src/context.rs`, `arcterm-app/src/main.rs`
**Action:** modify
**Description:**

**1. AI agent detection (`ai_detect.rs`):**
This module uses `detect_ai_agent(pid: u32)` which calls `process_comm()` / `process_args()` from `proc.rs`. It does NOT directly depend on arcterm-core/vt/pty — it takes a PID and reads `/proc/{pid}/comm`. Verify it still compiles and works with the new `Terminal::child_pid()` which returns `Some(u32)` (the stored PID extracted before EventLoop took ownership).

**2. Context sharing (`context.rs`):**
`collect_sibling_contexts` takes `&HashMap<PaneId, Terminal>` and calls `t.cwd()`. The `cwd()` method in the new Terminal reads `/proc/{pid}/cwd` using the stored child_pid. Verify this works. Also verify that `t.grid_state().grid.cursor` references (if any) are updated to use the snapshot or `lock_term()`.

**3. AI state in about_to_wait:**
The `AiAgentState::check(pid)` calls in the main loop use `terminal.child_pid()`. Verify these compile and function correctly.

**4. Pane context metadata:**
PaneContext stores `last_exit_code`, `cwd`, and `last_command`. Verify these are populated from the new Terminal's drain methods (`take_exit_codes`, `cwd()`).

**Acceptance Criteria:**
- `cargo check -p arcterm-app` succeeds with AI features compiling
- AI agent detection works: given a child PID, `detect_ai_agent` identifies known AI CLI tools
- Context sharing works: `collect_sibling_contexts` returns CWD and metadata for each pane
- Exit codes from OSC 133 are correctly stored in PaneContext

### Task 2: Delete old crates and clean up workspace
**Files:** `Cargo.toml` (workspace), `arcterm-app/Cargo.toml`, `arcterm-core/` (delete), `arcterm-vt/` (delete), `arcterm-pty/` (delete)
**Action:** refactor
**Description:**

**1. Remove crate directories:**
- Delete `arcterm-core/` directory entirely
- Delete `arcterm-vt/` directory entirely
- Delete `arcterm-pty/` directory entirely

**2. Update workspace Cargo.toml:**
- Remove `"arcterm-core"`, `"arcterm-vt"`, `"arcterm-pty"` from `[workspace] members`
- Remove `arcterm-core = { path = "arcterm-core" }`, `arcterm-vt = { path = "arcterm-vt" }`, `arcterm-pty = { path = "arcterm-pty" }` from `[workspace.dependencies]`
- Remove `portable-pty = "0.9"` from `[workspace.dependencies]` (no longer needed)
- Remove `vte = "0.15"` from `[workspace.dependencies]` ONLY IF no other crate uses it directly. Check: alacritty_terminal re-exports vte, and arcterm-render may need it for `vte::ansi::Color`. If `arcterm-render` uses vte types directly, keep it. If only via alacritty_terminal re-exports, remove it.

**3. Update arcterm-app/Cargo.toml:**
- Remove `arcterm-core = { path = "../arcterm-core" }`
- Remove `arcterm-vt = { path = "../arcterm-vt" }`
- Remove `arcterm-pty = { path = "../arcterm-pty" }`

**4. Grep for any remaining references:**
Search all `.rs` files for `arcterm_core`, `arcterm_vt`, `arcterm_pty`, `portable_pty`. Remove or replace any remaining references. Common locations:
- `use arcterm_core::*` in any file
- `use arcterm_vt::*` in any file
- `use arcterm_pty::*` in any file
- Doc comments referencing old crate names

**5. Verify clean build:**
```bash
cargo check --workspace
cargo clippy --workspace -- -D warnings
```

**Acceptance Criteria:**
- `arcterm-core/`, `arcterm-vt/`, `arcterm-pty/` directories do not exist
- `cargo check --workspace` succeeds
- `cargo clippy --workspace -- -D warnings` passes
- `grep -r "arcterm_core\|arcterm_vt\|arcterm_pty" --include="*.rs" --include="*.toml"` returns zero results (excluding .shipyard/ docs)
- Workspace has 3 members: `arcterm-render`, `arcterm-app`, `arcterm-plugin`

### Task 3: Add integration tests
**Files:** `arcterm-app/tests/engine_migration.rs` (new)
**Action:** create
**Description:**
Add integration tests that verify the migration preserved functional parity:

**1. Terminal creation test:**
```rust
#[tokio::test]
async fn terminal_creates_pty_and_reports_pid() {
    let (terminal, _wakeup_rx, _image_rx) = Terminal::new(...).unwrap();
    assert!(terminal.child_pid().is_some());
    assert!(terminal.child_pid().unwrap() > 0);
}
```

**2. PreFilter round-trip test:**
Feed raw bytes containing mixed OSC 7770, APC, and plain text through the PreFilter. Verify:
- Passthrough bytes are exactly the non-intercepted portion
- OSC 7770 params are extracted correctly
- APC payloads are extracted correctly

**3. Write-input test:**
Create a terminal, write `"echo hello\n"`, wait for wakeup, lock the term, read grid content, verify "hello" appears in the output.

**4. Resize test:**
Create a terminal at 80x24, resize to 120x40, verify the term's dimensions update.

**5. Structured content test (if feasible):**
Write an OSC 7770 start/content/end sequence to the PTY input, verify it passes through the pre-filter and appears in `take_completed_blocks()`.

Note: Some tests may be difficult to run in CI (PTY requires a TTY). Mark them with `#[ignore]` if they fail in headless environments, with a comment explaining how to run them locally.

**Acceptance Criteria:**
- `cargo test -p arcterm-app` passes (including new integration tests, possibly with some `#[ignore]` for CI)
- `cargo test --workspace` passes
- At least terminal creation, PreFilter round-trip, and write-input tests execute successfully
- No panics from any test

## Verification

```bash
cargo test --workspace && cargo clippy --workspace -- -D warnings
```

Full workspace compiles, all tests pass, clippy clean. The three old crates are deleted. AI features are connected. The migration is complete.
