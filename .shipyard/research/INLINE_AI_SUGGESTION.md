# Research: Inline AI Command Suggestion — Shell Detection, Ghost Text, Debounce, Input Interception

## Context

ArcTerm is a WezTerm fork. The planned `arcterm-ai` crate includes an inline command suggestion
feature: as the user types at a shell prompt, ArcTerm debounces keystrokes, queries an LLM, and
renders a "ghost text" suggestion to the right of the cursor. Pressing Tab (or another key) accepts
the suggestion; any other key dismisses it.

The existing stack relevant to this feature:
- Rust (2018/2021 editions), `smol` / `promise::spawn` for async work, no Tokio in the GUI process.
- GPU rendering via `wgpu` (primary) and `glium` (fallback). The render pipeline is:
  `TermWindow::paint_impl` -> `paint_pane` -> `render_screen_line`.
- The terminal model (`term` crate) is a pure VT state machine; it does not know about the GUI.
- OSC 133 (FinalTerm semantic prompts) is **fully implemented** in the `wezterm-escape-parser` and
  `term` crates.
- Key events flow: native event -> `window` crate -> `TermWindow::key_event_impl` -> `process_key`
  -> `lookup_key` -> `perform_key_assignment` -> `pane.key_down`.

Prior research in `.shipyard/research/AI_INTEGRATION.md` covers pane context reading, HTTP client
selection, and overlay rendering. This document focuses specifically on the four implementation
questions listed below.

---

## Topic 1: Shell Prompt Detection via OSC 133

### How WezTerm detects shell prompts

OSC 133 (the FinalTerm / semantic prompt protocol) is fully parsed and applied to cell attributes
in the terminal state machine.

**Parser location:** `wezterm-escape-parser/src/osc.rs` line 511.
The numeric OSC code `"133"` is registered as `FinalTermSemanticPrompt` in the `OscCode` enum.

**Variants parsed** (`osc.rs` lines 720-761):

| Shell emits | OSC code | Variant | Effect |
|---|---|---|---|
| Prompt start (fresh line) | `133;A` | `FreshLineAndStartPrompt` | `pen.set_semantic_type(Prompt)` |
| Prompt start (explicit) | `133;P` | `StartPrompt` | `pen.set_semantic_type(Prompt)` |
| End of prompt / start of input | `133;B` | `MarkEndOfPromptAndStartOfInputUntilNextMarker` | `pen.set_semantic_type(Input)` |
| End of prompt / input until EOL | `133;I` | `MarkEndOfPromptAndStartOfInputUntilEndOfLine` | `pen.set_semantic_type(Input)` + `clear_semantic_attribute_on_newline = true` |
| User hits Enter / command starts | `133;C` | `MarkEndOfInputAndStartOfOutput` | `pen.set_semantic_type(Output)` |
| Command finishes | `133;D;N` | `CommandStatus { status: N }` | No-op currently (handler stub at `performer.rs` line 913) |
| End of output, start new prompt | `133;N` | `MarkEndOfCommandWithFreshLine` | `fresh_line()` + `pen.set_semantic_type(Prompt)` |

**Cell-level tagging:** The `SemanticType` enum (`wezterm-cell/src/lib.rs` lines 181-185) has three
variants:
```
Output = 0   (default for all cells)
Input  = 1   (user's typing area)
Prompt = 2   (shell prompt display)
```
`SemanticType` is packed into bits 13-14 of each cell's `CellAttributes` bitfield
(`wezterm-cell/src/lib.rs` line 211). Every character printed while `pen.semantic_type == Input` is
tagged `Input`; every character printed while `pen.semantic_type == Output` is tagged `Output`.

**Zone computation:** `TerminalState::get_semantic_zones()` (`term/src/terminalstate/mod.rs`
line 2709) scans all physical lines and merges adjacent cells with the same `SemanticType` into
contiguous `SemanticZone` structs (`term/src/lib.rs` lines 117-123):
```rust
pub struct SemanticZone {
    pub start_y: StableRowIndex,
    pub start_x: usize,
    pub end_y: StableRowIndex,
    pub end_x: usize,
    pub semantic_type: SemanticType,
}
```

**Detecting "user is at a shell prompt typing":**

The condition "user is at the shell prompt, waiting for input" is met when:
1. The cursor's current `SemanticType` is `Input` (the shell sent `133;B` or `133;I` but not yet
   `133;C`).
2. The foreground process is the shell itself (not a child process).

The practical check:

```rust
// Condition 1: semantic zone at cursor position is Input
let zones = pane.get_semantic_zones()?;
let cursor = pane.get_cursor_position();
let at_input = zones.iter().any(|z| {
    z.semantic_type == SemanticType::Input
        && cursor.y >= z.start_y
        && cursor.y <= z.end_y
});

// Condition 2: no child process running (optional, belt-and-suspenders)
let proc = pane.get_foreground_process_name(CachePolicy::AllowStale);
let shell_is_foreground = proc.map(|p| is_shell_binary(&p)).unwrap_or(false);
```

The `CommandStatus` variant (OSC `133;D;N`) provides the exit code of the last command, but the
handler in `performer.rs` line 913 is currently a no-op — it just silently ignores the sequence.
If exit-code-aware suggestions are needed, this handler would need to store the status in
`TerminalState`.

**Fallback (no OSC 133):** Without semantic zones, the feature degrades gracefully: detect
"cursor is at the bottom-most non-empty line" as a heuristic, combined with foreground process
name check. This is weaker but does not require shell configuration.

### Decisions

- **Primary detection method:** `get_semantic_zones()` + cursor position, checked at each keystroke
  debounce firing, when the suggestion-state machine is active.
- **Fallback:** Cursor is on the last physical row and the foreground process is a known shell
  binary.
- **Trigger for OSC 133 support:** Encourage users to add `133` markers via shell init (e.g., the
  existing WezTerm shell integration scripts already do this for bash/zsh/fish/nushell).

---

## Topic 2: Ghost Text / Overlay Rendering

### How WezTerm renders the cursor

The cursor is rendered inside `render_screen_line` (`wezterm-gui/src/termwindow/render/screen_line.rs`
lines 306-421). At the start of rendering a line, the code computes `cursor_range` (the cell
columns occupied by the cursor) and `cursor_range_pixels`. If `cursor_range` is non-empty, it calls
`compute_cell_fg_bg` to determine the cursor color and shape, then allocates a quad at GPU layer 0
or 2 (bar-style cursors use layer 2, block/underline use layer 0) and sets its texture from the
`GlyphCache::cursor_sprite` atlas entry.

`RenderScreenLineParams` (`wezterm-gui/src/termwindow/render/mod.rs` lines 126-171) has no field
for extra/ghost text. There is currently no mechanism for rendering additional characters beyond the
terminal's own cell content at the cursor position.

### Existing "extra rendering on top of terminal" patterns

**Pattern A: Dead-key / IME composing text** (`screen_line.rs` lines 79-97).
When the user has an active dead-key composition (`DeadKeyStatus::Composing(text)`), the
`composition_width` is added to the `cursor_range`, causing the cursor to widen to cover the
composing text. The text itself is rendered through the normal shaped-cluster path — it is injected
into the `Line` before shaping via a modified line. This is the closest existing pattern to ghost
text.

**Pattern B: Search overlay line mutation** (`wezterm-gui/src/overlay/copy.rs` lines 1415-1490).
`CopyOverlay` implements `Pane::with_lines_mut`. Inside that implementation, it intercepts the line
data before it reaches the renderer, calls `line.fill_range(...)` and `line.overlay_text_with_attribute(...)`
to modify individual cells in-flight, without permanently altering the terminal model. This is the
**correct architectural pattern** for ghost text: mutate a line's cells in the `with_lines_mut`
call, rendering additional text with a distinct `CellAttributes` (e.g., dimmed foreground), then
return. The modification is not written back to the terminal state.

**Pattern C: Tab-bar rendering** uses `render_screen_line` with a synthetic `Line` object, not
derived from the terminal model at all. Less relevant here.

### How to render ghost text using Pattern B

1. Implement ghost text via a custom `Pane` wrapper (the `CopyOverlay` pattern): wrap the active
   pane with a thin shim that implements `with_lines_mut`. In that method:
   - Call the delegate's `with_lines_mut` to get the real lines.
   - For the cursor row (identified by `cursor.y`), clone the line, call
     `line.set_cell(col, Cell::new(ch, dimmed_attrs))` for each character of the suggestion,
     starting at `cursor.x`.
   - Pass the mutated line back to the upstream `with_lines` callback.
   This renders dimmed characters to the right of the cursor without touching the terminal model.

2. Alternatively (simpler, lower fidelity): write the ghost text as escape sequences directly
   into the pane using `pane.inject_output(esc_sequence)`, then erase it before the next keystroke
   is forwarded to the shell. This is fragile — if the process outputs anything between the inject
   and erase, the display is corrupt. Not recommended.

3. Another alternative: add a `ghost_text: Option<String>` field to `RenderScreenLineParams` and
   render it inside `render_screen_line` after the cursor quad. This requires modifying a
   heavily-shared struct and touches the hot render path. It is more invasive but avoids the
   wrapping layer.

**Decision: Use Pattern B (with_lines_mut wrapper shim).**

Rationale: `CopyOverlay` proves the pattern is correct and production-stable. The wrapper shim
does not require modifying `RenderScreenLineParams` or `render_screen_line`. It is assignable to a
pane via `assign_overlay_for_pane`, meaning the real pane continues running underneath and the
ghost text is only visible while the shim is active. When the user accepts or dismisses the
suggestion, the shim is cancelled via `cancel_overlay_for_pane`, exactly as the search overlay does.

**Color for ghost text:** Use the `config.resolved_palette.foreground` color at reduced alpha, or
the terminal's `ansi_colors[8]` (bright black / dark gray). Concretely:
`CellAttributes::default().set_foreground(ColorAttribute::PaletteIndex(8)).set_italic(true)`.

### Confirmed: no existing ghost-text mechanism

`RenderScreenLineParams` has 20 fields, none of which carry suggestion text, extra cells, or
inline annotations (`wezterm-gui/src/termwindow/render/mod.rs` lines 126-171). This is a new
capability.

---

## Topic 3: Debounce and Async Patterns

### How WezTerm handles timers

WezTerm does **not** use a platform timer API directly. The canonical pattern, used throughout the
codebase, is:

```rust
promise::spawn::spawn(async move {
    smol::Timer::after(Duration::from_millis(N)).await;
    window.notify(TermWindowNotif::Apply(Box::new(move |term_window| {
        // runs on the GUI main thread
        term_window.do_something();
    })));
    anyhow::Result::<()>::Ok(())
})
.detach();
```

`smol::Timer` (`keyevent.rs` line 8, `copy.rs` line 283) is the only timer primitive used.
`promise::spawn::spawn` runs the async block on the `smol` executor (background thread pool).
`window.notify(TermWindowNotif::Apply(...))` posts a closure to the GUI main thread's
`TermWindowNotif` channel, where it is drained in `TermWindow::dispatch_window_event`.

Evidence:
- `wezterm-gui/src/termwindow/keyevent.rs` line 8: `use smol::Timer;`
- `wezterm-gui/src/termwindow/keyevent.rs` lines 261-265: `Timer::at(target).await` for leader
  key expiration.
- `wezterm-gui/src/overlay/copy.rs` lines 282-297: `smol::Timer::after(350ms)` for search
  debounce.

### The debounce pattern (cookie-based)

The search overlay in `copy.rs` demonstrates the production debounce idiom:

```rust
fn schedule_update_search(&mut self) {
    self.typing_cookie += 1;              // monotone counter
    let cookie = self.typing_cookie;

    promise::spawn::spawn(async move {
        smol::Timer::after(Duration::from_millis(350)).await;
        window.notify(TermWindowNotif::Apply(Box::new(move |tw| {
            // Check cookie: if the user typed again before the timer fired,
            // the cookie has incremented and this closure is a no-op.
            if cookie == current_cookie(tw, pane_id) {
                tw.do_actual_work();
            }
        })));
        anyhow::Result::<()>::Ok(())
    })
    .detach();
}
```

Each keystroke increments `typing_cookie` and fires a new `smol::Timer::after(350ms)` async task.
The closure checks whether the cookie it captured still matches the current value; if the user typed
again, the cookie has moved on and the closure is a no-op. Only the last-fired timer's closure
executes real work. This is a classic debounce via monotone cookie — no explicit cancellation of
prior futures is needed.

### The update checker pattern (blocking thread)

`wezterm-gui/src/update.rs` line 222 demonstrates the blocking-thread pattern for periodic
background I/O:

```rust
std::thread::Builder::new()
    .name("update_checker".into())
    .spawn(update_checker)  // plain blocking fn with thread::sleep inside
    .expect("...");
```

The `update_checker` function runs `std::thread::sleep` between checks. This is appropriate when
the work is synchronous (e.g., HTTP with `http_req` or `ureq`). Crucially, results are delivered
back to the GUI thread via `window.notify(TermWindowNotif::Apply(...))`, not by touching shared
state directly.

### Correct pattern for "debounce keystroke → async LLM query → render"

The complete flow for inline AI suggestion:

```
Keystroke received in TermWindow (GUI main thread)
  |
  v
Increment suggestion_cookie
  |
  v
promise::spawn::spawn(async {
    smol::Timer::after(Duration::from_millis(300)).await;
    if cookie != current_cookie { return Ok(()); }   // user typed again, abort
    |
    v
    let text = read_input_zone_text(pane);            // read from pane on smol thread
    let response = ureq::post(ollama_url)             // blocking HTTP call
        .send_json(...)
        .map(|r| r.into_json::<Suggestion>());
    |
    v
    window.notify(TermWindowNotif::Apply(Box::new(move |tw| {
        // Back on GUI main thread:
        if cookie != tw.suggestion_cookie { return; }
        tw.set_ghost_text(pane_id, response.text);   // stores suggestion
        window.invalidate();                          // triggers repaint
    })));
})
.detach();
```

Key points:
- `smol::Timer::after` provides the debounce delay with zero platform API calls.
- The blocking `ureq` call runs on the `smol` thread pool, not the GUI main thread.
- Cookie comparison on both sides ensures stale results are silently dropped.
- `window.invalidate()` triggers a repaint; the ghost text shim (Topic 2) renders on the next
  paint pass.
- 300ms debounce matches typical LLM latency budgets; the search overlay uses 350ms.

**Warning:** `ureq` (blocking HTTP) must not be called on a `smol` async context — it blocks the
executor thread. The correct approach is either: (a) call it synchronously inside a
`promise::spawn::spawn_into_new_thread` (a plain OS thread, not a smol task), or (b) wrap the
blocking call in `smol::unblock(|| ureq::...())` which offloads to a dedicated blocking thread
pool. The `smol::unblock` approach keeps the debounce await in a single async task and is
preferred.

```rust
promise::spawn::spawn(async move {
    smol::Timer::after(Duration::from_millis(300)).await;
    if cookie != current_cookie { return Ok(()); }
    let result = smol::unblock(move || {
        ureq::post(&url).send_json(payload)?.into_json::<Suggestion>()
    }).await;
    // ... notify GUI thread
})
.detach();
```

### Decisions

- **Timer mechanism:** `smol::Timer::after` inside `promise::spawn::spawn`. This is the one
  established pattern; no alternatives exist in the codebase.
- **Debounce idiom:** Cookie-based monotone counter (identical to `CopyOverlay::schedule_update_search`).
- **Blocking HTTP:** Wrapped in `smol::unblock` inside the same async task as the timer.

---

## Topic 4: Input Interception

### How WezTerm intercepts keys before sending to the shell

The key dispatch chain in `TermWindow` (`wezterm-gui/src/termwindow/keyevent.rs`) is:

```
key_event_impl / raw_key_event_impl
  |
  ├─ Pass 1: raw physical key, OnlyKeyBindings::Yes
  |     lookup_key -> InputMap -> KeyAssignment (if registered)
  |
  ├─ Pass 2: raw scan code, OnlyKeyBindings::Yes
  |     lookup_key -> InputMap -> KeyAssignment (if registered)
  |
  └─ Pass 3: final mapped key, OnlyKeyBindings::No
        |
        ├─ modal.key_down (if any modal active) — returns bool; if true, consumed
        ├─ lookup_key -> InputMap -> KeyAssignment (if registered)
        └─ encode and send to pane.writer() / pane.key_down()
```

`lookup_key` checks three locations in priority order (`keyevent.rs` lines 211-237):
1. The **overlay's** `key_table_state` (if the pane has an active overlay assigned via
   `assign_overlay_for_pane`).
2. The **window-level** `key_table_state` (the `KeyTableState` stack on `TermWindow`).
3. The **global** `InputMap` (built from config `key_bindings`).

A key is consumed if `perform_key_assignment` returns `PerformAssignmentResult::Handled`. Only
then is the event _not_ forwarded to the shell.

Before `lookup_key`, the **modal system** (`termwindow/modal.rs`) gets first refusal:
```rust
if let Some(modal) = self.get_modal() {
    if modal.key_down(term_key, raw_modifiers, self) == Ok(true) {
        return true; // consumed
    }
}
```

### How Tab-completion interception could work

Three viable approaches:

**Approach A: KeyAssignment in the overlay's key_table_state.**

When the ghost text shim is active (it is assigned as an overlay via `assign_overlay_for_pane`),
the overlay's `key_table_state` is checked first by `lookup_key`. Register a key table entry
mapping `Tab` -> `AcceptSuggestion` (a new `KeyAssignment` variant) inside that overlay state.
`AcceptSuggestion` is handled in `perform_key_assignment`, which writes the suggestion text to
`pane.writer()` and then cancels the overlay.

When no suggestion is visible, the overlay is not assigned, so `Tab` falls through to the pane
normally (shell handles it for native tab-completion).

**Approach B: KeyAssignment in the global key table, conditional on suggestion state.**

Add a global key binding for `Tab` that maps to `AcceptSuggestion`. Inside `perform_key_assignment`,
check whether a suggestion is currently active for the focused pane:
```rust
AcceptSuggestion => {
    if let Some(suggestion) = self.active_suggestion(pane.pane_id()) {
        pane.writer().write_all(suggestion.text.as_bytes())?;
        self.cancel_suggestion(pane.pane_id());
        return Ok(PerformAssignmentResult::Handled);
    }
    // No suggestion active: fall through (returns Unhandled, Tab goes to shell)
    return Ok(PerformAssignmentResult::Unhandled);
}
```
`PerformAssignmentResult::Unhandled` causes the dispatch chain to continue, so the shell receives
`Tab` normally. This requires no overlay at all — it uses state in `TermWindow` directly.

**Approach C: Pane-level `perform_assignment` override.**

Implement a custom `Pane` wrapper (the ghost text shim from Topic 2) that overrides
`perform_assignment`. When the shim is active and `Tab` is pressed:
```rust
fn perform_assignment(&self, assignment: &KeyAssignment) -> PerformAssignmentResult {
    if let KeyAssignment::SendKey(KeyCode::Tab) = assignment {
        if self.suggestion.lock().is_some() {
            // Accept the suggestion
            ...
            return PerformAssignmentResult::Handled;
        }
    }
    PerformAssignmentResult::Unhandled
}
```
The pane wrapper is already in the dispatch chain because `pane.perform_assignment` is called
inside `TermWindow::perform_key_assignment` (`mod.rs` line 2596).

### KeyAssignment resolution priority system

The priority order is (from highest to lowest):

| Priority | Check | Source |
|---|---|---|
| 1 | Active modal (`modal.key_down`) | `keyevent.rs` ~line 272-285 |
| 2 | Overlay key table state (if pane has overlay) | `keyevent.rs` lines 218-226 |
| 3 | Window key table state (leader / stacked tables) | `keyevent.rs` lines 228-233 |
| 4 | Global InputMap (config `key_bindings`) | `keyevent.rs` lines 234-236 |
| 5 | `pane.perform_assignment()` (via `perform_key_assignment`) | `mod.rs` lines 2596-2598 |
| 6 | Encode and send to shell | Fallback in `process_key` |

Evidence: `keyevent.rs` `lookup_key` function (lines 211-237) and `process_key` (lines 239-428).

### Decisions

**Selected approach: Approach B (global key table, conditional on suggestion state).**

Rationale:
- Approach A (overlay key table) requires the ghost text shim to be a full pane overlay assigned
  via `assign_overlay_for_pane`. This is architecturally sound (Pattern B from Topic 2 already
  requires this), and the overlay's key table will naturally intercept Tab when active. However,
  the overlay approach replaces the pane entirely during rendering, which complicates the
  transparent ghost-text requirement — the underlying pane must still be visible.
- Approach B is simpler to implement and does not require a pane overlay. It stores suggestion
  state in `TermWindow::pane_state` (or a dedicated `HashMap<PaneId, SuggestionState>` on
  `TermWindow`) and checks it inside `perform_key_assignment`. When no suggestion is active,
  `Unhandled` is returned and Tab goes to the shell unimpeded.
- Approach C requires the ghost text shim to implement `perform_assignment`, which is possible
  but means Tab interception is inside the pane wrapper (priority 5) — lower priority than the
  global InputMap. This means a user could accidentally override it with a `Tab` binding in their
  config. Less reliable.

**Revised recommendation (combining Topics 2 and 4):** Use Approach A plus the `with_lines_mut`
wrapping from Topic 2 together. The ghost text shim is assigned as a pane-level overlay
(`assign_overlay_for_pane`) that: (a) wraps `with_lines_mut` to inject ghost cells, and (b) has
a key table that maps Tab to `AcceptSuggestion`. This way both rendering and interception are
handled by the same overlay. When the suggestion expires or is dismissed, `cancel_overlay_for_pane`
removes both. This matches how `CopyOverlay` works: it is both a `Pane` (for rendering) and
registers in `key_table_state`.

**Concern:** Since the overlay replaces the pane for rendering, the ghost text shim must delegate
all `get_lines` / `with_lines_mut` calls to the real pane except for adding the ghost cells on the
cursor row. This is exactly what `CopyOverlay::with_lines_mut` does at lines 1405-1413.

**Tab-completion interoperability:** When the suggestion shim is active and Tab is pressed, the
shim's key table handles it. If the user dismisses the suggestion (Escape, or any non-Tab key),
`cancel_overlay_for_pane` removes the shim and Tab reverts to native shell behavior. There is no
permanent change to Tab's meaning.

---

## Comparison Matrix

| Criteria | OSC 133 Detection | Ghost Text Rendering | Debounce Pattern | Tab Interception |
|---|---|---|---|---|
| Existing mechanism? | Yes (fully implemented) | No (new capability) | Yes (`copy.rs` cookie debounce) | Partial (key table priority system exists) |
| Recommended approach | `get_semantic_zones()` + cursor pos | `with_lines_mut` shim overlay | `smol::Timer::after` + cookie | Pane-overlay key table (Approach A) |
| Fallback available? | Yes (cursor at last row heuristic) | N/A | N/A | Yes (Approach B: global InputMap) |
| Code to modify | None (read-only API) | `assign_overlay_for_pane` + new shim type | `promise::spawn` + `smol::unblock` | `config/src/keyassignment.rs` + `perform_key_assignment` |
| Shell cooperation required? | Yes (must emit OSC 133) | No | No | No |

---

## Detailed Analysis

### Topic 1: OSC 133 — Further Notes

`CommandStatus` (`133;D`) is parsed but **not stored** in `TerminalState`. The handler in
`performer.rs` line 913 is a no-op comment-only block. If the feature needs "don't suggest when
the last command failed," exit code tracking needs to be added:

```rust
OperatingSystemCommand::FinalTermSemanticPrompt(
    FinalTermSemanticPrompt::CommandStatus { status, .. }
) => {
    self.last_command_status = Some(status);  // new field on TerminalState
}
```

The `MarkEndOfPromptAndStartOfInputUntilEndOfLine` variant (`133;I`) sets
`clear_semantic_attribute_on_newline = true` (`terminalstate/mod.rs` line 262), meaning the `Input`
semantic type is cleared when the cursor moves to the next line. This is the fish-shell variant: it
only marks a single-line input zone. The standard bash/zsh variant uses `133;B`
(`MarkEndOfPromptAndStartOfInputUntilNextMarker`) which persists until `133;C`.

For the AI suggestion detector, both variants must be handled:
```rust
let at_input_zone = zones.iter().any(|z| {
    z.semantic_type == SemanticType::Input
        && z.start_y <= cursor_y
        && cursor_y <= z.end_y
});
```

### Topic 2: Ghost Text — Rendering Architecture

The `with_lines_mut` approach requires the ghost text shim to implement `Pane`. The minimum
required methods are:

- `with_lines_mut`: intercept and inject ghost cells on the cursor row.
- `pane_id`: return the underlying pane's ID (so the overlay renders at the correct position).
- All other `Pane` methods: delegate to the underlying pane.

The shim holds:
- `delegate: Arc<dyn Pane>` — the real pane.
- `suggestion: Arc<Mutex<Option<String>>>` — the current ghost text (written by the async task,
  read during `with_lines_mut`).
- `cookie: AtomicUsize` — for debounce coordination.

The `with_lines_mut` injection:
```rust
fn with_lines_mut(&self, lines: Range<StableRowIndex>, with_lines: &mut dyn WithPaneLines) {
    let suggestion = self.suggestion.lock().clone();
    self.delegate.with_lines_mut(lines, &mut GhostTextLines {
        with_lines,
        cursor_y: self.delegate.get_cursor_position().y,
        cursor_x: self.delegate.get_cursor_position().x,
        suggestion,
    });
}
```

Inside `GhostTextLines::with_lines_mut`:
```rust
for (idx, line) in lines.iter_mut().enumerate() {
    let stable_idx = idx as StableRowIndex + first_row;
    if stable_idx == cursor_y {
        if let Some(ref text) = suggestion {
            let dimmed = CellAttributes::default()
                .set_foreground(ColorAttribute::PaletteIndex(8))
                .set_italic(true)
                .clone();
            line.overlay_text_with_attribute(cursor_x, text, dimmed, SEQ_ZERO);
        }
    }
}
```

`Line::overlay_text_with_attribute` already exists (`copy.rs` line 1446 uses it). It writes text
into cells starting at a given column with given attributes, without changing the underlying cell's
actual content in the terminal model.

### Topic 3: Debounce — Confirmed Pattern

The exact debounce used by search (350ms, cookie-based) is the correct model. For AI suggestion,
300ms is a reasonable starting point given LLM network latency for local Ollama (typically 50-200ms
for small models). The user experiences: type character -> 300ms silence -> suggestion appears.

`smol::unblock` is documented in `smol` 2.0 and is safe to use with blocking code like `ureq`. It
runs the closure on a dedicated blocking thread pool (distinct from the smol async executor pool).
Evidence: `smol` 2.0 is the workspace version (`Cargo.toml` line 204); `smol::unblock` is
available since smol 1.2.

### Topic 4: Key Interception — Overlay Key Table Detail

`OverlayState` (`wezterm-gui/src/termwindow/mod.rs` lines 188-191) holds:
```rust
pub struct OverlayState {
    pub pane: Arc<dyn Pane>,
    pub key_table_state: KeyTableState,
}
```

After `assign_overlay_for_pane(pane_id, shim_pane)`, `lookup_key` checks
`overlay.key_table_state` first. Registering an entry `Tab -> AcceptSuggestion` in that state
ensures Tab is consumed by the suggestion system when the overlay is active.

To activate a named key table in the overlay state:
```rust
overlay.key_table_state.activate(KeyTableArgs {
    name: "ai_suggestion",
    timeout_milliseconds: None,
    replace_current: false,
    one_shot: false,
    until_unknown: false,
    prevent_fallback: false,
});
```

The `"ai_suggestion"` key table is defined in the `InputMap` with an entry:
```rust
Tab -> AcceptAiSuggestion
Escape -> DismissAiSuggestion
```

This makes Tab and Escape "AI-aware" only while the overlay is active, with zero impact on normal
usage.

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Shell does not emit OSC 133; semantic zones always empty | High | Med | Implement fallback heuristic (cursor at last line + foreground process is shell). Document that OSC 133 integration improves accuracy. |
| Ghost text cells overwrite visible terminal content if suggestion is longer than remaining line width | Med | Low | Cap ghost text at `dims.cols - cursor.x - 1` characters. Add trailing ellipsis if truncated. |
| `smol::unblock` blocks the blocking thread pool if many Ollama requests queue up | Low | Med | Maintain at most one in-flight LLM request per pane; cancel via cookie before starting a new one. |
| `with_lines_mut` lock contention: shim holds suggestion mutex while rendering; async task tries to write new suggestion during paint | Med | Low | Suggestion write uses `try_lock` and drops new results if lock is held; repaint will show the new value on the next frame. |
| Tab key intercept permanently breaks native shell tab-completion | Low | High | The overlay is only active when a suggestion is present. When dismissed (`cancel_overlay_for_pane`), Tab reverts to native behavior. Test with bash, zsh, fish. |
| Ghost text shim pane leaks if the real pane is closed | Med | Low | Subscribe to `MuxNotification::PaneRemoved(pane_id)`; when received, call `cancel_overlay_for_pane` for the shim. |
| LLM latency > 300ms causes suggestions to appear mid-word (stale) | High | Med | On each keystroke, check whether the returned suggestion still matches the current input zone text. If it is a prefix match, display it; otherwise discard. |
| `overlay_text_with_attribute` not available on all `Line` types | Low | High | Verify the method exists on `wezterm_surface::line::Line` (it is used by `CopyOverlay` at `copy.rs` line 1446, confirming availability). |

---

## Implementation Considerations

### Integration points with existing code

- **`config/src/keyassignment.rs`** — Add `AcceptAiSuggestion` and `DismissAiSuggestion` to the
  `KeyAssignment` enum. Follow the existing `#[derive(FromDynamic, ToDynamic)]` pattern.

- **`wezterm-gui/src/termwindow/mod.rs`** — Add `suggestion_state: HashMap<PaneId, SuggestionState>`
  to `TermWindow`. Handle `AcceptAiSuggestion` and `DismissAiSuggestion` in `perform_key_assignment`.
  `SuggestionState` holds the current ghost text string and the `typing_cookie`.

- **New file: `wezterm-gui/src/overlay/ai_suggestion.rs`** — Implement `AiSuggestionPane` (the
  ghost text shim). Implements `Pane`, delegating all methods to the real pane except
  `with_lines_mut` (which injects ghost text) and `perform_assignment` (which handles Tab/Escape
  as a belt-and-suspenders fallback).

- **`wezterm-gui/src/termwindow/keyevent.rs`** — In `key_event_impl`, after resolving the active
  pane but before the key dispatch, increment `suggestion_cookie` and fire the debounce async task.
  Condition: only fire if the cursor is in an `Input` semantic zone (or the fallback condition).

- **`arcterm-ai/src/suggestion.rs`** (new `arcterm-ai` crate) — LLM query logic. Reads the current
  input zone text, builds a prompt, calls Ollama via `ureq` + `smol::unblock`, returns a
  `SuggestionResult`.

### Migration path

This is entirely additive. No existing pane, renderer, or key handling is modified in a
breaking way. The feature is gated on whether a suggestion is active.

### Testing strategy

- **Unit test OSC 133 detection:** Feed a `Terminal` model the sequence
  `\x1b]133;A\x1b\\ $ \x1b]133;B\x1b\\cmd` and verify that `get_semantic_zones()` returns a zone
  with `SemanticType::Input` covering `cmd`. This exercises the real `TerminalState` code path.
- **Unit test ghost text rendering:** Construct an `AiSuggestionPane` wrapping a mock `Pane` with
  known line content. Call `with_lines_mut` on it and verify the returned cells at `cursor_x`
  onwards match the suggestion text with dimmed attributes.
- **Integration test Tab interception:** Instantiate a `TermWindow` in headless mode (used by
  existing tests), assign the suggestion shim overlay, synthesize a Tab `KeyEvent`, and verify
  `perform_key_assignment` returns `Handled` and the suggestion text is written to the pane writer.
- **Manual test:** Use fish or zsh with OSC 133 integration. Type a partial command and verify the
  ghost text appears after the debounce delay. Press Tab and verify the text is accepted. Press any
  other key and verify the ghost text disappears.

### Performance implications

- `get_semantic_zones()` is called at most once per keystroke (inside the async task after the
  debounce, not on every render). It acquires the pane's internal mutex briefly.
- The `with_lines_mut` injection runs on every repaint for the cursor row only. It clones one `Line`
  and calls `overlay_text_with_attribute` which is O(suggestion_length). For typical suggestion
  lengths (10-80 chars), this is well under 1ms.
- The async LLM task runs on `smol::unblock`'s blocking thread pool, completely off the GUI thread.
- Only one in-flight LLM request per pane. Cookie check before HTTP call avoids queueing.

---

## Sources

1. `/Users/lgbarn/Personal/arcterm/wezterm-escape-parser/src/osc.rs` — `FinalTermSemanticPrompt` enum, OSC 133 parsing, `OscCode::FinalTermSemanticPrompt = "133"`
2. `/Users/lgbarn/Personal/arcterm/term/src/terminalstate/performer.rs` lines 876-915 — OSC 133 handler (all variant dispatch)
3. `/Users/lgbarn/Personal/arcterm/term/src/terminalstate/mod.rs` lines 258-262, 968-973, 2700-2745 — `clear_semantic_attribute_on_newline`, `get_semantic_zones`
4. `/Users/lgbarn/Personal/arcterm/wezterm-cell/src/lib.rs` lines 178-212 — `SemanticType` enum, bitfield packing
5. `/Users/lgbarn/Personal/arcterm/term/src/lib.rs` lines 115-123 — `SemanticZone` struct
6. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/termwindow/render/screen_line.rs` lines 66-421 — cursor rendering, `cursor_range`, dead-key composing
7. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/termwindow/render/mod.rs` lines 122-211 — `RenderScreenLineParams`, `RenderScreenLineResult`, `ComputeCellFgBgParams`
8. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/overlay/copy.rs` lines 70-90, 275-297, 1386-1490 — `typing_cookie` debounce, `schedule_update_search`, `with_lines_mut` cell mutation
9. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/termwindow/keyevent.rs` lines 1-12, 98-175, 190-428, 599-651 — `lookup_key` priority, `process_key` dispatch, `smol::Timer` usage
10. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/termwindow/mod.rs` lines 117-207, 188-191, 343-348, 2583-2600 — `OverlayState`, `PaneState`, `TabState`, `perform_key_assignment` dispatch
11. `/Users/lgbarn/Personal/arcterm/mux/src/pane.rs` lines 31-36, 244-249 — `PerformAssignmentResult`, `Pane::perform_assignment` default
12. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/update.rs` lines 146-232 — blocking thread pattern for background I/O (`update_checker`, `start_update_checker`)
13. `/Users/lgbarn/Personal/arcterm/.shipyard/research/AI_INTEGRATION.md` — prior research: `ureq` decision, `TermWizTerminalPane` pattern, overlay lifecycle
14. `/Users/lgbarn/Personal/arcterm/.shipyard/codebase/ARCHITECTURE.md` — async runtime characterization, smol vs Tokio, `SpawnQueue`
15. `/Users/lgbarn/Personal/arcterm/.shipyard/codebase/STACK.md` — `smol 2.0` workspace version (line 114)

---

## Uncertainty Flags

- **`overlay_text_with_attribute` signature:** The method is called at `copy.rs` line 1446 but its
  full signature on the `Line` type in `wezterm-surface` was not directly read. Confirm that the
  column argument accepts `cursor.x` (a `usize`) and that it does not panic when
  `cursor.x + text.len() > line.len()` (ghost text extends beyond line width).

- **`AiSuggestionPane` assigned as overlay vs. real pane render:** When a pane-level overlay is
  active, the renderer calls `with_lines_mut` on the overlay pane, not the real pane. Confirm that
  `get_active_pane_or_overlay()` (`mod.rs`) returns the overlay pane in this case, and that the
  overlay's `with_lines_mut` delegation to the real pane does not cause a double-lock on the
  terminal model mutex.

- **`smol::unblock` availability:** `smol 2.0` includes `smol::unblock`. However, the workspace
  uses `smol = "2.0"` as a dependency — verify that the specific feature set compiled for
  `wezterm-gui` includes the blocking thread pool. The `smol` crate conditionally compiles
  `unblock` based on an internal feature; it is included by default in `smol 2.x`.

- **Exit code from `CommandStatus`:** The `CommandStatus` variant is parsed and the status integer
  is available (`osc.rs` lines 757-760), but `TerminalState` does not store it (handler is a
  no-op). If "suppress suggestion after error" is a requirement, this gap must be filled. No
  research was done on whether any downstream code reads exit codes from the terminal model.

- **Fish shell OSC 133 variant:** Fish emits `133;I` (until-end-of-line) rather than `133;B`.
  The `clear_semantic_attribute_on_newline` flag means multi-line input (e.g., after a `\` line
  continuation) will lose the `Input` tag on the second line. The suggestion feature should treat
  the absence of an `Input` zone as "not at prompt" rather than "prompt, zone missing."

- **Suggestion shim and `pane_id`:** If the shim returns the underlying pane's `pane_id()`, the
  overlay system's routing (which keys off `pane_id`) needs to correctly identify that the overlay
  belongs to this pane. Verify that `assign_overlay_for_pane(real_pane_id, shim)` followed by
  `pane_state(real_pane_id).overlay` works as expected even though the shim's `pane_id()` matches
  the real pane.
