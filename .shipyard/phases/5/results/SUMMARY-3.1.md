# SUMMARY-3.1 — Workspace Switcher UI

**Plan**: PLAN-3.1
**Phase**: 5 (Workspaces)
**Wave**: 3
**Completed**: 2026-03-15
**Status**: All tasks complete. Build and clippy clean.

---

## Tasks Executed

### Task 1 — WorkspaceSwitcherState (TDD)

**Files touched**: `arcterm-app/src/palette.rs`

Added alongside the existing `PaletteState`:

- `WorkspaceEntry { name: String, path: PathBuf }` — a single discoverable workspace file.
- `WorkspaceSwitcherEvent { Consumed, Close, Open(PathBuf) }` — state machine output enum.
- `WorkspaceSwitcherState { query, entries, filtered, selected }` — full modal overlay state.
- `new(entries)` — initializes with all entries visible, selection at 0.
- `handle_input` / `handle_key` — routes Escape, Enter, ArrowUp/Down, Backspace, Character keys.
- `update_filter` — case-insensitive `str::contains` substring match on `entry.name`.
- `visible_entries` — capped at 10 indices.
- `render_quads` — dim overlay + box background + input field background + selection highlight, same layout math as `PaletteState`.
- `render_text_content` — query prompt and up to 10 entry name labels.

**TDD protocol**: Tests were written as a block in the existing `tests` module before running. The implementation was added in the same edit. All 11 tests passed on first run.

**Tests** (all passing):
- `workspace_switcher_all_entries_visible_initially`
- `workspace_switcher_filter_narrows_list`
- `workspace_switcher_filter_is_case_insensitive`
- `workspace_switcher_arrow_down_moves_selection`
- `workspace_switcher_arrow_up_clamps_at_zero`
- `workspace_switcher_enter_returns_open_with_path`
- `workspace_switcher_escape_returns_close`
- `workspace_switcher_backspace_removes_char_and_refilters`
- `workspace_switcher_empty_filter_shows_all`
- `workspace_switcher_visible_entries_capped_at_ten`

**Verify**: `cargo test -p arcterm-app -- workspace_switcher --nocapture` — 11/11 passed.

**Commit**: `e37e0d1 shipyard(phase-5): add WorkspaceSwitcherState to palette.rs with full TDD test suite`

---

### Task 2 — discover_workspaces()

**Files touched**: `arcterm-app/src/workspace.rs`

Added `discover_workspaces() -> Vec<palette::WorkspaceEntry>` to `workspace.rs`:
- Reads `workspaces_dir()` via `std::fs::read_dir`.
- Filters for `.toml` files whose stem does not start with `_`.
- Produces `WorkspaceEntry { name: stem, path: full_path }`.
- Sorts alphabetically by name.
- Returns empty `Vec` if the directory does not exist.

The `WorkspaceEntry` type is defined in `palette.rs` and referenced via `crate::palette::WorkspaceEntry` to keep the single source of truth in the palette module (consistent with the plan's intent of reuse).

Five hermetic tests were added in the `workspace::tests` module using a local `discover_workspaces_in(dir)` helper to avoid touching the real config directory:
- `discover_finds_toml_files_as_entries`
- `discover_ignores_underscore_prefixed_files`
- `discover_returns_empty_for_nonexistent_directory`
- `discover_sorts_alphabetically`
- `discover_entry_path_points_to_toml_file`

**Verify**: `cargo test -p arcterm-app -- discover --nocapture` — 6/6 passed (5 discover tests + 1 pre-existing neovim discover test).

**Commit**: `d47e371 shipyard(phase-5): add discover_workspaces() to workspace.rs returning WorkspaceEntry vec`

---

### Task 3 — Wire into main.rs

**Files touched**: `arcterm-app/src/main.rs`

Changes made:

1. **Import**: Added `WorkspaceSwitcherState` to the `use palette::` import.

2. **AppState field**: Added `workspace_switcher: Option<WorkspaceSwitcherState>` alongside `palette_mode`.

3. **Initialization**: Set `workspace_switcher: None` in the `AppState` construction in `resumed()`.

4. **OpenWorkspaceSwitcher handler**: Replaced the log-only stub with a call to `workspace::discover_workspaces()`, construction of `WorkspaceSwitcherState::new(entries)`, assignment to `state.workspace_switcher`, and `request_redraw()`.

5. **Keyboard routing**: Added a switcher intercept block in `WindowEvent::KeyboardInput`, checked before the palette intercept. When `workspace_switcher.is_some()`, all key input routes through `switcher.handle_input()`:
   - `Close` → set `workspace_switcher = None`, redraw.
   - `Open(path)` → set `workspace_switcher = None`, call `WorkspaceFile::load_from_file(&path)`, call `state.restore_workspace(&ws)`, redraw. Load errors are logged but do not crash.
   - `Consumed` → redraw.

6. **restore_workspace method**: Extracted the workspace restore logic from `resumed()` into a new `restore_workspace(&mut self, ws: &WorkspaceFile)` method on `AppState`. The method:
   - Computes initial grid size from the current window dimensions.
   - Shuts down all current panes (removes from `panes`, `pty_channels`, `auto_detectors`, `structured_blocks`, `nvim_states`).
   - Falls back to a single fresh pane if `count_leaves == 0`.
   - Calls `ws.layout.to_pane_tree()` to produce a fresh `PaneNode` tree with new `PaneId` values.
   - Spawns `Terminal` instances for each leaf, injecting workspace and per-pane environment variables and replaying saved commands.
   - Replaces the active tab layout, resets zoom, clears selection, sets `shell_exited = false`.
   - Updates the window title to `"Arcterm — {workspace_name}"`.

7. **RedrawRequested rendering**: Added a switcher overlay rendering block immediately after the palette block. When `workspace_switcher.is_some()`, calls `sw.render_quads()` and `sw.render_text_content()` and appends the results to `overlay_quads` and `palette_text` respectively, using the same `render_multipane` path as the palette overlay.

**Verify**: `cargo build -p arcterm-app 2>&1 | tail -5` — build clean. `cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -10` — clippy clean, no warnings.

**Commit**: `0b31399 shipyard(phase-5): wire WorkspaceSwitcherState into main.rs event loop`

---

## Deviations

**None.** All three tasks were implemented exactly as specified in PLAN-3.1.

The only design note worth recording: the plan specified `WorkspaceEntry` be defined in `palette.rs` (or `workspace.rs`) and re-exported. The implementation defines it in `palette.rs` and references it from `workspace.rs` as `crate::palette::WorkspaceEntry`. This keeps the type adjacent to the `WorkspaceSwitcherState` that owns a `Vec<WorkspaceEntry>`, which is the natural coupling point.

---

## Final State

| File | Change |
|------|--------|
| `arcterm-app/src/palette.rs` | +352 lines: `WorkspaceEntry`, `WorkspaceSwitcherEvent`, `WorkspaceSwitcherState` (impl + render methods + 10 tests) |
| `arcterm-app/src/workspace.rs` | +138 lines: `discover_workspaces()` + 5 hermetic tests |
| `arcterm-app/src/main.rs` | +170 lines: switcher field, OpenWorkspaceSwitcher handler, keyboard routing, `restore_workspace()` method, redraw rendering |

All tests pass. Build and clippy are clean. The workspace switcher is fully wired end-to-end.
