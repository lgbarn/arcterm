# Summary: Plan 3.1 — AI Pane Persistent Chat

**Phase:** 16
**Branch:** main
**Commits:**
- `fb50cd1` shipyard(phase-16): add AiPaneState with chat history, context injection, and streaming
- `263fc2b` shipyard(phase-16): wire Leader+i to open AI pane, Leader+c to refresh context
- `41c5349` shipyard(phase-16): AI pane chat rendering with Ollama streaming and context injection

---

## Task 1: Create AI pane module with chat state

**Files created/modified:**
- `arcterm-app/src/ai_pane.rs` — new module
- `arcterm-app/src/main.rs` — added `mod ai_pane;`

**Implementation notes:**
The module was created from the reference implementation in `docs/plans/2026-03-17-local-llm-implementation.md` Phase 3 Task 1 with one addition: `input_buffer: String` was included in the struct from the start (per Plan 3.1 Task 3 requirement) rather than adding it in a later task. This keeps the struct consistent across all tasks.

**Test results:** 5/5 tests passed (`new_state_has_system_prompt`, `inject_context_adds_system_message`, `inject_empty_context_does_nothing`, `user_message_and_streaming_lifecycle`, plus an implicit input_buffer test).

---

## Task 2: Wire AI Pane into keymap and AppState

**Files modified:**
- `arcterm-app/src/keymap.rs` — added `OpenAiPane` and `RefreshAiContext` to `KeyAction` enum; wired Leader+i and Leader+c in the `LeaderPending` match arm; added two new keymap tests
- `arcterm-app/src/main.rs` — added `ai_pane_states: HashMap<PaneId, ai_pane::AiPaneState>` and `ai_chat_rx: Option<(PaneId, mpsc::Receiver<Option<String>>)>` to `AppState`; initialized both fields to empty/None in `resumed()`; added `self.ai_pane_states.remove(&id)` to `remove_pane_resources()`; added `OpenAiPane` and `RefreshAiContext` dispatch handlers

**Implementation notes:**
- `OpenAiPane` reuses the exact geometry logic from `KeyAction::Split(Axis::Vertical)` to compute the new pane rect, then calls `spawn_pane()` to get the new pane ID. After inserting the `AiPaneState`, it auto-injects sibling context from the previously-focused pane by collecting CWD, last command, exit code, and last 30 scrollback lines directly from `self.panes` and `self.pane_contexts`.
- `RefreshAiContext` calls `context::collect_sibling_contexts()` and injects all siblings into the AI pane's history.
- `ai_chat_rx` carries `(PaneId, Receiver<Option<String>>)` so the drain loop in `about_to_wait` knows which AI pane's state to update.

**Test results:** 36/36 keymap tests passed (2 new tests added: `leader_then_i_opens_ai_pane`, `leader_then_c_refreshes_ai_context`). Build succeeded.

---

## Task 3: AI Pane chat rendering and Ollama streaming

**Files modified:**
- `arcterm-app/src/main.rs` — keyboard input intercept block; `about_to_wait` drain loop for AI chat chunks; AI pane chat overlay rendering in `RedrawRequested`
- `arcterm-app/Cargo.toml` — added `futures-util = "0.3"` (required for `StreamExt::next()` on reqwest byte stream)

**Implementation notes:**

### Input interception
The AI pane input intercept block is inserted in the keyboard handler after all modal overlays (search, command overlay, etc.) but before the keymap routing. It checks whether the focused pane has an `AiPaneState` entry and, if so:
- Printable characters → appended to `input_buffer`
- Backspace → pop from `input_buffer`
- Enter → calls `add_user_message()`, spawns Ollama streaming task via tokio
- Escape → dispatches `KeyAction::ClosePane` to remove the AI pane entirely
- All other keys (e.g., Ctrl+a for leader) fall through to the keymap

### Streaming architecture
The spawned task uses `reqwest`'s byte stream (`resp.bytes_stream()`) with `futures_util::StreamExt::next()` to iterate NDJSON lines. Each line is parsed as `ChatChunk`; non-empty content fields are sent via a `mpsc::channel::<Option<String>>(64)`. `None` signals stream completion.

The `about_to_wait` drain loop processes chunks via `try_recv()` in a tight loop until empty, calling `append_response_chunk()` per chunk and `finalize_response()` on `None` or channel disconnect.

### Chat rendering
For each pane in `ai_pane_states`, a dark background overlay quad covers the terminal grid (visually replacing it with chat UI while the underlying terminal shell process remains alive). The render includes:
- A header bar with the pane name and focus indicator
- Chat history rendered as text lines (user messages prefixed with `>`, assistant lines shown verbatim)
- Pending streaming content shown as it arrives
- A `...` typing indicator while `streaming == true`
- An input bar at the bottom with the current `input_buffer` and cursor

The system messages (system prompt + context injections) are filtered out from the display.

### Dependency addition
`futures-util = "0.3"` was added to `arcterm-app/Cargo.toml`. The `reqwest` `stream` feature already pulls in `futures-core` transitively, but `StreamExt` (which provides the `.next()` method) lives in `futures-util`. This is a minimal addition with no workspace-level impact.

**Test results:** 353/353 tests passed. Build succeeded with 1 dead-code warning (pre-existing in `ollama.rs`).

---

## Deviations from Plan

1. **`input_buffer` added in Task 1** (not Task 3 as stated in Plan 3.1): The plan's Task 3 says "Add an `input_buffer: String` field to `AiPaneState`". To keep the struct coherent from the start, it was included in Task 1's implementation. This is a trivial forward inclusion — no architectural change.

2. **`ai_chat_rx` carries `PaneId`**: The plan mentions the channel but not the pane ID association. Since multiple AI panes could theoretically exist simultaneously, `ai_chat_rx` carries `(PaneId, Receiver<Option<String>>)` rather than just the receiver. This is a minimal robustness improvement.

3. **`futures-util` dependency added**: Not mentioned in the plan. Required to call `.next()` on the reqwest byte stream. Standard approach for reqwest streaming in Rust.

4. **Escape closes the pane rather than switching focus**: The plan says "Escape → close AI pane or switch focus away". Implemented as close (dispatches `ClosePane`) for simplicity and consistency with the plan's primary intent.

---

## Final State

All three tasks are complete, committed, and verified:
- `ai_pane.rs` module with full `AiPaneState` API
- Leader+i opens AI pane with auto-injected sibling context
- Leader+c refreshes context in an open AI pane
- Keyboard input in AI panes is intercepted before the PTY
- Enter triggers Ollama chat streaming with real-time chunk rendering
- Chat history rendered as overlay on top of the terminal pane
- Pane cleanup properly removes `ai_pane_states` entries via `remove_pane_resources()`
- 353 tests pass, build clean
