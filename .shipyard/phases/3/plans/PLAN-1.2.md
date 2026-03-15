---
phase: multiplexer
plan: "1.2"
wave: 1
dependencies: []
must_haves:
  - Tab struct grouping a PaneNode tree, focused PaneId, and pane storage
  - TabManager owning Vec<Tab> with active_tab index
  - Tab add, close, and switch operations
  - Leader key configuration in ArctermConfig
files_touched:
  - arcterm-app/src/tab.rs
  - arcterm-app/src/config.rs
  - arcterm-app/src/main.rs (mod declaration only)
tdd: true
---

# PLAN-1.2 -- Tab Model and Leader Key Configuration

## Goal

Define the Tab data structure that groups a pane tree with its pane storage, and add leader key configuration to `ArctermConfig`. This plan runs in parallel with PLAN-1.1 (no shared files except the mod declaration in main.rs, which both add a single line to -- the builder should coordinate this trivially).

## Why Wave 1

The Tab struct is a container for `PaneNode` + `HashMap<PaneId, Terminal>` + `HashMap<PaneId, Receiver>`. It is defined here as a struct with generic type placeholders (or concrete types from existing crates) so that Wave 2 plans can populate it with real Terminals. The config additions are needed by the keymap module in Wave 2.

## Tasks

<task id="1" files="arcterm-app/src/tab.rs" tdd="true">
  <action>Create `arcterm-app/src/tab.rs` with:

1. `Tab` struct containing:
   - `label: String` -- user-visible tab name (default: "Tab N")
   - `layout: PaneNode` -- the binary split tree (import from `crate::layout`)
   - `focus: PaneId` -- the currently focused pane within this tab
   - `zoomed: Option<PaneId>` -- if Some, this pane is in fullscreen zoom mode

   Note: `Tab` does NOT own Terminals or PTY receivers. Those are stored in flat `HashMap`s on `AppState` so that background tabs' PTY channels can still be polled. The `Tab` struct is a lightweight layout + focus descriptor.

2. `TabManager` struct containing:
   - `tabs: Vec<Tab>`
   - `active: usize` -- index of the active tab

3. `TabManager` methods:
   - `new(initial_pane_id: PaneId) -> Self` -- creates a TabManager with one tab containing a single-leaf PaneNode.
   - `active_tab(&self) -> &Tab` -- returns reference to the active tab.
   - `active_tab_mut(&mut self) -> &mut Tab` -- mutable reference.
   - `add_tab(pane_id: PaneId) -> usize` -- creates a new tab with a single-leaf layout, appends it, returns its index.
   - `close_tab(index: usize) -> Vec<PaneId>` -- removes the tab at `index`, returns all PaneIds that were in it (so the caller can clean up Terminals/PTY channels). If only one tab remains, returns empty vec and does nothing. Adjusts `active` if needed.
   - `switch_to(index: usize)` -- sets `active` to `index` (clamped to valid range).
   - `tab_count(&self) -> usize`
   - `all_pane_ids(&self) -> Vec<PaneId>` -- collects PaneIds across all tabs (for polling PTY channels of background tabs).

Write tests covering:
- New TabManager has exactly 1 tab
- `active_tab` returns the correct tab
- `add_tab` increases tab count and returns correct index
- `close_tab` removes the tab and returns its pane IDs
- `close_tab` on the last tab is a no-op
- `close_tab` adjusts `active` when the active tab is closed
- `switch_to` clamps to valid range
- `all_pane_ids` collects IDs from all tabs</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app tab -- --nocapture</verify>
  <done>All TabManager tests pass. Tab creation, closing, switching, and pane ID collection work correctly.</done>
</task>

<task id="2" files="arcterm-app/src/config.rs" tdd="true">
  <action>Add multiplexer-related configuration fields to `ArctermConfig`:

1. Add a `[multiplexer]` section as a new struct `MultiplexerConfig`:
   ```rust
   #[derive(Debug, Clone, Deserialize)]
   #[serde(default)]
   pub struct MultiplexerConfig {
       /// Leader key chord (default: "Ctrl+a").
       pub leader_key: String,
       /// Leader key timeout in milliseconds (default: 500).
       pub leader_timeout_ms: u64,
       /// Whether to show the tab bar (default: true).
       pub show_tab_bar: bool,
       /// Border width in logical pixels (default: 1.0).
       pub border_width: f32,
       /// Whether Ctrl+h/j/k/l pane navigation is enabled (default: true).
       pub pane_navigation: bool,
   }
   ```

2. Add `Default` impl for `MultiplexerConfig` with the values shown above.

3. Add `pub multiplexer: MultiplexerConfig` field to `ArctermConfig` with `#[serde(default)]`.

4. Add a test that parses a TOML string with `[multiplexer]` section and verifies all fields override correctly. Add a test that omitting the section uses defaults.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app config -- --nocapture</verify>
  <done>All config tests pass including new multiplexer config tests. `MultiplexerConfig` fields deserialize correctly from TOML and default when omitted.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs" tdd="false">
  <action>Add module declarations to `arcterm-app/src/main.rs`:

1. Add `mod layout;` after the existing `mod terminal;` line.
2. Add `mod tab;` after `mod layout;`.

These are declaration-only changes. No other modifications to main.rs. The actual AppState restructuring happens in Wave 2 plans.

Verify the crate compiles cleanly.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app 2>&1 | tail -5 && cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -5</verify>
  <done>`cargo build` and `cargo clippy` succeed with no errors or warnings. The `layout` and `tab` modules are recognized by the compiler.</done>
</task>
