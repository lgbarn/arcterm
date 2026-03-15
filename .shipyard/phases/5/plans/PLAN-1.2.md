---
phase: workspaces
plan: "1.2"
wave: 1
dependencies: []
must_haves:
  - CWD capture for child PTY process on macOS via proc_pidinfo
  - CWD capture on Linux via /proc/<pid>/cwd symlink
  - Fallback to None when CWD cannot be determined (no panic, no crash)
  - Terminal::new() accepts optional working directory parameter and spawns shell in that directory
files_touched:
  - arcterm-pty/src/session.rs
  - arcterm-app/src/terminal.rs
tdd: true
---

# PLAN-1.2 -- Per-Pane CWD Capture and CWD-Aware Spawn

## Goal

Add two capabilities that workspace serialization and restore depend on: (1) reading the current working directory of a running PTY child process, and (2) spawning a new PTY session in a specific working directory. These are independent of the workspace data model (PLAN-1.1) and can be built in parallel.

## Why This Must Come First

Session save (Wave 2) needs to capture the CWD of each pane's shell process. Session restore (Wave 2) needs to spawn panes in the correct directory. Without these, workspace files cannot capture or restore working directories -- a core Phase 5 requirement.

## Design Notes

**CWD capture on macOS**: Use `libc::proc_pidinfo` with `PROC_PIDVNODEPATHINFO` to get the CWD. The `libc` crate is already a dependency of `arcterm-app` (version 0.2). The function returns `Option<PathBuf>` -- if the syscall fails (permissions, zombie process), return `None`.

**CWD capture on Linux**: Read the `/proc/<pid>/cwd` symlink with `std::fs::read_link`. Return `None` on error.

**CWD-aware spawn**: `PtySession::new()` currently calls `CommandBuilder::new(&shell)` but does not set a working directory. Add an optional `cwd: Option<&Path>` parameter. When `Some`, call `cmd.cwd(cwd)` on the `CommandBuilder` before spawning. Thread this through `Terminal::new()` as well.

## Tasks

<task id="1" files="arcterm-pty/src/session.rs" tdd="true">
  <action>Add CWD capture and CWD-aware spawn to `PtySession`.

1. Add a `pub fn cwd(&self) -> Option<PathBuf>` method to `PtySession` that reads the current working directory of the child process:
   - Get the child PID from `self.child_pid`. If `None`, return `None`.
   - On macOS (`cfg(target_os = "macos")`): use `libc::proc_pidinfo` with `libc::PROC_PIDVNODEPATHINFO` and `libc::vnode_info_path` to read the CWD. Convert the `vip_path.vip_path` C string to `PathBuf`. Return `None` on any error.
   - On Linux (`cfg(target_os = "linux")`): read the symlink at `/proc/{pid}/cwd` via `std::fs::read_link`. Return `None` on error.
   - On other platforms: return `None`.

2. Modify `PtySession::new()` to accept an additional parameter `cwd: Option<&std::path::Path>`. When `Some(dir)`, call `cmd.cwd(dir)` on the `CommandBuilder` before `spawn_command`. When `None`, behavior is unchanged (shell starts in the arcterm process's CWD).

Write tests FIRST:
- `test_cwd_returns_some_after_spawn`: spawn a PTY, immediately read CWD, assert it returns `Some` and the path exists on disk.
- `test_cwd_changes_after_cd`: spawn a PTY, write `cd /tmp\n`, wait briefly, read CWD, assert it contains "/tmp" (or platform temp dir).
- `test_spawn_with_cwd`: spawn a PTY with `cwd = Some("/tmp")`, write `pwd\n`, read output, assert it contains "/tmp".
- `test_spawn_without_cwd_uses_process_cwd`: spawn a PTY with `cwd = None`, verify CWD matches `std::env::current_dir()`.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-pty -- cwd --nocapture</verify>
  <done>All CWD tests pass. `cwd()` returns the child process's working directory on macOS (and Linux if tested). `PtySession::new()` with a `cwd` argument spawns the shell in the specified directory.</done>
</task>

<task id="2" files="arcterm-app/src/terminal.rs, arcterm-app/src/main.rs" tdd="false">
  <action>Thread the CWD parameter through `Terminal::new()` and update all call sites.

1. Add `cwd: Option<&std::path::Path>` parameter to `Terminal::new()`, passing it through to `PtySession::new()`.

2. Add a `pub fn cwd(&self) -> Option<std::path::PathBuf>` method on `Terminal` that delegates to `self.pty.cwd()`.

3. Update `spawn_pane()` in `main.rs` to pass `None` as the CWD (preserving existing behavior). Add a `spawn_pane_with_cwd(&mut self, size: GridSize, cwd: Option<&Path>) -> PaneId` method that passes the CWD through.

4. Update any other call sites of `Terminal::new()` to include the new `cwd: None` parameter.

5. Run full test suite and clippy to verify zero regressions.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app -- --nocapture 2>&1 | tail -20 && cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -10</verify>
  <done>All existing tests pass. `Terminal::new()` accepts optional CWD. `spawn_pane_with_cwd()` is available for workspace restore. Clippy clean.</done>
</task>

<task id="3" files="arcterm-pty/src/session.rs" tdd="false">
  <action>Run the full arcterm-pty test suite and clippy to verify the new `cwd` parameter and `cwd()` method do not break existing tests.

1. Run `cargo test -p arcterm-pty` -- all existing spawn, write, resize, exit detection, and write-after-exit tests must still pass with the updated `PtySession::new()` signature.

2. Run `cargo clippy -p arcterm-pty -- -D warnings`.

3. Verify that the existing `PtySession::new(default_size(), None)` call pattern still compiles after adding the `cwd` parameter (callers must now pass two `None` values or use a builder pattern -- verify the approach is clean).</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-pty -- --nocapture && cargo clippy -p arcterm-pty -- -D warnings</verify>
  <done>All arcterm-pty tests pass. Clippy clean. Existing test call sites compile correctly with the updated signature.</done>
</task>
