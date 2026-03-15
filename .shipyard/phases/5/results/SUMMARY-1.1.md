---
plan: "1.1"
phase: workspaces
status: complete
commits:
  - cfeb0df  shipyard(phase-5): add WorkspaceFile data model with TOML round-trip serialization
  - 1e4128b  shipyard(phase-5): register workspace module and fix pre-existing terminal.rs compile error
---

# SUMMARY-1.1 — Workspace TOML Data Model and Serialization

## What Was Done

### Task 1 — Workspace DTOs with serde (TDD)

Created `arcterm-app/src/workspace.rs` with the full workspace data model:

- `WorkspaceFile` struct: `schema_version: u32`, `workspace: WorkspaceMeta`, `window: Option<WindowState>`, `layout: WorkspacePaneNode`, `environment: HashMap<String, String>`. Derives `Serialize, Deserialize, Debug, Clone, PartialEq`.
- `WorkspaceMeta` struct: `name: String`, `directory: Option<String>`.
- `WindowState` struct: `width: u32`, `height: u32`.
- `WorkspacePaneNode` enum with `#[serde(tag = "type", rename_all = "lowercase")]`: `Leaf { command, directory, env }`, `HSplit { ratio, left, right }`, `VSplit { ratio, top, bottom }`.
- `Default` for `WorkspaceFile`: single `Leaf` with no command, `schema_version = 1`.
- `workspaces_dir() -> PathBuf`: returns `<config_dir>/arcterm/workspaces/`.
- `save_to_file(&self, path: &Path) -> io::Result<()>`: atomic write via `.toml.tmp` sibling + POSIX rename.
- `load_from_file(path: &Path) -> Result<Self, WorkspaceError>`: reads TOML, validates `schema_version == 1`.
- `WorkspaceError` enum: `IoError(io::Error)`, `TomlParseError(String)`, `UnsupportedVersion(u32)`.

All 7 round-trip tests written and pass:
- `round_trip_single_leaf`
- `round_trip_4_pane_layout` (HSplit + VSplit, 3-level nesting)
- `round_trip_with_environment` (3 env vars)
- `toml_output_is_human_readable` (asserts `[workspace]`, `[layout]`, `command = ` as plain text)
- `schema_version_mismatch_returns_error` (schema_version = 99 → UnsupportedVersion(99))
- `default_workspace_file_round_trips`
- `workspaces_dir_contains_arcterm`

#### TOML nesting decision

The plan flagged uncertainty about TOML 1.0's inline table restriction for recursive enums. Using `#[serde(tag = "type", rename_all = "lowercase")]` (internally tagged) rather than serde's default externally-tagged representation resolves this. The `toml` crate serializes the internally-tagged form as standard TOML sections (`[layout]`, `[layout.left]`, `[layout.right]`, `[layout.right.top]`, etc.), which is fully compliant with TOML 1.0 and produces the human-readable shape specified in the plan.

### Task 2 — Conversion Functions (TDD)

Added to `arcterm-app/src/workspace.rs`:

- `PaneMetadata` struct: `command: Option<String>`, `directory: Option<String>`, `env: Option<HashMap<String, String>>`.
- `WorkspacePaneNode::from_pane_tree(tree: &PaneNode, pane_metadata: &HashMap<PaneId, PaneMetadata>) -> WorkspacePaneNode`: walks the live tree, looks up per-leaf metadata.
- `WorkspacePaneNode::to_pane_tree(&self) -> (PaneNode, Vec<PaneMetadata>)`: allocates fresh `PaneId::next()` values, returns metadata in left-to-right / top-to-bottom traversal order.
- `capture_session(tab_manager, pane_metadata, name, window_size) -> WorkspaceFile`: snapshots the active tab's layout.

All 6 conversion tests written and pass:
- `from_pane_tree_single_leaf`
- `from_pane_tree_nested`
- `to_pane_tree_assigns_fresh_ids`
- `to_pane_tree_preserves_ratios`
- `round_trip_pane_tree`
- `capture_session_produces_valid_workspace_file`

Note: Tasks 1 and 2 both touch `arcterm-app/src/workspace.rs`. The file was written as a single atomic unit; per commit convention, both tasks share commit `cfeb0df`. The distinction between tasks is preserved in test names and code organisation within the file.

### Task 3 — Module Registration

- Added `mod workspace;` to `arcterm-app/src/main.rs` after `mod terminal;`.
- Added `#![allow(dead_code)]` to `workspace.rs` with a documentation comment explaining these are public APIs consumed by downstream CLI subcommands (Phase 5 Wave 2+). This follows the existing codebase convention (`#[allow(dead_code)]` used in `layout.rs` and `tab.rs` for API functions not yet wired into `main.rs`).
- Full suite: 191 tests pass, 0 failures.
- `cargo clippy -p arcterm-app -- -D warnings`: clean.

## Deviations

### Pre-existing compilation error in `terminal.rs`

`arcterm-pty/src/session.rs` had a pre-existing change (from another work session) that added a `cwd: Option<&Path>` parameter to `PtySession::new`. The call site in `arcterm-app/src/terminal.rs` was not updated, causing `error[E0061]: this function takes 3 arguments but 2 arguments were supplied`. This blocked compilation entirely and had to be fixed before any tests could run.

Fix: passed `None` as the `cwd` argument in `Terminal::new`, preserving existing behaviour (shell inherits current process working directory). This is the minimal correct fix — `None` is semantically identical to the previous 2-argument behaviour.

This fix is included in commit `1e4128b` with full documentation.

## Final State

| File | Lines | Status |
|------|-------|--------|
| `arcterm-app/src/workspace.rs` | 813 | Created |
| `arcterm-app/src/main.rs` | +1 line | `mod workspace;` added |
| `arcterm-app/src/terminal.rs` | 1 line changed | Pre-existing bug fixed |

Tests: 191 pass (176 pre-existing + 15 new workspace tests). Clippy: clean.
