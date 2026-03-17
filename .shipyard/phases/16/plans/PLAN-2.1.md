# Plan 2.1: Command Overlay — Quick-Invoke UI

> **Wave 2** — Depends on Plan 1.1 (Ollama client + config).

**Goal:** Build the `Ctrl+Space → type question → get command → Enter to accept` overlay.

## Task 1: Create CommandOverlay state machine

**Files:**
- Create: `arcterm-app/src/command_overlay.rs`
- Modify: `arcterm-app/src/main.rs` (add `mod command_overlay;`)
- Test: `arcterm-app/src/command_overlay.rs` (inline tests)

**Step 1: Create the module with types and tests**

Create `arcterm-app/src/command_overlay.rs` with the full state machine. See `docs/plans/2026-03-17-local-llm-implementation.md` Phase 2 Task 1 for the complete source code.

Key types:
- `OverlayAction` enum: UpdateQuery, Submit, Accept(String), Close, Noop
- `OverlayPhase` enum: Input, Loading, Result(String), Error(String)
- `CommandOverlayState` struct with `query: String` and `phase: OverlayPhase`
- `handle_key(&mut self, logical_key: &Key) -> OverlayAction`
- `set_result(&mut self, command: String)` and `set_error(&mut self, msg: String)`

**Step 2: Register the module**

Add `mod command_overlay;` to `arcterm-app/src/main.rs` after `mod ollama;`.

**Step 3: Run tests**

Run: `cargo test --package arcterm-app --lib command_overlay::tests`
Expected: ALL PASS (12 tests)

**Step 4: Commit**

```bash
git add arcterm-app/src/command_overlay.rs arcterm-app/src/main.rs
git commit -m "shipyard(phase-16): add CommandOverlay state machine with input/loading/result phases"
```

---

## Task 2: Wire Command Overlay into keymap and AppState

**Files:**
- Modify: `arcterm-app/src/keymap.rs:41-117` (add `OpenCommandOverlay` variant to KeyAction)
- Modify: `arcterm-app/src/keymap.rs:185+` (handle Ctrl+Space in Normal state)
- Modify: `arcterm-app/src/main.rs:540-676` (add `command_overlay` field to AppState)
- Modify: `arcterm-app/src/main.rs:1035+` (add KeyAction::OpenCommandOverlay dispatch)

**Step 1: Add `OpenCommandOverlay` to KeyAction enum**

In `arcterm-app/src/keymap.rs`, add after `CrossPaneSearch` (around line 73):
```rust
/// Open the command overlay (Ctrl+Space).
OpenCommandOverlay,
```

**Step 2: Handle Ctrl+Space in Normal state**

In `handle_logical_key_with_time`, inside `KeymapState::Normal`, add before the generic Ctrl handler:
```rust
Key::Named(NamedKey::Space) if ctrl => {
    return KeyAction::OpenCommandOverlay;
}
```

**Step 3: Add `command_overlay` field to AppState**

In `AppState` struct (main.rs ~line 642):
```rust
/// Command overlay state; `None` when the overlay is closed.
command_overlay: Option<command_overlay::CommandOverlayState>,
```
Initialize as `None` in the constructor.

**Step 4: Add dispatch for KeyAction::OpenCommandOverlay**

In `dispatch_action`:
```rust
KeyAction::OpenCommandOverlay => {
    if self.command_overlay.is_none() {
        self.command_overlay = Some(command_overlay::CommandOverlayState::new());
    }
    DispatchOutcome::Redraw
}
```

**Step 5: Route key events to overlay when open**

In the keyboard event handler, add early check: if `self.command_overlay.is_some()`, route key to `command_overlay.handle_key()` instead of keymap.
- `OverlayAction::Close` → set `self.command_overlay = None`
- `OverlayAction::Submit` → spawn tokio task calling `ollama.generate()`, send result back via channel
- `OverlayAction::Accept(cmd)` → write `cmd + "\n"` to active pane's PTY

**Step 6: Verify**

Run: `cargo build --package arcterm-app` (must compile)
Manual: `Ctrl+Space` opens overlay, Escape closes, typing works

**Step 7: Commit**

```bash
git add arcterm-app/src/keymap.rs arcterm-app/src/main.rs
git commit -m "shipyard(phase-16): wire Ctrl+Space command overlay into keymap and event loop"
```

---

## Task 3: Render Command Overlay

**Files:**
- Modify: `arcterm-app/src/main.rs` (build overlay quads in render path)
- Modify: `arcterm-render/src/lib.rs` (if needed — check existing OverlayQuad infra)

**Step 1: Build overlay quads in render snapshot**

In `about_to_wait` / render snapshot assembly, when `self.command_overlay.is_some()`, construct overlay quads:
- Semi-transparent dark background bar at top of window (full width, ~60px height)
- Query text rendered in white
- Phase indicator: "..." during Loading, command string during Result (green), error during Error (red)
- During Result phase: hint text "Enter to accept · Esc to dismiss"

Follow the same pattern used by `search_overlay` and `palette_mode` for rendering overlay quads.

**Step 2: Verify manually**

Run: `cargo run --package arcterm-app`
- `Ctrl+Space` → dark bar at top with cursor
- Type query → text appears
- Enter → shows "..." then command or error
- Escape → overlay disappears

**Step 3: Commit**

```bash
git add arcterm-app/src/main.rs arcterm-render/src/lib.rs
git commit -m "shipyard(phase-16): render command overlay bar with input, loading, and result states"
```
