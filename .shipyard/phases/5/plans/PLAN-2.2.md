---
phase: workspaces
plan: "2.2"
wave: 2
dependencies: ["1.1", "1.2"]
must_haves:
  - Auto-save session to _last_session.toml on window close (CloseRequested event)
  - Auto-restore on default launch (no subcommand, no workspace) if _last_session.toml exists
  - Atomic file write (write .tmp then rename) to prevent corruption on crash
  - Fresh PaneId assignment on restore (no stale AtomicU64 values from previous session)
  - Session file captures pane tree shape, per-pane CWD, and window dimensions
files_touched:
  - arcterm-app/src/main.rs
  - arcterm-app/src/workspace.rs
tdd: false
---

# PLAN-2.2 -- Session Auto-Save and Auto-Restore

## Goal

Automatically save the current session state to `~/.config/arcterm/workspaces/_last_session.toml` when arcterm exits (via `CloseRequested` window event), and automatically restore it on the next default launch (no CLI subcommand, no explicit workspace). This provides session persistence across exit and reboot -- the second success criterion for Phase 5.

## Why Wave 2

Depends on PLAN-1.1 (workspace data model for `WorkspaceFile` serialization) and PLAN-1.2 (CWD capture for populating per-pane directories). Parallel with PLAN-2.1 (CLI subcommands) -- both consume the same data model but touch different parts of main.rs.

## Design Notes

**Save trigger**: The `WindowEvent::CloseRequested` handler in `App::window_event()` currently calls `event_loop.exit()`. Before exiting, it should capture the session and write it to disk. The save is synchronous (single file write, microseconds for a 4-pane layout) -- no need for async I/O.

**Auto-restore**: At the top of `main()`, after CLI parsing, if no subcommand was given, check for `_last_session.toml`. If it exists and parses successfully, use it as the `initial_workspace`. If it fails to parse, log a warning and proceed with a fresh single-pane session.

**Reserved filename**: `_last_session.toml` is a reserved name in the workspaces directory. The `list_workspaces()` function should skip files starting with `_` so the auto-save file does not appear in the workspace switcher or `arcterm list` output.

**Capture logic**: Use `workspace::capture_session()` from PLAN-1.1 Task 2. For each pane, call `terminal.cwd()` from PLAN-1.2 to get the current working directory. The command field is left as `None` for auto-save (we cannot reliably determine what command the user typed to start the shell; workspace files created by `arcterm save` may have commands specified, but auto-save captures state, not intent).

## Tasks

<task id="1" files="arcterm-app/src/main.rs, arcterm-app/src/workspace.rs" tdd="false">
  <action>Implement session auto-save on window close.

1. Add a `save_session(&self) -> Result<(), workspace::WorkspaceError>` method to `AppState` that:
   - Collects `PaneMetadata` for each pane by calling `terminal.cwd()` to get the directory. Command is `None` for auto-save. Env is `None`.
   - Calls `workspace::capture_session()` with the tab manager, tab layouts, pane metadata map, name `"_last_session"`, and current window dimensions from `self.window.inner_size()`.
   - Ensures `workspace::workspaces_dir()` exists (call `std::fs::create_dir_all`).
   - Calls `workspace_file.save_to_file(&workspace::workspaces_dir().join("_last_session.toml"))`.

2. In the `WindowEvent::CloseRequested` handler in `App::window_event()`, before `event_loop.exit()`, call `state.save_session()`. Log any error but do not prevent exit.

3. Update `workspace::list_workspaces()` to skip files whose name starts with `_` (filtering out `_last_session.toml` from user-facing listings).</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app 2>&1 | tail -5 && cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -10</verify>
  <done>Build succeeds. On window close, arcterm writes `_last_session.toml` to the workspaces directory. `list_workspaces()` skips files starting with `_`. Clippy clean.</done>
</task>

<task id="2" files="arcterm-app/src/main.rs" tdd="false">
  <action>Implement session auto-restore on default launch.

1. In `main()`, after CLI parsing and the `match cli.command` block, if the command is `None` (default launch) and `self.initial_workspace` is still `None`:
   - Check if `workspace::workspaces_dir().join("_last_session.toml")` exists.
   - If it exists, attempt `WorkspaceFile::load_from_file(&path)`.
   - On success, set `initial_workspace = Some(ws)`.
   - On error (corrupt file, version mismatch), log the error at `warn` level and proceed with a fresh single-pane session.

2. This integrates with the workspace restore logic already wired in PLAN-2.1 Task 2 -- the `resumed()` handler checks `initial_workspace` and spawns panes accordingly. No new restore code is needed here.

3. Add a log message at `info` level when auto-restoring: `"Restoring last session from {path}"`.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app 2>&1 | tail -5</verify>
  <done>Build succeeds. Default launch checks for `_last_session.toml` and restores the session if found. Corrupt session files are logged and skipped gracefully.</done>
</task>

<task id="3" files="arcterm-app/src/workspace.rs" tdd="true">
  <action>Add tests for the auto-save file path logic and the underscore-prefix filtering.

1. Add test `list_workspaces_skips_underscore_files`: create a temp directory, write `_last_session.toml` and `my-project.toml` into it, call a testable version of the listing logic, assert only `my-project` appears.

2. Add test `last_session_path_is_in_workspaces_dir`: assert `workspaces_dir().join("_last_session.toml")` contains "arcterm" and "workspaces" and ends with `_last_session.toml`.

3. Add test `save_to_file_creates_parent_dirs`: create a WorkspaceFile, save to a path under a non-existent nested directory in a temp dir, verify the file exists after save. (This tests that `save_to_file` or the caller creates parent dirs.)</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app -- workspace --nocapture</verify>
  <done>All workspace tests pass including the new auto-save path and filtering tests. Underscore-prefixed files are correctly excluded from listings.</done>
</task>
