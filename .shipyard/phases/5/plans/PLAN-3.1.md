---
phase: workspaces
plan: "3.1"
wave: 3
dependencies: ["2.1", "2.2"]
must_haves:
  - Leader+w opens fuzzy workspace switcher overlay
  - Workspace switcher lists .toml files from ~/.config/arcterm/workspaces/ (excluding _-prefixed)
  - Substring filtering on workspace names as user types
  - Enter on selected workspace triggers close-current-and-open-new workflow
  - Escape closes the switcher
  - Reuses PaletteState UI pattern (quads + text rendering)
files_touched:
  - arcterm-app/src/palette.rs
  - arcterm-app/src/main.rs
  - arcterm-app/src/workspace.rs
tdd: true
---

# PLAN-3.1 -- Workspace Switcher UI

## Goal

Implement the `Leader+w` workspace switcher as a modal overlay that lists available workspace files, supports substring filtering, and opens the selected workspace on Enter. The switcher reuses the `PaletteState` UI rendering pattern (dim overlay, centered box, input field, highlighted selection row) but uses a separate `WorkspaceSwitcherState` struct to avoid coupling with the command palette.

## Why Wave 3

Depends on PLAN-2.1 (the `KeyAction::OpenWorkspaceSwitcher` variant and workspace loading logic) and PLAN-2.2 (the `list_workspaces` filtering logic that skips underscore-prefixed files). The switcher is the final user-facing integration piece.

## Design Notes

**State machine**: `WorkspaceSwitcherState` mirrors `PaletteState` exactly:
- `query: String` -- filter text
- `entries: Vec<WorkspaceEntry>` -- all workspace files (name + path)
- `filtered: Vec<usize>` -- indices into `entries` matching query
- `selected: usize` -- highlighted row in `filtered`
- `handle_key()` returns `WorkspaceSwitcherEvent` (Consumed, Close, Open(PathBuf))

**Rendering**: The same `PaletteQuad` and `PaletteText` types are reused. The switcher produces quads/texts via `render_quads()` and `render_text_content()` methods identical in structure to `PaletteState`'s.

**Integration in main.rs**: `AppState` gains `workspace_switcher: Option<WorkspaceSwitcherState>`. When `KeyAction::OpenWorkspaceSwitcher` is received, populate the switcher by scanning the workspaces directory and set the field to `Some`. When `WorkspaceSwitcherEvent::Open(path)` is received, close all current panes, load the workspace file, and restore -- reusing the same restore logic from PLAN-2.1 Task 2.

**Performance**: The switcher scans `read_dir` on the workspaces directory when opened. For a typical user with 5-30 files, this is sub-millisecond. The scan is not cached; each open rescans.

## Tasks

<task id="1" files="arcterm-app/src/palette.rs" tdd="true">
  <action>Implement `WorkspaceSwitcherState` in `palette.rs` alongside the existing `PaletteState`.

1. Define `WorkspaceEntry` struct: `name: String`, `path: PathBuf`. Derive `Debug, Clone`.

2. Define `WorkspaceSwitcherEvent` enum: `Consumed`, `Close`, `Open(PathBuf)`.

3. Define `WorkspaceSwitcherState` struct:
   - `query: String`
   - `entries: Vec<WorkspaceEntry>`
   - `filtered: Vec<usize>`
   - `selected: usize`

4. Implement `WorkspaceSwitcherState::new(entries: Vec<WorkspaceEntry>) -> Self` that sets `filtered` to all indices and `selected` to 0.

5. Implement `handle_key(&mut self, logical_key: &Key, text: Option<&str>, modifiers: ModifiersState) -> WorkspaceSwitcherEvent`:
   - Escape -> Close
   - Enter -> if a selection exists, `Open(self.entries[self.filtered[self.selected]].path.clone())`; else Close
   - ArrowUp/Down -> move selection
   - Backspace -> pop query char, refilter
   - Character -> append to query, refilter

6. Implement `update_filter(&mut self)` using case-insensitive `str::contains` on `entry.name` (same algorithm as `PaletteState::update_filter`).

7. Implement `render_quads()` and `render_text_content()` methods with the same signature and layout math as `PaletteState`, but using workspace entry names instead of command labels. The input prompt shows `> ` prefix. The title text above the list shows "Switch Workspace".

8. Implement `visible_entries(&self) -> &[usize]` capped at 10.

Write tests FIRST:
- `all_entries_visible_initially`: create switcher with 5 entries, assert filtered.len() == 5.
- `filter_narrows_list`: type "proj", assert only entries containing "proj" remain.
- `filter_is_case_insensitive`: type "PROJ", assert matches entries with "proj".
- `arrow_down_moves_selection`: press ArrowDown, assert selected == 1.
- `arrow_up_clamps_at_zero`: press ArrowUp at selected=0, assert still 0.
- `enter_returns_open_with_path`: select entry, press Enter, assert Open(path).
- `escape_returns_close`: press Escape, assert Close.
- `backspace_removes_char_and_refilters`: type "ab", backspace, assert query == "a".
- `empty_filter_shows_all`: clear query, assert filtered matches all entries.
- `visible_entries_capped_at_ten`: create switcher with 15 entries, assert visible_entries().len() == 10.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app -- workspace_switcher --nocapture</verify>
  <done>All workspace switcher state machine tests pass. Filtering, selection, Enter/Escape, and rendering methods work correctly.</done>
</task>

<task id="2" files="arcterm-app/src/workspace.rs" tdd="false">
  <action>Add `discover_workspaces() -> Vec<WorkspaceEntry>` function to `workspace.rs`.

1. Implement `discover_workspaces()` that:
   - Reads `workspaces_dir()` with `std::fs::read_dir`.
   - Filters for files ending in `.toml` whose name does NOT start with `_`.
   - For each matching file, creates a `WorkspaceEntry { name: stem, path: full_path }`.
   - Sorts entries alphabetically by name.
   - Returns empty Vec if the directory does not exist.

2. Re-export `WorkspaceEntry` from `workspace.rs` (or import from palette.rs if defined there -- ensure the type is accessible to both modules).</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app -- discover --nocapture</verify>
  <done>`discover_workspaces()` correctly lists workspace files, excludes underscore-prefixed files, and sorts alphabetically. Returns empty vec for nonexistent directory.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs" tdd="false">
  <action>Wire the workspace switcher into the main event loop.

1. Add `workspace_switcher: Option<WorkspaceSwitcherState>` field to `AppState`.

2. In the `KeyAction::OpenWorkspaceSwitcher` handler:
   - Call `workspace::discover_workspaces()` to get entries.
   - Create `WorkspaceSwitcherState::new(entries)`.
   - Set `state.workspace_switcher = Some(switcher)`.
   - Request redraw.

3. In the keyboard event handler, when `state.workspace_switcher.is_some()`:
   - Route the key event to `switcher.handle_key()` instead of the normal keymap.
   - On `WorkspaceSwitcherEvent::Close`: set `state.workspace_switcher = None`, request redraw.
   - On `WorkspaceSwitcherEvent::Open(path)`: set `state.workspace_switcher = None`, load the workspace file, close all current panes (shutdown PTY sessions, remove from maps), restore from the loaded workspace (reuse the same restore logic from PLAN-2.1 Task 2 -- extract it into a `restore_workspace(&mut self, ws: &WorkspaceFile)` method on `AppState`), request redraw.
   - On `WorkspaceSwitcherEvent::Consumed`: request redraw.

4. In the `RedrawRequested` handler, when `state.workspace_switcher.is_some()`:
   - After rendering panes, render the switcher overlay by calling `switcher.render_quads()` and `switcher.render_text_content()`.
   - Pass the quads and texts to the renderer using the same overlay rendering path used by the command palette.

5. Verify that the workspace switcher can be opened with `Leader+w`, workspace names are displayed, typing filters the list, Enter opens a workspace (closing current panes and restoring the new layout), and Escape dismisses the switcher.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app 2>&1 | tail -5 && cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -10</verify>
  <done>Build succeeds. Leader+w opens the workspace switcher overlay. Typing filters workspace names. Enter loads the selected workspace. Escape dismisses. Clippy clean. The workspace restore workflow is complete end-to-end.</done>
</task>
