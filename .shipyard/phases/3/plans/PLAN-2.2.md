---
phase: multiplexer
plan: "2.2"
wave: 2
dependencies: ["1.1", "1.2"]
must_haves:
  - KeymapState enum with Normal and LeaderPending variants
  - Leader key detection with configurable timeout
  - Leader+n horizontal split, Leader+v vertical split
  - Leader+q close pane, Leader+z zoom toggle
  - Leader+arrow pane resize
  - Leader+t new tab, Leader+1..9 switch tabs
  - Ctrl+h/j/k/l pane navigation (always active)
  - Input routing to focused pane only
files_touched:
  - arcterm-app/src/keymap.rs
  - arcterm-app/src/main.rs (mod declaration only)
tdd: true
---

# PLAN-2.2 -- Leader Key State Machine and Pane Navigation Keybindings

## Goal

Implement the two-key leader chord system and all multiplexer keybindings as a pure state machine module. This module takes key events and produces `KeyAction` commands that the app layer dispatches. It has no dependencies on the renderer, PTY, or windowing system, making it fully testable.

## Why Wave 2

Depends on PLAN-1.2 for `MultiplexerConfig` (leader key string, timeout). The keymap module produces actions that reference `Direction` from PLAN-1.1. Both Wave 1 plans must be complete before this can compile.

## Tasks

<task id="1" files="arcterm-app/src/keymap.rs" tdd="true">
  <action>Create `arcterm-app/src/keymap.rs` with the core state machine:

1. `KeymapState` enum:
   ```rust
   pub enum KeymapState {
       Normal,
       LeaderPending { entered_at: Instant },
   }
   ```

2. `KeyAction` enum describing all multiplexer commands:
   ```rust
   pub enum KeyAction {
       /// Forward these bytes to the focused pane's PTY.
       Forward(Vec<u8>),
       /// Navigate to the pane in this direction.
       NavigatePane(Direction),
       /// Split the focused pane.
       Split(Axis),
       /// Close the focused pane.
       ClosePane,
       /// Toggle zoom on the focused pane.
       ToggleZoom,
       /// Resize the focused pane's parent split.
       ResizePane(Direction),
       /// Create a new tab.
       NewTab,
       /// Switch to tab by index (0-based).
       SwitchTab(usize),
       /// Close the current tab.
       CloseTab,
       /// Open the command palette.
       OpenPalette,
       /// No action (key consumed silently, e.g., leader key press).
       Consumed,
   }
   ```

3. `KeymapHandler` struct:
   ```rust
   pub struct KeymapHandler {
       state: KeymapState,
       leader_timeout_ms: u64,
   }
   ```

4. `KeymapHandler::new(leader_timeout_ms: u64) -> Self`.

5. `KeymapHandler::handle_key(&mut self, event: &KeyEvent, modifiers: ModifiersState, app_cursor_keys: bool) -> KeyAction`:
   - Check if `state` is `LeaderPending` and `entered_at.elapsed() > Duration::from_millis(self.leader_timeout_ms)` -> timeout: reset to `Normal`, produce `Forward(vec![0x01])` (raw Ctrl+a), then re-process the current key in Normal state.
   - In `Normal` state:
     - Ctrl+a (or configured leader): transition to `LeaderPending`, return `Consumed`.
     - Ctrl+h: return `NavigatePane(Left)`.
     - Ctrl+j: return `NavigatePane(Down)`.
     - Ctrl+k: return `NavigatePane(Up)`.
     - Ctrl+l: return `NavigatePane(Right)`.
     - Ctrl+Space: return `OpenPalette`.
     - All other keys: call `input::translate_key_event()` and return `Forward(bytes)` or `Consumed` if None.
   - In `LeaderPending` state (not timed out):
     - 'n' -> `Split(Horizontal)`, reset to Normal.
     - 'v' -> `Split(Vertical)`, reset to Normal.
     - 'q' -> `ClosePane`, reset to Normal.
     - 'z' -> `ToggleZoom`, reset to Normal.
     - 't' -> `NewTab`, reset to Normal.
     - 'w' -> `CloseTab`, reset to Normal.
     - '1'..'9' -> `SwitchTab(digit - 1)`, reset to Normal.
     - Arrow keys -> `ResizePane(direction)`, reset to Normal.
     - Any other key -> reset to Normal, forward the key as if in Normal state.

Write tests covering:
- Ctrl+a transitions to LeaderPending and returns Consumed
- Leader + 'n' returns Split(Horizontal) and resets to Normal
- Leader + 'v' returns Split(Vertical)
- Leader + 'q' returns ClosePane
- Leader + 'z' returns ToggleZoom
- Leader + unknown key resets to Normal and forwards
- Ctrl+h/j/k/l returns NavigatePane in correct direction
- Regular keys in Normal state return Forward with correct bytes</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app keymap -- --nocapture</verify>
  <done>All keymap state machine tests pass. Leader key chord detection, timeout handling, and all multiplexer keybindings produce correct KeyActions.</done>
</task>

<task id="2" files="arcterm-app/src/keymap.rs" tdd="true">
  <action>Add leader key timeout tests and edge case handling:

1. Add a test helper `KeymapHandler::handle_key_with_time(&mut self, event, modifiers, app_cursor_keys, now: Instant) -> KeyAction` that accepts an explicit timestamp instead of using `Instant::now()`. This allows testing timeout behavior without real sleeps. Refactor `handle_key` to call this with `Instant::now()`.

2. Add `KeymapHandler::is_leader_pending(&self) -> bool` for UI display (showing a "leader pending" indicator).

Write tests:
- Leader key pressed, then after timeout_ms + 1ms another key arrives: the Ctrl+a byte (0x01) is produced as a Forward, then the second key is processed normally. Verify by calling `handle_key_with_time` with `entered_at + Duration::from_millis(timeout + 1)`.
- Leader key pressed, then within timeout_ms a leader action key arrives: no Ctrl+a byte is produced, the action is returned.
- Double-tap leader (Ctrl+a, Ctrl+a) sends 0x01 to the PTY (common pattern for sending literal Ctrl+a).
- `is_leader_pending` returns true between leader press and second key.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app keymap -- --nocapture</verify>
  <done>All timeout and edge case tests pass. Double-tap leader sends 0x01 correctly. Timeout expiry retroactively sends the leader byte.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs" tdd="false">
  <action>Add `mod keymap;` declaration to `arcterm-app/src/main.rs` after the `mod tab;` line. No other changes to main.rs.

Verify the entire crate compiles and all tests pass.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app && cargo test -p arcterm-app -- --nocapture 2>&1 | tail -20</verify>
  <done>`cargo build` succeeds. All arcterm-app tests pass including layout, tab, keymap, config, and input tests.</done>
</task>
