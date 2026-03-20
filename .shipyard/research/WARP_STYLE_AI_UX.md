# Research: Warp-Style AI UX — Bottom Panel, `#` Prefix Interception, Step Execution Monitoring

## Context

ArcTerm is a WezTerm fork. This document investigates three implementation questions
necessary for a Warp-like AI UX:

1. How to render a fixed-height AI panel pinned to the bottom of a pane without
   disrupting the terminal viewport.
2. How to intercept the `#` prefix on a command line and redirect Enter to ArcTerm's
   AI instead of the shell.
3. How to reliably detect when a command finishes executing.

The existing stack relevant to all three topics:

- Rust workspace, GPU rendering via `wgpu` (primary) + `glium` (fallback).
- Terminal model (`term`/`wezterm-term`) is a pure VT state machine.
- Pane rendering flows: `paint_pane` -> `with_lines_mut` -> `render_screen_line`.
- The tab bar already demonstrates a fixed-height strip rendered outside the viewport,
  with support for `tab_bar_at_bottom`.
- OSC 133 semantic prompts are fully parsed and cell-tagged; `CommandStatus` (133;D) is
  parsed but its handler is currently a no-op.
- Key dispatch: `key_event_impl` -> `process_key` -> `lookup_key` (overlay key table
  first, then window key table, then global InputMap) -> `perform_key_assignment` ->
  `pane.key_down`.
- Prior research in `INLINE_AI_SUGGESTION.md` established the overlay/shim pattern for
  ghost text and the cookie-based debounce. This document does not re-cover those topics.

---

## Topic 1: Compact Bottom Panel Rendering

### How the tab bar achieves a fixed strip at the bottom of the window

The tab bar, when `tab_bar_at_bottom = true` (config field at `config/src/config.rs`
line 477), renders below all panes using a simple pixel-geometry calculation:

```
tab_bar_y = (pixel_height - (tab_bar_height + border.bottom)).max(0)
```

(`wezterm-gui/src/termwindow/render/tab_bar.rs` lines 26-31)

The terminal viewport row count is reduced to exclude the tab bar height in
`apply_dimensions` (`wezterm-gui/src/termwindow/resize.rs` lines 254-263):

```rust
let avail_height = dimensions.pixel_height
    .saturating_sub(padding_top + padding_bottom + border.top + border.bottom)
    .saturating_sub(tab_bar_height);   // <-- tab bar is subtracted

let rows = avail_height / cell_height;   // rows reported to PTY
```

This means the tab bar works by:
1. Subtracting its pixel height from `avail_height` before dividing by `cell_height`.
2. Calling `tab.resize(size)` with the reduced row count, so the PTY reports a smaller
   terminal to the shell.
3. Painting the tab bar as a synthetic `render_screen_line` call with a `Line` object
   that is not part of the terminal model (lines 54-98 in `tab_bar.rs`).

The tab bar height equals exactly one cell height for the classic tab bar
(`render_metrics.cell_size.height as f32`), or `~1.75 * cell_height` for the fancy bar
(lines 103-114 in `tab_bar.rs`).

### How pane pixel geometry maps to PTY rows (the key constraint)

`TerminalSize` (`wezterm-term`) is the single source of truth for the PTY row/col
count. It is recomputed on every resize from:

```
rows = avail_height / cell_height
avail_height = window_height - padding - border - tab_bar_height
```

(`resize.rs` lines 250-263 and `mod.rs` lines 613-618)

A bottom panel could reserve N pixel rows by adding to `tab_bar_height` in this
formula. The PTY is already informed of the reduced row count because `tab.resize(size)`
propagates the change to all panes. A bottom panel of height `P` pixels would reduce
`rows` by `ceil(P / cell_height)`.

### Panel Rendering: Three Approaches

**Approach A: Piggyback on the existing `tab_bar_at_bottom` mechanism.**

Add an `ai_panel_height: usize` field to `TermWindow` (in pixels). In
`apply_dimensions`, add it to the `avail_height` subtraction alongside
`tab_bar_height`. In `paint_pass` (after the existing `paint_tab_bar` call), add a
`paint_ai_panel` call that renders a synthetic `Line` (or a box model element) at the
computed `ai_panel_y` pixel position, identical to how `paint_tab_bar` paints the tab
bar.

The panel height must be an integer multiple of `cell_height` or the PTY will receive a
fractional row count that rounds down, leaving unused pixel rows. For a 3-4 row panel
at a typical 20px cell height, this is 60-80 pixels.

Advantages:
- The PTY naturally loses N rows from its viewport, so shell output never overwrites the
  panel area.
- Uses the exact rendering path that already works for the tab bar bottom strip.
- No changes to `render_screen_line`, `pane.rs`, or the terminal model.
- The panel is window-global, not per-pane — appropriate for an AI panel that follows
  the active pane.

Disadvantages:
- The panel pixel region is not a real pane; it cannot receive shell output or cursor
  input.
- `tab_bar_height` and `ai_panel_height` must both be subtracted from `avail_height` in
  the same place; any code that checks only `tab_bar_height` (there are ~8 call sites)
  would need updating.

Evidence: `resize.rs` lines 254-263 (avail_height computation); `paint.rs` lines 267-271
(`paint_tab_bar` call); `tab_bar.rs` lines 54-98 (synthetic `render_screen_line`).

**Approach B: Thin horizontal split pane at the bottom.**

WezTerm already supports pane splits via `mux::Tab::split_pane`. A `SplitDirection::Down`
split with `size = SplitSize::Cells(3)` would create a 3-row pane at the bottom of the
active tab. The AI panel content would be rendered by a dedicated `Pane` implementation
(implementing the `Pane` trait), similar to `TermWizTerminalPane` (used by overlays like
the launcher). The panel pane would render its content using the normal `render_screen_line`
path.

Advantages:
- The PTY resize is handled automatically by the split system; no changes to the
  viewport calculation.
- The panel pane is a proper `Pane` and can render arbitrary styled content via the
  terminal model (`TermWizTerminal`).
- Splitting is already tested and cross-platform.
- The user can resize or close the panel pane with normal split management commands.

Disadvantages:
- Splits are per-tab, not per-window. If the user opens a new tab, the AI panel is not
  present unless explicitly created there too.
- The panel pane would appear in the split graph as a sibling to the user's shell pane.
  The user could accidentally `Ctrl+W` it, focus it, or zoom it.
- The AI panel content is driven by a `TermWizTerminal` (a VT state machine), which
  means the panel's rendering is indirect — the controlling thread must emit escape
  sequences to draw the panel content, rather than directly setting cell attributes.
- A minimum of 1 row is reserved even when the panel is not needed.

Evidence: `mux/src/tab.rs` `split_pane` function; `wezterm-gui/src/termwindow/mod.rs`
`SplitPane` key assignment handler; `wezterm-gui/src/termwindow/render/split.rs` for
split line rendering.

**Approach C: Box-model render element (existing `use_box_model_render` path).**

`paint_pane` already has a `use_box_model_render` branch (line 37 in `pane.rs`) that
calls `paint_pane_box_model` / `build_pane`, which constructs a `ComputedElement`. The
box model path can render arbitrary styled regions via `content_rect` and
`ComputedElementContent`. Adding a bottom panel as a child `ComputedElement` of the pane
box is theoretically possible.

Disadvantages:
- The box model render path is marked as experimental (`use_box_model_render` config
  flag) and the `build_pane` function has TODOs for visual bell and scrollbar (lines
  659-660 in `pane.rs`). It is not production-ready.
- It does not address the PTY row count; a panel rendered here would overlap terminal
  content.

This approach is not recommended.

### Decision: Approach A

Approach A is the correct path. The `tab_bar_at_bottom` mechanism is already proven for
exactly this use case: a fixed-height strip below all terminal rows, with the PTY resized
to exclude it. The implementation is additive — no existing behavior changes unless
`ai_panel_height > 0`.

The panel content is rendered as a synthetic `Line` (a fixed sequence of cells with
custom attributes), painted after `paint_tab_bar` in `paint_pass`. The panel line is not
derived from the terminal model; it is assembled by ArcTerm code and passed directly to
`render_screen_line` with `pane: None` and `stable_line_idx: None`, exactly as
`paint_tab_bar` does.

For 3-4 rows of panel content, the panel height would be `3 * cell_height` or
`4 * cell_height` pixels. The panel renders 3-4 synthetic lines stacked vertically,
each rendered by one `render_screen_line` call offset by `cell_height` increments.

Approach B (thin pane split) is architecturally viable as a fallback — especially for a
panel that needs to accept keyboard input — but requires dealing with split lifecycle and
per-tab state management, which adds significant complexity for a first iteration.

### Integration Points

- `wezterm-gui/src/termwindow/resize.rs` lines 254-263: subtract `ai_panel_height` from
  `avail_height` alongside `tab_bar_height`.
- `wezterm-gui/src/termwindow/render/paint.rs` lines 267-278: add `paint_ai_panel` call
  after `paint_tab_bar`.
- New file: `wezterm-gui/src/termwindow/render/ai_panel.rs`: implement `paint_ai_panel`,
  following the structure of `tab_bar.rs`.
- `TermWindow` struct (`mod.rs`): add `ai_panel_height: usize` and
  `ai_panel_content: Vec<Line>` (or equivalent).
- All existing call sites that reference `tab_bar_height` for geometry calculations
  (approximately 8 places in `mod.rs`, `resize.rs`, `pane.rs`, `mouseevent.rs`,
  `split.rs`, `charselect.rs`, `paneselect.rs`, `palette.rs`) must also account for
  `ai_panel_height`. A computed property `total_chrome_height()` returning
  `tab_bar_height + ai_panel_height` would centralize this.

---

## Topic 2: `#` Prefix Interception

### The question

When the user types `# some AI query` at a shell prompt and presses Enter, ArcTerm
should intercept the Enter key and route the text to the AI rather than the shell. This
requires:
1. Knowing that the user is at a shell prompt (not inside a program).
2. Reading the content of the current input line to check whether it starts with `#`.
3. Consuming Enter before it is sent to the shell.

### Reading the current input line

The `Pane` trait exposes `get_lines(range: Range<StableRowIndex>) -> (StableRowIndex,
Vec<Line>)` (`mux/src/pane.rs` line 200). `Line` has `pub fn as_str(&self) -> Cow<str>`
(`wezterm-surface/src/line/line.rs` line 631). Together, these allow reading the text of
the cursor row:

```rust
let cursor = pane.get_cursor_position();
let (first, lines) = pane.get_lines(cursor.y..cursor.y + 1);
if let Some(line) = lines.first() {
    let text = line.as_str();
    if text.trim_start().starts_with('#') {
        // intercept
    }
}
```

This read is synchronous and takes the pane's internal mutex briefly. It is safe to call
from the GUI main thread inside a key handler.

However, `get_lines` returns the entire line, including the shell prompt prefix (e.g.,
`$ # query`). To extract only the user's input, the input zone's `start_x` from
`get_semantic_zones()` should be used to index into the line's cells:

```rust
let zones = pane.get_semantic_zones()?;
let input_zone = zones.iter().find(|z| {
    z.semantic_type == SemanticType::Input
        && z.start_y <= cursor.y
        && cursor.y <= z.end_y
});
if let Some(zone) = input_zone {
    let (_, lines) = pane.get_lines(zone.start_y..cursor.y + 1);
    // lines[0] cells from zone.start_x onwards are the user's input
    let input_text: String = lines[0]
        .visible_cells()
        .skip(zone.start_x)
        .map(|c| c.str())
        .collect();
}
```

Without OSC 133 (no `input_zone`), fallback to scanning from the character after the
last `$` or `%` prompt character on the current line. This is heuristic and will
misbehave for prompts that do not use those characters.

### Key interception: where to hook

The question is: at what point in the dispatch chain can Enter be intercepted
*conditionally* (only when the first character of the input is `#`)?

The key dispatch chain (established in `INLINE_AI_SUGGESTION.md`) is:

```
process_key:
  1. Modal key_down (if modal active)
  2. Overlay key_table_state (if pane has overlay)
  3. Window key_table_state (leader key tables)
  4. Global InputMap (config key_bindings)
  5. pane.perform_assignment() (via perform_key_assignment)
  6. pane.key_down() -> writes to PTY
```

**Approach A: KeyAssignment in the global InputMap.**

Add a global key binding `Enter -> InterceptHashPrefix`. In
`perform_key_assignment`, handle `InterceptHashPrefix`:

```rust
InterceptHashPrefix => {
    let cursor = pane.get_cursor_position();
    let input_text = read_input_zone_text(&pane, cursor.y)?;
    if input_text.trim_start().starts_with('#') {
        let query = input_text.trim_start_matches('#').trim().to_string();
        self.send_to_ai_panel(pane_id, query);
        return Ok(PerformAssignmentResult::Handled);
    }
    // Not a # query: fall through to send Enter to shell
    pane.key_down(KeyCode::Enter, Modifiers::NONE)?;
    return Ok(PerformAssignmentResult::Handled);
}
```

This unconditionally binds Enter globally, so the `perform_key_assignment` handler must
always send Enter to the shell when the condition is not met. It works, but binds Enter
permanently, which could interfere with Enter in overlay panes (the fuzzy launcher, the
copy overlay, the color palette picker) unless those overlays consume Enter first in
their own key tables (they do — overlays have priority at step 2).

**Approach B: Overlay pane key table.**

Following the pattern established for AI ghost text in `INLINE_AI_SUGGESTION.md`: when
at an `Input` semantic zone, assign a thin shim overlay over the pane with a key table
that maps `Enter -> InterceptHashPrefix`. The shim is active only while the user is at
the shell prompt (detected at each keystroke). When the shim's key table intercepts
Enter:
- If line starts with `#`: route to AI, erase the typed line, and cancel the shim.
- Otherwise: write `\r` to `pane.writer()` (equivalent to Enter) and cancel the shim.

The shim overlay is cancelled when OSC 133;C (`MarkEndOfInputAndStartOfOutput`) fires,
meaning the shell accepted a command — at which point it is no longer needed.

This is architecturally cleaner because the shim is only active at the prompt, not
globally.

**Approach C: Terminal-layer hook (not viable).**

Hooking at `pane.key_down` is too late: by then, the keystroke has already been encoded
and written to the PTY. There is no pre-write callback in the `Pane` trait.

### Reading the line content: timing concern

The line text is read at key-press time, not asynchronously. This is synchronous and
correct — the terminal model is not changing during key handling (the PTY reader thread
updates the model on parse events, which happen between render frames, not during key
dispatch). The pane mutex acquisition in `get_lines` is brief.

### The `#` at `start_x` vs. column 0

Shell prompts occupy columns 0..(start_x). The user's typed characters begin at
`start_x`. If the user types `# query`, the first character at `start_x` is `#`. The
check should be:

```
input_text[0] == '#'
```

not `line[0] == '#'`, because `line[0]` may be the beginning of the shell prompt.

Without OSC 133, the fallback is to scan the line for the last prompt token (`$`, `%`,
`#` when used as a prompt character) and read text after it. This is ambiguous since `#`
itself is a common prompt character in root shells.

### Decision

**Use Approach B (overlay pane key table) as the primary approach.** The shim overlay
is the same mechanism as the AI ghost text shim from `INLINE_AI_SUGGESTION.md`. A
single "AI prompt shim" overlay can handle both ghost text (via `with_lines_mut`) and
`#` prefix interception (via key table). When Enter is pressed:
- The shim reads the input zone text.
- If it starts with `#`, the shim routes to AI and calls `erase_line` (write `\x15`
  — Ctrl+U — to the PTY to clear the line, as if the user never typed it), then cancels
  itself.
- Otherwise, the shim writes `\r` to `pane.writer()` and cancels itself (the shell
  receives Enter normally).

Approach A (global InputMap) is acceptable as a simpler starting point, but the
unconditional global Enter binding is a footgun for future overlay conflicts.

OSC 133 support is strongly recommended. Without it, detecting that the cursor is at
the shell prompt input zone (not inside vim, inside a Python REPL, etc.) is
unreliable. The `#` character is valid input to many programs; intercepting it
indiscriminately would break those programs.

### OSC 133 as a gate

The `#` prefix feature MUST be gated on OSC 133 `Input` zone detection. If no `Input`
zone is detected at the cursor position when Enter is pressed, the interception is
skipped and Enter is passed to the shell normally. This prevents breakage for users who
have not configured shell integration or are running programs inside the shell.

### Integration Points

- `config/src/keyassignment.rs`: add `InterceptHashPrefix` to the `KeyAssignment` enum
  (for Approach A) or simply handle Enter inside the overlay key table without a new
  variant.
- `wezterm-gui/src/termwindow/mod.rs`: `perform_key_assignment` or the overlay shim's
  key handler.
- `wezterm-gui/src/overlay/ai_suggestion.rs`: the AI prompt shim (combining ghost text
  and `#` interception).
- `arcterm-ai/src/prompt.rs` (new): the logic that extracts the query from input text
  and submits it to the AI backend.

---

## Topic 3: Step Execution Monitoring

### OSC 133 command lifecycle markers

The OSC 133 protocol provides four events relevant to command execution monitoring:

| Escape sequence | FinalTermSemanticPrompt variant | Handler in performer.rs | Effect |
|---|---|---|---|
| `\e]133;A\e\\` or `\e]133;P\e\\` | `FreshLineAndStartPrompt` / `StartPrompt` | lines 876-888 | `pen.set_semantic_type(Prompt)` |
| `\e]133;B\e\\` | `MarkEndOfPromptAndStartOfInputUntilNextMarker` | lines 896-900 | `pen.set_semantic_type(Input)` |
| `\e]133;C\e\\` | `MarkEndOfInputAndStartOfOutput` | lines 907-911 | `pen.set_semantic_type(Output)` |
| `\e]133;D;N\e\\` | `CommandStatus { status: N }` | line 913-915 | **no-op (handler stub)** |

Evidence: `term/src/terminalstate/performer.rs` lines 876-915.

The `CommandStatus` variant is parsed (the `status: N` exit code is extracted in
`wezterm-escape-parser/src/osc.rs` lines 757-760) but the `TerminalState` does not store
it — the match arm at line 913-915 is an empty block. This means exit codes are
currently discarded.

### How OSC 133 events reach the GUI

OSC 133 sequences are processed inside the `parse_buffered_data` thread
(`mux/src/lib.rs` lines 140-243). They call `pane.perform_actions(actions)`, which
calls `TerminalState::perform_actions` on the `LocalPane`'s terminal. The semantic type
changes update `pen.semantic_type`, which is reflected in `CellAttributes` of
subsequently-printed cells.

These events do NOT currently fire a `MuxNotification`. When `133;C` fires (command
starts), the semantic type changes to `Output`, but no subscriber is notified separately
— the change is only visible when the GUI re-reads lines via `with_lines_mut`.

When `133;D` fires (command finishes), the current no-op handler means no notification
occurs at all.

### Three approaches to detect command completion

**Approach A: Extend the `CommandStatus` handler to fire a `MuxNotification`.**

Modify `performer.rs` line 913-915:

```rust
OperatingSystemCommand::FinalTermSemanticPrompt(
    FinalTermSemanticPrompt::CommandStatus { status, .. }
) => {
    self.last_command_exit_status = Some(status);  // store on TerminalState
    self.alert(Alert::CommandComplete { status });  // fire alert
}
```

Add `CommandComplete { status: i64 }` to the `Alert` enum (`term/src/terminal.rs`
line 47).

`LocalPaneNotifHandler::alert` (`mux/src/localpane.rs` lines 924-950) already converts
`Alert` values to `MuxNotification::Alert`. Adding `Alert::CommandComplete` would
automatically flow through to `MuxNotification::Alert`, which ArcTerm's AI subscriber
can listen for.

This is the correct, structured approach. It adds one field to `TerminalState` and one
variant to `Alert` and `MuxNotification::Alert`. All other code paths are unchanged.

**Approach B: Watch for the `Input` semantic zone reappearing after `Output`.**

When the shell emits a new prompt after a command completes, `MarkEndOfPromptAndStart-
OfInputUntilNextMarker` (`133;B`) fires, transitioning the semantic type from `Output`
back to `Input`. An ArcTerm subscriber can track transitions:

```
state machine per pane:
  Idle -> (133;B) -> AtPrompt -> (133;C) -> CommandRunning -> (133;B) -> CommandComplete
```

This can be implemented entirely in the GUI layer without touching the `term` crate, by
observing `MuxNotification::PaneOutput` and checking `get_semantic_zones()` on each
notification to detect the transition from `Output` to `Input`.

Advantages:
- No changes to `term` or `mux` crates.
- Works even without `133;D` support (e.g., older shell integrations that omit
  `133;D;N`).

Disadvantages:
- `MuxNotification::PaneOutput` fires on every output byte, making this a hot-path
  check.
- `get_semantic_zones()` acquires the pane mutex on every call; doing this on every
  `PaneOutput` notification may introduce contention.
- Does not provide the exit code.
- Race condition: the new `Input` zone might appear before all output has been
  rendered (e.g., if the shell emits the prompt before the command's output is
  complete, which can happen with buffered output).

**Approach C: Foreground process name change as a fallback heuristic.**

`pane.get_foreground_process_name(CachePolicy::AllowStale)` returns the name of the
foreground process running in the PTY. When a command finishes, the foreground process
reverts to the shell. This can be polled.

Disadvantages:
- Not event-driven; requires polling with a timer.
- `CachePolicy::FetchNow` (accurate) has non-trivial overhead (reads from procfs or
  uses platform-specific APIs).
- Gives no exit code.
- Completely unreliable when the shell itself spawns sub-shells or when the PTY is a
  multiplexer (tmux, screen).

This is a last-resort fallback only.

### Decision: Approach A as primary, Approach B as fallback

**Approach A** (extend `CommandStatus` to fire `Alert::CommandComplete`) is the
correct long-term solution. It is minimal, architecturally clean, and consistent with
how all other OSC 133 events propagate. It is also the only approach that provides the
exit code.

**Approach B** (semantic zone transition detection) should be the fallback for shells
that emit `133;A/B/C` but not `133;D`. Many real-world fish and bash integrations do
not emit `133;D`. A state machine in the ArcTerm AI subscriber watching for the
`Output -> Input` zone transition covers this gap, with acceptable overhead if the
check is only performed when a command is known to be running (i.e., when the state
machine is in the `CommandRunning` state).

The implementation plan:

1. Add `last_command_exit_status: Option<i64>` to `TerminalState`
   (`term/src/terminalstate/mod.rs`).
2. Add `Alert::CommandComplete { status: i64 }` to `term/src/terminal.rs`.
3. In `performer.rs` line 913-915, store the status on `TerminalState` and call
   `self.alert(Alert::CommandComplete { status })`.
4. In `MuxNotification::Alert` handler in `wezterm-gui/src/termwindow/mod.rs` (line
   1249), add an arm for `Alert::CommandComplete` that notifies the ArcTerm AI
   subscriber.
5. The ArcTerm AI subscriber (in `arcterm-ai`) listens for `MuxNotification::Alert
   { alert: Alert::CommandComplete { status }, pane_id }` and triggers step completion
   logic.
6. Additionally, maintain a per-pane semantic zone state machine (Approach B logic) to
   handle shells that emit `133;A/B/C` without `133;D`.

### Detecting command completion without OSC 133 at all

If the user's shell emits no OSC 133 sequences, neither approach works. The
`get_foreground_process_name` heuristic (Approach C) is the only option. ArcTerm should
document that full AI monitoring requires shell integration and provide the existing
WezTerm shell integration scripts (for bash, zsh, fish, nushell) as a first-class setup
step.

---

## Comparison Matrix

| Criteria | Approach A (Tab-bar-style panel) | Approach B (Split pane) | Approach C (Box model) |
|---|---|---|---|
| Mechanism | Subtract panel height from avail_height; paint synthetic lines | `split_pane(Down, N)` with a custom Pane impl | `ComputedElement` child in box model render path |
| PTY row impact | Automatic (same as tab bar) | Automatic (same as any split) | Not addressed |
| Per-tab or per-window | Per-window (follows active pane) | Per-tab (must be created per tab) | Per-pane |
| User can accidentally interact | No (not a split pane) | Yes (focus, close, zoom) | No |
| Content rendering | Synthetic Lines via `render_screen_line` | Full `Pane` impl + `TermWizTerminal` | Box model elements |
| Implementation complexity | Low (follows tab bar pattern) | Medium | High (experimental path) |
| Risk | Low | Medium | High |

| Criteria | `#` Approach A (Global InputMap) | `#` Approach B (Overlay key table) |
|---|---|---|
| Scope | Globally overrides Enter | Active only at shell prompt (overlay) |
| OSC 133 required | No (but recommended as gate) | Yes (to know when prompt is active) |
| Overlay conflicts | Possible (unless handled carefully) | None (overlay has priority) |
| Implementation complexity | Low | Medium (same overlay as ghost text) |

| Criteria | Step Monitoring Approach A (Alert::CommandComplete) | Approach B (Zone transition) | Approach C (Process heuristic) |
|---|---|---|---|
| Provides exit code | Yes | No | No |
| Shell cooperation required | Yes (133;D) | Yes (133;A/B/C) | No |
| Event-driven | Yes (via MuxNotification) | Yes (via PaneOutput) | No (polling) |
| Hot-path overhead | Zero | Medium (per PaneOutput check) | High (procfs/OS calls) |
| Changes term crate | Yes (TerminalState + Alert) | No | No |
| Reliability | High | Medium | Low |

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| AI panel height not an exact multiple of cell_height, leaving pixel gaps at window bottom | Med | Low | Always compute panel height as `N * render_metrics.cell_size.height`; never use arbitrary pixel values. |
| Existing code that checks `tab_bar_height` for geometry (8 call sites) not updated, causing panel/pane overlap | High | High | Introduce a `total_fixed_chrome_height()` method returning `tab_bar_height + ai_panel_height`; use it everywhere. Do not inline the calculation. |
| `#` interception fires inside a running program (e.g., Python REPL) because no OSC 133 is present | High | High | Gate on `SemanticType::Input` zone at cursor. If zone is absent or `Output`, do not intercept. Document requirement. |
| `#` is a root shell prompt character; users whose PS1 ends in `#` will have every Enter intercepted | Med | High | Require the `#` to be the first character of the OSC 133 `Input` zone, not the first character of the visual line. A prompt ending in `#` prints it in the `Prompt` zone, not the `Input` zone. OSC 133 makes this unambiguous. |
| Shell emits no OSC 133 — command completion cannot be detected | High | Med | Fall back to zone transition heuristic (Approach B). Document shell integration setup. |
| `Alert::CommandComplete` fires on the parser thread; `alert()` posts to main thread via `spawn_into_main_thread` — delay possible | Low | Low | Acceptable. The `alert` mechanism already uses this pattern for all other Alert variants. The delay is bounded by the next main thread loop tick (~16ms). |
| Shell emits `133;D` with garbage status values (some integrations use `133;D;`) | Low | Low | Parse status as `i64` with a default of 0 on parse failure (already handled in `osc.rs`). |
| `get_lines` called during key handler contends with PTY parser thread that is also writing lines | Low | Med | The pane mutex serializes access. `get_lines` is brief (one row). Contention is momentary. |

---

## Implementation Considerations

### Integration points with existing code

**Topic 1 (Bottom Panel):**
- `wezterm-gui/src/termwindow/resize.rs` lines 254-263: add `ai_panel_height` to
  `avail_height` subtraction.
- `wezterm-gui/src/termwindow/render/paint.rs` after `paint_tab_bar` call: add
  `paint_ai_panel`.
- All 8 call sites that reference `tab_bar_height` for geometry: update to include
  `ai_panel_height`. Consider a `total_fixed_chrome_height()` helper on `TermWindow`.

**Topic 2 (`#` Prefix):**
- `wezterm-gui/src/overlay/ai_suggestion.rs`: extend the ghost text shim with Enter
  key interception and `#` prefix check.
- `wezterm-surface/src/line/line.rs` `as_str()` and `mux/src/pane.rs` `get_lines`: used
  as-is, no changes needed.
- `term/src/terminalstate/mod.rs` `get_semantic_zones`: used as-is.

**Topic 3 (Step Monitoring):**
- `term/src/terminal.rs`: add `CommandComplete` to `Alert`.
- `term/src/terminalstate/mod.rs`: add `last_command_exit_status: Option<i64>`.
- `term/src/terminalstate/performer.rs` line 913: populate `last_command_exit_status`
  and call `alert`.
- `wezterm-gui/src/termwindow/mod.rs` MuxNotification handler: add
  `Alert::CommandComplete` arm.
- `arcterm-ai` crate: subscribe to `MuxNotification::Alert` and maintain per-pane state
  machines for both Approach A and Approach B completion detection.

### Testing strategy

- **Topic 1:** Verify that opening a pane with the AI panel active reports `rows =
  expected_rows` to the shell (`stty size`). Verify no pixel gap appears between the
  panel and the pane. Verify the panel renders correctly when `tab_bar_at_bottom = true`
  (panel below tab bar) and `tab_bar_at_bottom = false` (panel below pane, above bottom
  edge).
- **Topic 2:** Unit test: construct a `Terminal` model, emit `133;B`, type `# test
  query\r`. Verify that `get_semantic_zones()` shows `start_x = N` and that the line
  text from `N` onwards is `# test query`. Integration test: synthetic keystroke of
  Enter with the AI shim overlay active while input zone text starts with `#`; verify
  the query is routed to AI and the pane writer does NOT receive `\r`.
- **Topic 3:** Unit test: feed a `Terminal` the sequence `133;D;0` and verify
  `last_command_exit_status == Some(0)` and `Alert::CommandComplete { status: 0 }` is
  fired. Integration test with zsh+OSC 133 integration: run a command, verify the
  `CommandComplete` notification arrives with the correct exit status.

---

## Sources

1. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/termwindow/render/tab_bar.rs` —
   `paint_tab_bar`, `tab_bar_pixel_height`, `tab_bar_at_bottom` logic (lines 10-119)
2. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/termwindow/resize.rs` —
   `apply_dimensions` `avail_height` computation (lines 250-263); `set_window_size`
   (lines 475-533)
3. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/termwindow/render/pane.rs` —
   `paint_pane`, `top_bar_height`/`bottom_bar_height` (lines 64-77); `build_pane` box
   model path (lines 585-688)
4. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/termwindow/render/paint.rs` —
   `paint_pass` (lines 161-278), render order: panes, splits, tab bar, borders, modal
5. `/Users/lgbarn/Personal/arcterm/config/src/config.rs` line 477 —
   `tab_bar_at_bottom: bool` config field
6. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/termwindow/mod.rs` lines 613-618 —
   `TerminalSize` construction; lines 655-658 — `tab_bar_height` in initial dimensions
7. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/termwindow/keyevent.rs` —
   `lookup_key` priority (lines 211-237); `process_key` dispatch (lines 239-428);
   `key_event_impl` (lines 599-651)
8. `/Users/lgbarn/Personal/arcterm/mux/src/pane.rs` — `Pane::get_lines` (line 200);
   `Pane::get_semantic_zones` (line 303); `WithPaneLines` trait (lines 349-360)
9. `/Users/lgbarn/Personal/arcterm/wezterm-surface/src/line/line.rs` line 631 —
   `Line::as_str()`
10. `/Users/lgbarn/Personal/arcterm/term/src/terminalstate/performer.rs` lines 876-915 —
    OSC 133 handlers for all `FinalTermSemanticPrompt` variants; `CommandStatus` no-op
    (lines 913-915)
11. `/Users/lgbarn/Personal/arcterm/term/src/terminal.rs` lines 47-78 — `Alert` enum,
    `AlertHandler` trait
12. `/Users/lgbarn/Personal/arcterm/mux/src/lib.rs` lines 56-98 — `MuxNotification`
    enum
13. `/Users/lgbarn/Personal/arcterm/mux/src/localpane.rs` lines 924-950 —
    `LocalPaneNotifHandler::alert` (Alert -> MuxNotification routing)
14. `/Users/lgbarn/Personal/arcterm/.shipyard/research/INLINE_AI_SUGGESTION.md` —
    prior research: overlay shim pattern, cookie debounce, key table priority system,
    OSC 133 zone detection

---

## Uncertainty Flags

- **Bottom panel with `tab_bar_at_bottom = true`:** When both the tab bar and the AI
  panel are at the bottom, their combined height must be subtracted from `avail_height`.
  The rendering order (panel above or below tab bar) is a UX decision not resolved here.
  Both require the same geometry calculation change; the ordering is controlled by the
  sequence of `render_screen_line` calls in `paint_pass`.

- **Panel pixel height and cell alignment:** If the panel height is not an exact
  multiple of `cell_height`, the pixel gap between the panel top edge and the last
  terminal row creates an unrendered strip. The resolution (round up vs. round down) is
  unspecified here. Rounding up (larger panel) avoids the gap at the cost of reducing
  the terminal by an extra row.

- **`get_lines` vs. `with_lines_mut` for reading the current line at keystroke time:**
  `get_lines` returns a cloned `Vec<Line>` (owned copy). `with_lines_mut` takes a
  mutable borrow of the renderer callback. For a one-row read during key handling,
  `get_lines` is correct and safe. The overhead of cloning one `Line` is negligible.
  Confirmed via `mux/src/localpane.rs` line 210 and `mux/src/pane.rs` line 200.

- **`#` at column 0 in shells without OSC 133:** In `sh`, `bash`, and `ksh`, a `#` at
  the start of a line (outside quotes) is a comment; pressing Enter sends it to the
  shell, which ignores it. This means accidental interception (sending to AI instead of
  shell) is low-impact in OSC-133-less mode — the shell would have ignored the line
  anyway. However, in `zsh` with `INTERACTIVE_COMMENTS` off, `#` is not a comment and
  would be treated as a command error. Gating on OSC 133 avoids this edge case entirely.

- **`Alert::CommandComplete` thread safety:** `alert()` is called on the PTY parser
  thread. `LocalPaneNotifHandler::alert` uses `spawn_into_main_thread`, which posts to
  the GUI thread's SpawnQueue. This is the same path as all other Alert variants.
  However, it has not been directly confirmed from the source that `spawn_into_main_thread`
  is reentrant and safe to call from multiple parser threads simultaneously. It is
  strongly implied by the existing use for `Bell`, `WindowTitleChanged`, etc.

- **Shells emitting `133;D` but with non-standard status encoding:** The WezTerm OSC
  parser reads the exit status as the numeric value after `133;D;`. Some shell
  integrations (particularly older fish integrations) may not emit `133;D` at all, or
  may emit `133;D` without a status value (treated as status 0 by the parser). The
  fallback (zone transition heuristic) covers this case.

- **Multiple `133;D` per prompt:** Some shells emit `133;D` immediately after each
  piped command in a pipeline (not just at the end of the pipeline). If `CommandComplete`
  fires multiple times, the AI subscriber must deduplicate by checking whether the next
  event is `133;A`/`133;B` (new prompt) before acting.
