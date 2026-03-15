---
phase: workspaces
plan: "1.1"
wave: 1
dependencies: []
must_haves:
  - WorkspaceFile struct with Serialize + Deserialize covering workspace metadata, pane tree shape, per-pane config (command, cwd, env), and window dimensions
  - WorkspacePaneNode enum (Leaf/HSplit/VSplit) as a serialization DTO decoupled from live PaneId values
  - schema_version field (u32, value 1) at the top level for forward compatibility
  - TOML round-trip test proving serialize then deserialize produces identical struct
  - TOML output is human-readable and matches the schema described in CONTEXT-5.md
files_touched:
  - arcterm-app/src/workspace.rs
  - arcterm-app/src/main.rs
tdd: true
---

# PLAN-1.1 -- Workspace TOML Data Model and Serialization

## Goal

Define the workspace file schema as Rust types with serde Serialize + Deserialize, write round-trip tests proving the TOML output is correct and human-readable, and verify that the recursive `WorkspacePaneNode` enum serializes cleanly through the `toml` crate (addressing the uncertainty flag from RESEARCH.md about inline table nesting).

## Why This Must Come First

Every other plan in Phase 5 depends on this data model: CLI subcommands parse it, session auto-save produces it, the workspace switcher reads file names from disk. Without a proven schema, downstream work builds on assumptions.

## Design Notes

The workspace data model is intentionally separate from the live runtime types (`PaneNode`, `Tab`, `TabManager`). Live types contain `PaneId(u64)` values allocated from an `AtomicU64` counter -- these are meaningless across sessions. The workspace DTO (`WorkspacePaneNode`) stores tree shape and per-leaf metadata only; fresh `PaneId::next()` values are assigned on restore.

The `toml` crate (already in Cargo.toml as `toml = "1"`) serializes Rust enums using serde's externally tagged representation by default. For a recursive enum like `WorkspacePaneNode`, this produces:

```toml
[layout]
HSplit = { ratio = 0.5, left = { Leaf = { ... } }, right = { Leaf = { ... } } }
```

This may hit TOML 1.0's inline table restriction (no newlines inside inline tables). The round-trip test in Task 1 must verify this works for up to 3-level nesting (the maximum practical depth for a 4-pane layout). If it fails, Task 1 must switch to `#[serde(tag = "type")]` or a flattened array representation and document the decision.

The target TOML shape for a workspace file:

```toml
schema_version = 1

[workspace]
name = "my-project"
directory = "/Users/dev/projects/my-project"

[window]
width = 1920
height = 1080

[layout]
type = "hsplit"
ratio = 0.6

[layout.left]
type = "leaf"
command = "nvim ."
directory = "/Users/dev/projects/my-project"

[layout.right]
type = "vsplit"
ratio = 0.5

[layout.right.top]
type = "leaf"
command = "cargo watch -x test"

[layout.right.bottom]
type = "leaf"

[environment]
KUBECONFIG = "/Users/dev/.kube/prod-config"
```

## Tasks

<task id="1" files="arcterm-app/src/workspace.rs" tdd="true">
  <action>Create `arcterm-app/src/workspace.rs` with the workspace data model and round-trip serialization tests.

1. Define `WorkspaceFile` struct:
   - `schema_version: u32` (default 1)
   - `workspace: WorkspaceMeta` (name, directory)
   - `window: Option<WindowState>` (width, height in physical pixels)
   - `layout: WorkspacePaneNode` (the pane tree)
   - `environment: HashMap<String, String>` (workspace-level env vars)
   All fields derive `Serialize, Deserialize, Debug, Clone, PartialEq`.

2. Define `WorkspaceMeta` struct:
   - `name: String`
   - `directory: Option<String>` (workspace root directory; ~ expanded on load)

3. Define `WindowState` struct:
   - `width: u32`
   - `height: u32`

4. Define `WorkspacePaneNode` enum using `#[serde(tag = "type", rename_all = "lowercase")]`:
   - `Leaf { command: Option<String>, directory: Option<String>, env: Option<HashMap<String, String>> }`
   - `HSplit { ratio: f32, left: Box<WorkspacePaneNode>, right: Box<WorkspacePaneNode> }`
   - `VSplit { ratio: f32, top: Box<WorkspacePaneNode>, bottom: Box<WorkspacePaneNode> }`

5. Implement `Default` for `WorkspaceFile` (single leaf, no command, schema_version = 1).

6. Add a `workspaces_dir() -> PathBuf` function returning `dirs::config_dir().join("arcterm").join("workspaces")`.

7. Add a `save_to_file(&self, path: &Path) -> io::Result<()>` method on `WorkspaceFile` that serializes to TOML, writes to a `.tmp` sibling, and atomically renames (same-dir rename for POSIX atomicity).

8. Add a `load_from_file(path: &Path) -> Result<Self, WorkspaceError>` associated function that reads TOML, validates `schema_version == 1` (or returns an error with a descriptive message), and returns the parsed struct.

9. Define `WorkspaceError` enum: `IoError(io::Error)`, `TomlParseError(String)`, `UnsupportedVersion(u32)`.

Write tests FIRST (TDD):
- `round_trip_single_leaf`: create a single-leaf WorkspaceFile, serialize to TOML string, deserialize, assert equality.
- `round_trip_4_pane_layout`: create HSplit(Leaf, VSplit(Leaf, Leaf)) with commands and directories, serialize, deserialize, assert equality. This tests 3-level nesting.
- `round_trip_with_environment`: workspace with 3 env vars, verify they survive round-trip.
- `toml_output_is_human_readable`: serialize a 2-pane layout, assert the TOML string contains `[workspace]`, `[layout]`, and `command = ` as plain-text keys (not binary or encoded).
- `schema_version_mismatch_returns_error`: manually write TOML with `schema_version = 99`, call `load_from_file`, assert `UnsupportedVersion(99)`.
- `default_workspace_file_round_trips`: default() -> serialize -> deserialize -> assert eq.
- `workspaces_dir_contains_arcterm`: assert `workspaces_dir()` path string contains "arcterm" and ends with "workspaces".</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app -- workspace --nocapture</verify>
  <done>All workspace round-trip tests pass. The TOML output for a 4-pane layout is human-readable with clear `[layout]` sections. Schema version validation rejects unknown versions. `workspaces_dir()` returns the correct path.</done>
</task>

<task id="2" files="arcterm-app/src/workspace.rs" tdd="true">
  <action>Add conversion functions between workspace DTOs and live runtime types.

1. Add `from_pane_tree(tree: &PaneNode, pane_metadata: &HashMap<PaneId, PaneMetadata>) -> WorkspacePaneNode` associated function that walks the live `PaneNode` tree and produces a `WorkspacePaneNode`, looking up per-pane metadata (command, cwd, env) from the provided map.

2. Define `PaneMetadata` struct in `workspace.rs`:
   - `command: Option<String>` (the shell command that was used to start this pane, if any)
   - `directory: Option<String>` (current working directory, captured at save time)
   - `env: Option<HashMap<String, String>>` (per-pane env overrides)

3. Add `to_pane_tree(&self) -> (PaneNode, Vec<PaneMetadata>)` method on `WorkspacePaneNode` that produces a live `PaneNode` tree with freshly allocated `PaneId::next()` values and returns the per-leaf metadata in tree-traversal order (left-to-right, top-to-bottom).

4. Add `capture_session(tab_manager: &TabManager, tab_layouts: &[PaneNode], pane_metadata: &HashMap<PaneId, PaneMetadata>, name: &str, window_size: Option<(u32, u32)>) -> WorkspaceFile` function that captures the full session state into a `WorkspaceFile`. Uses the active tab's layout tree for serialization (multi-tab workspace support deferred).

Write tests FIRST:
- `from_pane_tree_single_leaf`: create a Leaf PaneNode, convert, assert WorkspacePaneNode is Leaf.
- `from_pane_tree_nested`: create HSplit(Leaf, VSplit(Leaf, Leaf)) with metadata, convert, verify tree shape and metadata propagation.
- `to_pane_tree_assigns_fresh_ids`: convert a WorkspacePaneNode to PaneNode, verify all PaneIds are unique and nonzero.
- `to_pane_tree_preserves_ratios`: HSplit with ratio 0.7, convert, verify PaneNode HSplit has ratio 0.7.
- `round_trip_pane_tree`: PaneNode -> WorkspacePaneNode -> PaneNode, verify tree shapes match (ratios and structure, not PaneId values).
- `capture_session_produces_valid_workspace_file`: build a TabManager + layouts + metadata, call capture_session, verify resulting WorkspaceFile has correct name, layout shape, and metadata.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app -- workspace --nocapture</verify>
  <done>All conversion tests pass. `from_pane_tree` correctly maps live trees to DTOs. `to_pane_tree` assigns fresh PaneIds and preserves ratios. `capture_session` produces a complete WorkspaceFile.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs" tdd="false">
  <action>Register the `workspace` module in `arcterm-app/src/main.rs`:

1. Add `mod workspace;` to the module declarations at the top of `main.rs` (after `mod terminal;`).

2. Run the full arcterm-app test suite to verify zero regressions in existing code.

3. Run clippy on arcterm-app to verify no new warnings from the workspace module.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app -- --nocapture 2>&1 | tail -20 && cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -10</verify>
  <done>All existing tests pass (zero regressions). Clippy clean. `workspace` module is registered and its types are available to the rest of arcterm-app.</done>
</task>
