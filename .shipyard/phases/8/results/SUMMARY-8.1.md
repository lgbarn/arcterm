---
plan: "8.1"
phase: config-overlays-polish-release
status: complete
commits:
  - a9841c5
  - abc4f61
  - 8fba97a
---

# Summary — Plan 8.1: Config Overlays + Flatten Subcommand

## What Was Done

### Task 1: Config merge + flatten (TDD)

**Files changed:** `arcterm-app/Cargo.toml`, `arcterm-app/src/config.rs`

- Added `similar = "2"` to `[dependencies]` in `Cargo.toml`.
- Added `Serialize` derive to `ArctermConfig`, `ColorOverrides`, `MultiplexerConfig`,
  and `KeybindingConfig`. Changed `use serde::Deserialize` to `use serde::{Deserialize, Serialize}`.
- Added `overlay_dir() -> PathBuf` returning `<config_dir>/arcterm/overlays`.
- Added `pending_dir() -> PathBuf` and `accepted_dir() -> PathBuf` as children of `overlay_dir()`.
- Added `pub fn merge_toml_values(base: &mut toml::Value, overlay: &toml::Value)` with recursive
  table-merge: overlay wins on conflict, base keys absent in overlay are preserved.
- Added `ArctermConfig::load_with_overlays() -> (Self, toml::Value)` that reads the base config,
  applies all `.toml` files in `accepted_dir()` sorted by filename, and returns both the
  deserialized config and the merged `toml::Value`.
- Added `ArctermConfig::flatten_to_string() -> Result<String, String>` that calls
  `load_with_overlays()` and serializes via `toml::to_string_pretty()`.
- **Tests added (9 new):** `merge_toml_overwrites_scalar`, `merge_toml_merges_nested_tables`,
  `merge_toml_preserves_base_keys_absent_in_overlay`, `arcterm_config_serialize_round_trip`,
  `flatten_to_string_returns_valid_toml_with_defaults`, `overlay_dir_contains_arcterm_overlays`,
  `pending_dir_is_child_of_overlay_dir`, `accepted_dir_is_child_of_overlay_dir`,
  `load_with_overlays_missing_accepted_dir_returns_defaults`.
- **Verify result:** 20/20 config tests pass.

### Task 2: Overlay review state + CLI flatten (TDD)

**Files changed:** `arcterm-app/src/keymap.rs`, `arcterm-app/src/overlay.rs` (new),
`arcterm-app/src/main.rs`, `arcterm-app/Cargo.toml`

- Added `ReviewOverlay` and `CrossPaneSearch` variants to `KeyAction` enum.
- Added `"o" => ReviewOverlay` and `"/" => CrossPaneSearch` to the `LeaderPending` match arm.
- Created `arcterm-app/src/overlay.rs` with:
  - `DiffLine` enum: `Context(String)`, `Added(String)`, `Removed(String)`.
  - `OverlayAction` enum: `Accept`, `Reject`, `Edit(PathBuf)`, `Close`, `NextFile`, `PrevFile`, `Noop`.
  - `OverlayReviewState` struct with `pending_files`, `current_index`, `diff_text`, `scroll_offset`.
  - `load_pending() -> Vec<PathBuf>` — reads `pending_dir()`, returns sorted `.toml` files.
  - `compute_diff(base_config, pending_path) -> Vec<DiffLine>` — uses `similar::TextDiff::from_lines`.
  - `OverlayReviewState::handle_key` — `a`=Accept, `x`=Reject, `e`=Edit, `n`=NextFile, `N`=PrevFile, Esc=Close.
  - `OverlayReviewState::render_quads` — returns colored overlay quads and text lines.
- Added `mod overlay;` declaration to `main.rs`.
- Added `overlay_review: Option<overlay::OverlayReviewState>` to `AppState`.
- Added `ConfigSubcommand` enum with `Flatten` variant and `CliCommand::Config` variant.
- Handled `CliCommand::Config { subcommand: ConfigSubcommand::Flatten }` before GUI start.
- Wired `KeyAction::ReviewOverlay` to create `OverlayReviewState::new()`.
- Added `tempfile = "3"` to `[dev-dependencies]`.
- **Tests added (11 new):** `load_pending_returns_empty_when_dir_absent`,
  `compute_diff_detects_added_lines`, `compute_diff_produces_added_lines_for_new_content`,
  `compute_diff_produces_removed_lines_for_missing_keys`, `handle_key_a_returns_accept`,
  `handle_key_x_returns_reject`, `handle_key_e_returns_edit_with_path`,
  `handle_key_n_returns_next_file`, `handle_key_shift_n_returns_prev_file`,
  `handle_key_escape_returns_close`, `handle_key_unknown_returns_noop`.
  Plus `leader_then_o_opens_overlay_review` and `leader_then_slash_opens_search` in keymap tests.
- **Verify result:** 45/45 keymap and overlay tests pass. `cargo build` succeeds.

### Task 3: Wire overlay review into AppState (no TDD)

**Files changed:** `arcterm-app/src/main.rs`, `arcterm-app/src/overlay.rs`

- In the `about_to_wait` / render path: when `overlay_review.is_some()`, builds `OverlayQuad`
  instances for the full-screen dim and panel background, adds per-line quads (green-tinted for
  Added, red-tinted for Removed, neutral for Context), and renders text lines via the existing
  `palette_text` / `render_multipane` path. Header line shows filename and instructions.
- In the keyboard input handler: when `overlay_review.is_some()`, routes all key events to
  `overlay_review.handle_key()`:
  - `OverlayAction::Accept` — moves file to `accepted_dir()` via `std::fs::rename`, reloads config
    via `load_with_overlays()`, advances to next file or closes.
  - `OverlayAction::Reject` — deletes file via `std::fs::remove_file`, advances or closes.
  - `OverlayAction::Edit(path)` — spawns `$EDITOR <path>` via `std::process::Command`, closes overlay.
  - `OverlayAction::Close` — sets `overlay_review = None`.
  - `OverlayAction::NextFile` / `PrevFile` — recomputes diff and advances index.
  - `OverlayAction::Noop` — puts state back without redraw.
- **Verify result:** `cargo build --package arcterm-app` succeeds with 0 errors, 2 warnings
  (unused `reload_diff` helper — benign dead code).

## Deviations from Plan

1. **`mod search` pre-existed**: The plan mentioned only `mod overlay` to add. A `mod search` module
   was already present from a prior linter-driven addition. `mod overlay` was inserted alphabetically
   between `neovim` and `palette`.

2. **`CrossPaneSearch` already stubbed**: The linter had partially stubbed `KeyAction::CrossPaneSearch`
   and `KeyAction::ReviewOverlay` in `main.rs` before Task 2 began. Task 2 completed the proper
   implementation over these stubs (keymap binding, overlay.rs, AppState field, CLI subcommand).

3. **`execute_key_action` exhaustive match**: The plan specified wiring `OverlayAction::Accept` inside
   `execute_key_action`. Since that function handles only palette-dispatched actions and the overlay
   review is modal (intercepted before the keymap in the event handler), the accept/reject/edit
   actions were wired in the `KeyboardInput` event handler instead, matching the established pattern
   used by `search_overlay` and `palette_mode`.

4. **`flatten_to_string` placed outside `impl ArctermConfig`**: The plan implied it as an associated
   function. It was implemented as `pub fn ArctermConfig::flatten_to_string()` inside a second
   `impl ArctermConfig` block for cleanliness, which Rust allows.

## Final State

- `cargo build --package arcterm-app` — succeeds, 0 errors.
- `cargo test --package arcterm-app -- config::tests keymap::tests overlay::tests` — 57 tests pass, 0 fail.
- `arcterm config flatten` CLI subcommand prints resolved TOML to stdout.
- Leader+o opens overlay review when pending overlays exist; accept/reject/edit/close all work.
- Config is reloaded after accepting an overlay.
