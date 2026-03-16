---
phase: config-overlays-polish-release
plan: "8.1"
wave: 1
dependencies: []
must_haves:
  - Config overlay workflow (AI writes pending overlay, Leader+o shows diff, accept/reject/edit)
  - arcterm config flatten exports resolved TOML to stdout
  - Serialize derive on ArctermConfig and nested structs
files_touched:
  - arcterm-app/Cargo.toml
  - arcterm-app/src/config.rs
  - arcterm-app/src/overlay.rs
  - arcterm-app/src/keymap.rs
  - arcterm-app/src/main.rs
tdd: true
---

# Plan 8.1 -- Config Overlays + Flatten Subcommand

**Wave 1** | No dependencies | Parallel with Plan 8.2

## Goal

Implement the config overlay system end-to-end: overlay directory structure
(`~/.config/arcterm/overlays/`), `toml::Value`-level merge logic, `Serialize` on
`ArctermConfig` for TOML output, the `arcterm config flatten` CLI subcommand, and the
`Leader+o` diff-view overlay with accept/reject/edit actions. This is a vertical slice
from data model through CLI and UI.

---

<task id="1" files="arcterm-app/Cargo.toml, arcterm-app/src/config.rs" tdd="true">
  <action>
    Add `similar = "2"` to `[dependencies]` in `arcterm-app/Cargo.toml`.

    In `arcterm-app/src/config.rs`:

    1. Add `Serialize` to the derive macros on `ArctermConfig`, `ColorOverrides`,
       `MultiplexerConfig`, and `KeybindingConfig` (add `use serde::Serialize;` or change
       the existing import to `use serde::{Deserialize, Serialize};`).

    2. Add a public function `overlay_dir() -> PathBuf` that returns
       `dirs::config_dir().join("arcterm/overlays")`.

    3. Add `pending_dir() -> PathBuf` returning `overlay_dir().join("pending")` and
       `accepted_dir() -> PathBuf` returning `overlay_dir().join("accepted")`.

    4. Add `pub fn merge_toml_values(base: &mut toml::Value, overlay: &toml::Value)` that
       recursively merges: if both are `Table`, merge key-by-key (overlay wins); otherwise
       overlay replaces base entirely.

    5. Add `pub fn load_with_overlays() -> (Self, toml::Value)` that:
       a. Reads base config as `toml::Value` via `toml::from_str` (or empty table if absent)
       b. Reads all `.toml` files from `accepted_dir()` sorted by filename
       c. Calls `merge_toml_values` for each accepted overlay
       d. Deserializes the merged `Value` into `ArctermConfig`
       e. Returns both the config and the merged `Value` (for flatten output)

    6. Add `pub fn flatten_to_string() -> Result<String, String>` that calls
       `load_with_overlays()`, then serializes the `ArctermConfig` via
       `toml::to_string_pretty()`.

    7. Add unit tests:
       - `merge_toml_values` overwrites scalar, merges nested tables, preserves base keys
         absent in overlay
       - `ArctermConfig` round-trips through serialize/deserialize (serialize default,
         deserialize, compare fields)
       - `flatten_to_string` produces valid TOML containing default field values
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-app -- config::tests --no-fail-fast 2>&1 | tail -30</verify>
  <done>All config tests pass including new merge and serialize tests. `ArctermConfig` derives both `Serialize` and `Deserialize`. `merge_toml_values` correctly handles nested table merge. `flatten_to_string()` returns valid TOML that deserializes back to `ArctermConfig`.</done>
</task>

<task id="2" files="arcterm-app/src/keymap.rs, arcterm-app/src/overlay.rs, arcterm-app/src/main.rs" tdd="true">
  <action>
    1. In `arcterm-app/src/keymap.rs`:
       - Add `ReviewOverlay` and `CrossPaneSearch` variants to the `KeyAction` enum.
       - In the `LeaderPending` match arm, add `"o"` mapping to `KeyAction::ReviewOverlay`
         and `"/"` mapping to `KeyAction::CrossPaneSearch`.
       - Add tests: `leader_then_o_opens_overlay_review`, `leader_then_slash_opens_search`.

    2. Create `arcterm-app/src/overlay.rs` with `OverlayReviewState`:
       - Fields: `pending_files: Vec<PathBuf>`, `current_index: usize`,
         `diff_text: Vec<DiffLine>`, `scroll_offset: usize`.
       - `DiffLine` enum: `Context(String)`, `Added(String)`, `Removed(String)`.
       - `fn load_pending() -> Vec<PathBuf>`: reads `config::pending_dir()`, returns sorted
         `.toml` files.
       - `fn compute_diff(base_config: &ArctermConfig, pending_path: &Path) -> Vec<DiffLine>`:
         serializes base to TOML string, reads pending file, uses
         `similar::TextDiff::from_lines()` to produce diff lines tagged as
         Context/Added/Removed.
       - `fn handle_key(&mut self, key: &Key) -> OverlayAction`: `a` -> Accept (move file
         from `pending/` to `accepted/`, return `OverlayAction::Accept`), `x` -> Reject
         (delete file, return `OverlayAction::Reject`), `e` -> Edit (return
         `OverlayAction::Edit(path)`), `Escape` -> Close, `n`/`N` -> next/prev pending file.
       - `OverlayAction` enum: `Accept`, `Reject`, `Edit(PathBuf)`, `Close`, `NextFile`,
         `PrevFile`, `Noop`.
       - Unit tests: `compute_diff` produces Added lines for new keys, Removed lines for
         changed values; `handle_key` returns correct `OverlayAction` for each key; `load_pending`
         returns empty vec when directory is absent.

    3. In `arcterm-app/src/main.rs`:
       - Add `mod overlay;` declaration.
       - Add `overlay_review: Option<overlay::OverlayReviewState>` to `AppState`.
       - Add `Config` variant to `CliCommand` enum:
         ```
         Config {
             #[command(subcommand)]
             subcommand: ConfigSubcommand,
         }
         ```
       - Add `ConfigSubcommand` enum with `Flatten` variant.
       - Handle `CliCommand::Config { subcommand: ConfigSubcommand::Flatten }` before the
         GUI starts: call `config::flatten_to_string()`, print to stdout, exit.
       - Wire `KeyAction::ReviewOverlay` to create `OverlayReviewState::new()` and set
         `self.overlay_review = Some(state)`.
       - Wire `OverlayAction::Accept` to call `config::load_with_overlays()` and update
         `self.config`.
       - Wire `OverlayAction::Edit(path)` to spawn `$EDITOR <path>` via `std::process::Command`.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-app -- keymap::tests overlay::tests --no-fail-fast 2>&1 | tail -30 && cargo build --package arcterm-app 2>&1 | tail -10</verify>
  <done>Keymap tests for Leader+o and Leader+/ pass. Overlay review tests pass (diff computation, key handling, empty pending dir). `arcterm config flatten` subcommand compiles and is wired before GUI startup. `cargo build` succeeds with no errors.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs, arcterm-app/src/overlay.rs" tdd="false">
  <action>
    Wire overlay review rendering into the event loop:

    1. In the `about_to_wait` / render path of `main.rs`, when `self.overlay_review.is_some()`:
       - Build `OverlayQuad` instances for the diff view background (semi-transparent dark
         overlay covering the full window, same pattern as `workspace_switcher` rendering).
       - Build per-line quads: green-tinted background for `DiffLine::Added`, red-tinted for
         `DiffLine::Removed`, neutral for `DiffLine::Context`.
       - Build overlay text content from the diff lines.
       - Add a header line showing the pending overlay filename and instructions:
         "[a]ccept  [x]reject  [e]dit  [Esc]close  [n/N]ext/prev".
       - Pass quads and text through the existing `render_multipane` overlay parameters.

    2. In the keyboard input handler, when `self.overlay_review.is_some()`:
       - Route all key events to `overlay_review.handle_key()` instead of the normal keymap.
       - On `OverlayAction::Accept`: move the pending file to `accepted_dir()` using
         `std::fs::rename()`, reload config via `load_with_overlays()`, advance to next
         pending file or close if none remain.
       - On `OverlayAction::Reject`: delete the pending file via `std::fs::remove_file()`,
         advance to next or close.
       - On `OverlayAction::Edit(path)`: close the overlay review, spawn
         `std::process::Command::new(std::env::var("EDITOR").unwrap_or("vi".into()))` with
         the path as argument.
       - On `OverlayAction::Close`: set `self.overlay_review = None`.

    3. In `overlay.rs`, add `pub fn render_quads(&self, window_w: f32, window_h: f32) -> (Vec<OverlayQuad>, Vec<String>)` that returns the overlay quads and text lines for the current diff view, respecting `scroll_offset` for long diffs.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app 2>&1 | tail -10</verify>
  <done>`cargo build --package arcterm-app` compiles without errors. Overlay review renders diff with colored lines when Leader+o is pressed. Accept moves file to accepted/, reject deletes, edit spawns $EDITOR, Escape closes. Config is reloaded after accept.</done>
</task>
