---
plan: "2.1"
phase: workspaces
status: complete
commits:
  - e43b5b8
  - 98416b5
  - 48e84b5
---

# SUMMARY-2.1 -- CLI Subcommands with clap 4

## What Was Done

### Task 1 -- Add clap 4 and define CLI structure

**Files:** `arcterm-app/Cargo.toml`, `arcterm-app/src/main.rs`

Added `clap = { version = "4", features = ["derive"] }` to `[dependencies]`.

Defined `Cli` and `CliCommand` structs at the top of `main.rs` using clap derive macros. Three subcommands: `Open { name: String }`, `Save { name: String }`, `List`.

Added `initial_workspace: Option<workspace::WorkspaceFile>` field to `App`.

At the top of `main()`, after `env_logger::init()`, the CLI is parsed and dispatched:
- `List` — calls `workspace::list_workspaces()`, prints names one per line (or "No workspaces found."), then returns (no GUI).
- `Save` — prints a stub message to stderr directing the user to `Leader+s` within arcterm, then returns.
- `Open { name }` — resolves `~/.config/arcterm/workspaces/{name}.toml`, calls `WorkspaceFile::load_from_file`. On error, prints to stderr and exits with code 1. On success, stores the `WorkspaceFile` in `initial_workspace`.
- `None` (no subcommand) — `initial_workspace` is `None`; existing default launch behaviour is unchanged.

**Verification:** Build passed, all 191 existing tests passed.

---

### Task 2 -- Wire workspace restore into resumed()

**Files:** `arcterm-app/src/main.rs`, `arcterm-app/src/layout.rs`

The file already contained partial workspace restore scaffolding (`count_leaves`, `spawn_default_pane` helpers, and the `if let Some(ref ws) = self.initial_workspace` branch). The implementation was complete except for environment variable injection.

Added the missing env var injection in the workspace restore loop:
1. Workspace-level `environment` map: each entry is written as `export KEY=VALUE\n` to the terminal before any command.
2. Per-pane `env` map (from `PaneMetadata`): same treatment, written after workspace-level vars.
3. Per-pane `command` replay was already present.

Added window title update: if `initial_workspace` is `Some(ws)`, the window title is set to `"Arcterm — {name}"` before `AppState` is constructed.

Added `PaneNode::remap_pane_ids(id_map: &HashMap<PaneId, PaneId>) -> PaneNode` to `layout.rs`. This method is available for future callers that need to swap placeholder IDs (from `to_pane_tree()`) for actual spawned IDs. The existing restore path uses `to_pane_tree()` directly (it allocates fresh IDs that are used as-is for the PTY map), so `remap_pane_ids` was added as infrastructure but the current restore path does not need a remap step — `to_pane_tree()` allocates PaneIds, those same IDs are inserted into the `panes`/`pty_channels` maps, and the tree is used directly.

**PaneId synchronization approach (documented):** `to_pane_tree()` allocates fresh `PaneId`s in tree traversal order and returns both the `PaneNode` tree (containing those IDs) and a `Vec<PaneMetadata>` in the same order. The restore loop zips `leaf_ids` (from `pane_tree.all_pane_ids()`) with `leaf_metadata` and calls `Terminal::new()` for each, inserting results into `panes`/`pty_channels` keyed by the same `PaneId` that is already in the tree. No post-creation fixup is required — the IDs are consistent from the moment `to_pane_tree()` returns.

**Verification:** Build passed, clippy -D warnings clean.

---

### Task 3 -- Rebind Leader+w to OpenWorkspaceSwitcher, Leader+W to CloseTab

**Files:** `arcterm-app/src/keymap.rs`, `arcterm-app/src/main.rs`

**TDD sequence (as required by tdd=true):**
1. Added `OpenWorkspaceSwitcher` variant to `KeyAction` enum (required for compilation).
2. Renamed `leader_then_w_close_tab` test to `leader_then_w_opens_workspace_switcher` and changed its assertion to `KeyAction::OpenWorkspaceSwitcher`.
3. Added new `leader_then_shift_w_closes_tab` test asserting `Leader+"W"` → `KeyAction::CloseTab`.
4. Ran tests — **both new tests failed** (confirmed red state).
5. In the `LeaderPending` match arm, changed `"w"` → `Some(KeyAction::OpenWorkspaceSwitcher)` and added `"W"` → `Some(KeyAction::CloseTab)`.
6. Ran tests — **all 29 keymap tests passed** (green).

In `main.rs`, added a `KeyAction::OpenWorkspaceSwitcher` arm to both `KeyAction` match blocks:
- In the main `window_event` handler: logs "Workspace switcher: open (stub — PLAN-3.1)".
- In the palette dispatch helper: added to the exhaustive no-op arm.

`palette.rs` was inspected — the "Close Tab" palette entry has no displayed key binding string, so no update was required there.

**Verification:** All 192 tests passed. Keymap tests: 29/29.

---

## Final State

| Metric | Value |
|--------|-------|
| Tests | 192 passed, 0 failed |
| Clippy | Clean (`-D warnings`) |
| Commits | 3 (one per task) |
| Files modified | `arcterm-app/Cargo.toml`, `arcterm-app/src/main.rs`, `arcterm-app/src/layout.rs`, `arcterm-app/src/keymap.rs` |

## Deviations from Plan

**Task 2:** The plan describes building the workspace restore path from scratch. In practice, the file already contained partial scaffolding (`count_leaves`, `spawn_default_pane`, and the conditional branch structure) that was consistent with the plan's design. The implementation completed the missing pieces (env var injection, window title) rather than rewriting what was already correct. This is not a deviation from intent — the plan's design was followed; some prior work had already been done.

**Task 2 — remap_pane_ids:** The plan asked to document the PaneId synchronization approach and choose between remap fixup or reusing IDs from `to_pane_tree()`. The implementation uses the "reuse IDs" approach (no remap needed at runtime) but still adds `remap_pane_ids` to `layout.rs` as infrastructure for future callers. The function is marked `#[allow(dead_code)]` by the linter since it is not called at runtime in this plan.

## Infrastructure Validation

No IaC files were modified. Not applicable.
