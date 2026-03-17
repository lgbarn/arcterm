# Plan 3.1: AI Pane — Persistent Chat

> **Wave 3** — Depends on Plan 1.1 (Ollama client, config, context) and Plan 2.1 (overlay patterns).

**Goal:** Build the persistent AI chat pane opened via `Leader+i` with sibling context awareness and streamed Ollama responses.

## Task 1: Create AI pane module with chat state

**Files:**
- Create: `arcterm-app/src/ai_pane.rs`
- Modify: `arcterm-app/src/main.rs` (add `mod ai_pane;`)
- Test: `arcterm-app/src/ai_pane.rs` (inline tests)

**Step 1: Create the module with types and tests**

Create `arcterm-app/src/ai_pane.rs` with the full chat state. See `docs/plans/2026-03-17-local-llm-implementation.md` Phase 3 Task 1 for the complete source code.

Key types:
- `SYSTEM_PROMPT` const — terse terminal assistant prompt for DevOps
- `AiPaneState` struct: `history: Vec<ChatMessage>`, `streaming: bool`, `pending_response: String`
- `AiPaneState::new()` — initializes with system prompt
- `inject_context(cwd, last_cmd, exit_code, scrollback)` — adds sibling context as system message
- `add_user_message(content)` — appends user message, sets streaming=true
- `append_response_chunk(chunk)` — accumulates streamed response
- `finalize_response()` — commits response to history, sets streaming=false

**Step 2: Register the module**

Add `mod ai_pane;` to `arcterm-app/src/main.rs`.

**Step 3: Run tests**

Run: `cargo test --package arcterm-app --lib ai_pane::tests`
Expected: ALL PASS (4 tests: new_state_has_system_prompt, inject_context_adds_system_message, inject_empty_context_does_nothing, user_message_and_streaming_lifecycle)

**Step 4: Commit**

```bash
git add arcterm-app/src/ai_pane.rs arcterm-app/src/main.rs
git commit -m "shipyard(phase-16): add AiPaneState with chat history, context injection, and streaming"
```

---

## Task 2: Wire AI Pane into keymap and AppState (Leader+i)

**Files:**
- Modify: `arcterm-app/src/keymap.rs` (add `OpenAiPane` and `RefreshAiContext` to KeyAction)
- Modify: `arcterm-app/src/main.rs` (add `ai_pane_states` HashMap, dispatch handlers)

**Step 1: Add KeyAction variants**

In `keymap.rs` KeyAction enum, add:
```rust
/// Open a new AI chat pane (Leader+i).
OpenAiPane,
/// Refresh sibling context in the active AI pane (Leader+c).
RefreshAiContext,
```

**Step 2: Handle in LeaderPending state**

In `handle_logical_key_with_time`, `KeymapState::LeaderPending` arm:
```rust
Key::Character(s) if s.as_str() == "i" => {
    self.state = KeymapState::Normal;
    return KeyAction::OpenAiPane;
}
Key::Character(s) if s.as_str() == "c" => {
    self.state = KeymapState::Normal;
    return KeyAction::RefreshAiContext;
}
```

**Step 3: Add AI pane tracking to AppState**

```rust
/// Per-pane AI chat state; only populated for AI panes.
ai_pane_states: HashMap<PaneId, ai_pane::AiPaneState>,
```

**Step 4: Dispatch OpenAiPane**

Reuse existing `Split(Axis::Vertical)` logic to create the pane, then track the new pane ID in `ai_pane_states` with `AiPaneState::new()`. Auto-inject sibling context from the previously-focused pane.

**Step 5: Dispatch RefreshAiContext**

If focused pane is in `ai_pane_states`, collect sibling context and call `inject_context()`.

**Step 6: Verify**

Run: `cargo build --package arcterm-app` (must compile)
Manual: `Ctrl+a, i` opens AI pane; `Ctrl+a, c` refreshes context (visible in logs)

**Step 7: Commit**

```bash
git add arcterm-app/src/keymap.rs arcterm-app/src/main.rs
git commit -m "shipyard(phase-16): wire Leader+i to open AI pane, Leader+c to refresh context"
```

---

## Task 3: AI Pane chat rendering and Ollama streaming

**Files:**
- Modify: `arcterm-app/src/main.rs` (intercept input in AI panes, stream responses)
- Modify: `arcterm-app/src/ai_pane.rs` (add input buffer and render helpers)

**Step 1: Intercept keyboard input in AI panes**

In keyboard handler, check if focused pane is in `ai_pane_states`:
- Printable chars + Backspace → modify input buffer in AiPaneState
- Enter → call `add_user_message(input)`, spawn tokio task:
  - `ollama_client.chat(ai_state.history.clone()).await`
  - Stream chunks via tokio channel → `append_response_chunk()` + request redraw
  - On done → `finalize_response()`
- Escape → close AI pane or switch focus

**Step 2: Render chat in AI pane**

For panes in `ai_pane_states`, override the terminal grid rendering:
- Render chat history as styled blocks (user messages prefixed `> `, assistant left-aligned)
- Use existing `pulldown-cmark` + `syntect` for Markdown/code in responses
- Show "..." typing indicator while streaming
- Show input line at bottom with current user text

**Step 3: Auto-inject context on open**

When `OpenAiPane` creates the pane, immediately call `inject_context()` with sibling pane's CWD, last command, exit code, and last 30 scrollback lines.

**Step 4: Verify manually**

Run: `cargo run --package arcterm-app` (with Ollama running)
- `Ctrl+a, i` → AI pane opens with context
- Type question + Enter → response streams in with syntax highlighting
- `Ctrl+a, c` → refreshes context

**Step 5: Commit**

```bash
git add arcterm-app/src/main.rs arcterm-app/src/ai_pane.rs
git commit -m "shipyard(phase-16): AI pane chat rendering with Ollama streaming and context injection"
```
