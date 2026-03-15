---
phase: workspaces
plan: "3.2"
wave: 3
dependencies: ["2.1", "2.2"]
must_haves:
  - arcterm save <name> from CLI captures running session and writes workspace TOML
  - Leader+s triggers in-app session save with name prompt (future plan, stub wiring only)
  - Workspace restore under 500ms for a 4-pane layout (performance verification)
  - Full end-to-end test checklist covering all Phase 5 success criteria
files_touched:
  - arcterm-app/src/main.rs
  - arcterm-app/src/workspace.rs
tdd: false
---

# PLAN-3.2 -- Save Command, Performance Verification, and Integration Testing

## Goal

Complete the `arcterm save` CLI command (currently a stub from PLAN-2.1), verify that workspace restore meets the 500ms performance target for a 4-pane layout, and provide a comprehensive manual test checklist that maps to all five Phase 5 success criteria.

## Why Wave 3

Depends on the full workspace infrastructure from Waves 1 and 2. The save command needs the running app state (PLAN-2.1's App struct changes) and CWD capture (PLAN-1.2). Performance verification requires the complete restore path (PLAN-2.1 Task 2) to be wired.

## Design Notes

**`arcterm save <name>` from CLI**: This command cannot capture a running session from outside the app -- it requires access to `AppState`. Two approaches:
1. IPC: running arcterm instance listens on a Unix socket, CLI sends a save command.
2. In-app only: the CLI `save` command prints a message directing the user to use the in-app keybinding.

For Phase 5, approach 2 is chosen (simpler, matches CONTEXT-5.md scope). The in-app save path is triggered by a new `Leader+s` keybinding that calls `state.save_session()` with the user-provided name. The name input can reuse the palette text input pattern but is scoped as a future enhancement -- for Phase 5, `Leader+s` saves with a timestamp-based name (e.g., `session-2026-03-15-1423`).

**Performance target**: The 500ms target for a 4-pane layout covers: TOML parsing (~microseconds) + 4x PTY spawn (~50ms each on macOS) + 4x grid allocation (~microseconds) + layout tree construction (~microseconds). The bottleneck is PTY spawning. Four sequential spawns take ~200ms. This is well within the 500ms budget. Verification is a timed manual test.

## Tasks

<task id="1" files="arcterm-app/src/keymap.rs, arcterm-app/src/main.rs" tdd="true">
  <action>Add Leader+s keybinding for in-app workspace save.

1. Add `SaveWorkspace` variant to `KeyAction` enum in `keymap.rs`.

2. In the `LeaderPending` match arm, add `"s"` mapping to `Some(KeyAction::SaveWorkspace)`.

3. In `main.rs`, handle `KeyAction::SaveWorkspace`:
   - Generate a name based on current timestamp: `format!("session-{}", chrono_or_manual_timestamp)`. Use `std::time::SystemTime` to format as `YYYYMMDD-HHMM` without adding a chrono dependency.
   - Call `state.save_session()` with the generated name (reuse the save logic from PLAN-2.2, but write to `workspaces_dir().join(format!("{name}.toml"))` instead of `_last_session.toml`).
   - Log the save path at `info` level.

4. Add a `save_named_session(&self, name: &str) -> Result<(), workspace::WorkspaceError>` method to `AppState` that calls `save_session` logic but writes to the named file path.

Write test:
- `leader_then_s_saves_workspace`: enter leader, press 's', assert `KeyAction::SaveWorkspace`.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app -- keymap --nocapture</verify>
  <done>Leader+s produces `KeyAction::SaveWorkspace`. The save handler generates a timestamped workspace file in the workspaces directory.</done>
</task>

<task id="2" files="arcterm-app/src/workspace.rs" tdd="true">
  <action>Add performance benchmarking test and edge case tests for workspace restore.

1. Add test `workspace_toml_parse_under_1ms`: create a 4-pane workspace TOML string (HSplit(Leaf, VSplit(Leaf, VSplit(Leaf, Leaf)))), call `toml::from_str`, assert parse time is under 1ms using `Instant::now()` elapsed measurement. This validates the parsing portion of the 500ms budget.

2. Add test `workspace_file_with_no_panes_defaults_to_single_leaf`: create a WorkspaceFile with a single Leaf layout, serialize, deserialize, verify it produces exactly one pane on restore.

3. Add test `workspace_with_tilde_in_directory`: create a workspace with `directory = "~/projects/test"`, serialize, deserialize, verify the tilde is preserved as a literal string (expansion is the caller's responsibility at restore time).

4. Add test `workspace_with_empty_environment`: create a workspace with an empty `environment` HashMap, serialize, deserialize, verify round-trip produces an empty map (not a missing field).

5. Add test `workspace_large_tree_round_trips`: create a deeply nested tree (4 levels, 8 leaves), serialize to TOML, deserialize, assert equality. This stress-tests the serde enum representation.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app -- workspace --nocapture</verify>
  <done>All workspace tests pass including the 1ms parse benchmark, edge cases for empty environments, tilde paths, and deeply nested trees.</done>
</task>

<task id="3" files="" tdd="false">
  <action>Create a manual test checklist that maps to all five Phase 5 success criteria. This task produces no code changes -- it verifies the integration end-to-end.

Manual test procedure:

**SC-1: `arcterm open <workspace>` reads TOML and restores layout**
1. Create `~/.config/arcterm/workspaces/test-project.toml` with a 2-pane HSplit layout, left pane command `echo "left pane"`, right pane command `echo "right pane"`, directory `/tmp`.
2. Run `arcterm-app open test-project`.
3. Verify: window opens with two side-by-side panes. Left pane shows "left pane" output. Right pane shows "right pane" output. Both panes' shells are in `/tmp`.

**SC-2: Session persistence survives exit and reboot**
1. Open arcterm with default launch. Split into 2 panes. `cd /tmp` in one pane.
2. Close the window (Cmd+Q or click close).
3. Reopen arcterm with default launch (no arguments).
4. Verify: the 2-pane layout is restored. One pane's CWD is `/tmp`.

**SC-3: Leader+w opens fuzzy workspace switcher**
1. Create 3 workspace files in `~/.config/arcterm/workspaces/`: `alpha.toml`, `beta.toml`, `gamma.toml`.
2. Open arcterm. Press Ctrl+a then w.
3. Verify: dim overlay appears with a search box and three workspace names listed.
4. Type "al" -- only "alpha" remains visible.
5. Press Enter -- alpha workspace loads.
6. Reopen switcher (Ctrl+a, w), press Escape -- overlay dismisses, current session unchanged.

**SC-4: Workspace TOML files are human-readable and git-committable**
1. Run `arcterm-app save test-save` (via Leader+s from within arcterm).
2. Open `~/.config/arcterm/workspaces/session-*.toml` in a text editor.
3. Verify: the file is valid TOML with clear `[workspace]`, `[layout]`, and `[environment]` sections. No binary data. Suitable for `git add`.

**SC-5: Workspace restore under 500ms for 4-pane layout**
1. Create a workspace with 4 panes (HSplit(VSplit(Leaf, Leaf), VSplit(Leaf, Leaf))).
2. Run `RUST_LOG=info arcterm-app open four-pane-test`.
3. Measure time from process start to first frame render (use the latency-trace feature or wall-clock observation).
4. Verify: restore completes in under 500ms.</action>
  <verify>echo "Manual test checklist -- no automated verification. Run the five test scenarios described in the task action."</verify>
  <done>All five Phase 5 success criteria have been verified through the manual test procedure. Workspace open, session persistence, fuzzy switcher, human-readable TOML, and sub-500ms restore are all confirmed working.</done>
</task>
