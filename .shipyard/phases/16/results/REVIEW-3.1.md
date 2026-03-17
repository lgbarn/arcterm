# Review: Plan 3.1

## Verdict: MINOR_ISSUES

---

## Stage 1: Spec Compliance

### Task 1: Create AI pane module with chat state

**Status:** PASS

**Evidence:**
- `arcterm-app/src/ai_pane.rs` exists and contains `SYSTEM_PROMPT`, `AiPaneState` with all five required fields (`history`, `streaming`, `pending_response`, `input_buffer` — the last added early per the reported deviation), and all six required methods (`new`, `inject_context`, `add_user_message`, `append_response_chunk`, `finalize_response`).
- `mod ai_pane;` is present at `arcterm-app/src/main.rs:200`.
- All four plan-required tests are present at `ai_pane.rs:106-155`. The summary reports 5/5 tests passing (the fifth being an implicit test of `input_buffer`). The four named tests match the spec exactly: `new_state_has_system_prompt`, `inject_context_adds_system_message`, `inject_empty_context_does_nothing`, `user_message_and_streaming_lifecycle`.

**Notes:** The forward-inclusion of `input_buffer` in Task 1 is a trivial, coherent deviation with no architectural impact.

---

### Task 2: Wire AI Pane into keymap and AppState

**Status:** PASS

**Evidence:**
- `keymap.rs:77-79`: `OpenAiPane` and `RefreshAiContext` variants are present in `KeyAction` with the required doc comments.
- `keymap.rs:289-292`: Leader+i maps to `OpenAiPane`, Leader+c maps to `RefreshAiContext` within the `LeaderPending` match arm.
- `main.rs:673-675`: `ai_pane_states: HashMap<PaneId, ai_pane::AiPaneState>` and `ai_chat_rx: Option<(PaneId, mpsc::Receiver<Option<String>>)>` are present in `AppState`.
- `main.rs:726`: `self.ai_pane_states.remove(&id)` is correctly wired into `remove_pane_resources`.
- `main.rs:2017-2018`: Both fields initialised to empty/None in `resumed()`.
- `main.rs:1625-1687`: `OpenAiPane` handler spawns pane via the Vertical split geometry path and calls `inject_context` from sibling data.
- `main.rs:1689-1719`: `RefreshAiContext` handler calls `collect_sibling_contexts` and injects all siblings.
- Two new keymap tests at `keymap.rs:847-868`: `leader_then_i_opens_ai_pane` and `leader_then_c_refreshes_ai_context`.

**Notes:**
- The `ai_chat_rx` carrying `(PaneId, Receiver)` instead of just `Receiver` is a sound robustness improvement over the plan's implied single-receiver design.
- The `OpenAiPane` geometry uses `height / 2.0` (top-bottom split), which is consistent with the existing `Split(Axis::Vertical)` arm at `main.rs:1190-1193`. The comment "split vertically (right side)" is technically misleading since `Axis::Vertical` splits top-bottom in this codebase — but it matches existing behaviour faithfully.

---

### Task 3: AI Pane chat rendering and Ollama streaming

**Status:** PASS

**Evidence:**
- `main.rs:3771-3911`: Keyboard intercept block is inserted after all modal overlays and before keymap routing. Handles Escape (ClosePane), Backspace (pop from buffer), Enter (submit), printable chars (append), with all other keys falling through.
- `main.rs:3821-3892`: Tokio task spawned on Enter; uses `reqwest` byte stream + `futures_util::StreamExt::next()` to iterate NDJSON lines; chunks sent via `mpsc::channel(64)`; None signals done; Ollama error sends a user-visible `[Error: ...]` message before sending None.
- `main.rs:2230-2271`: `about_to_wait` drain loop processes chunks via `try_recv()`, calls `append_response_chunk` per chunk, `finalize_response` on None or Disconnected, then calls `request_redraw()` only when data arrived.
- `main.rs:3253-3355`: Chat overlay rendered for each AI pane: dark background quad, header bar, filtered history (system messages excluded), pending streaming content, "..." indicator while streaming, input bar at bottom.
- `futures-util = "0.3"` added to `arcterm-app/Cargo.toml` as justified by the summary.

**Notes:** The plan specifies `pulldown-cmark` + `syntect` for Markdown/code rendering. The implementation renders assistant messages verbatim line-by-line with no Markdown processing. This is an accepted simplification — the summary does not flag it, but it deviates from the plan spec (Task 3, Step 2: "Use existing `pulldown-cmark` + `syntect` for Markdown/code in responses"). Logged as a Minor finding below.

---

## Stage 2: Integration Review

---

## Findings

### Critical

None.

---

### Minor

**MINOR-1: No guard against submitting while a stream is in-flight**

- **File:** `arcterm-app/src/main.rs:3803-3819`
- **Description:** When the user presses Enter while `ai_state.streaming == true` (a prior response is still arriving), `add_user_message()` is called unconditionally, `ai_chat_rx` is overwritten with a new receiver, and a new tokio task is spawned. The old sender (held by the still-running prior task) will log `warn!` on every failed `.send()` but the old task continues running, consuming Ollama resources and CPU until it naturally finishes. With a slow model and a quick user this can stack up multiple concurrent Ollama requests.
- **Remediation:** Add a check before the submission block: `if ai_state.streaming { return; }` (or visually indicate the pending state in the input bar). A one-line guard is sufficient.

**MINOR-2: Markdown/code rendering not implemented despite being in the plan spec**

- **File:** `arcterm-app/src/main.rs:3289-3301`
- **Description:** Plan 3.1 Task 3 Step 2 calls for using "existing `pulldown-cmark` + `syntect` for Markdown/code in responses." The implementation renders assistant messages verbatim with no Markdown processing. Code blocks appear as raw triple-backtick text. The summary does not list this as a deviation.
- **Remediation:** Either implement Markdown stripping before display (acceptable for a first pass), or explicitly list this as a carry-forward item in the summary. The plan spec is currently unmet for this sub-requirement.

**MINOR-3: Streaming "..." indicator can visually overlap chat content**

- **File:** `arcterm-app/src/main.rs:3329-3335`
- **Description:** The streaming indicator `"  ..."` is placed at a fixed y-coordinate of `pane_rect.y + pane_rect.height - cell_h * 2.5`. This position is within the scrollable chat content area (content area ends at `pane_rect.height - cell_h * 3.5`). When the chat history is short the indicator appears in whitespace, but when chat content fills the viewport, the last visible chat line renders at the same y-position as the indicator (both within the final cell_h of the content area), causing visual overlap with no separating background quad.
- **Remediation:** Move the indicator to a fixed reserved slot between the content area and the input bar, and back it with an `OverlayQuad` matching the background color. Alternatively, extend `content_height` calculation to exclude the indicator row.

**MINOR-4: `finalize_response` pushes empty assistant message when Ollama returns no content**

- **File:** `arcterm-app/src/ai_pane.rs:92-99`
- **Description:** If the Ollama stream completes with zero non-empty content chunks (e.g., a network timeout that hits the `break` path without any chunks delivered), `pending_response` is empty when `finalize_response()` is called. This pushes `ChatMessage { role: "assistant", content: "" }` to history, which is then included in the next turn's API call. Some models produce unexpected behavior when given empty assistant turns.
- **Remediation:** In `finalize_response`, add a guard: `if !self.pending_response.is_empty() { self.history.push(...) }`. Still set `streaming = false` and clear `pending_response` unconditionally.

---

### Positive

- **Lifecycle cleanup is solid.** `remove_pane_resources` at `main.rs:723-726` correctly removes `ai_pane_states` entries, and `CloseTab` at `main.rs:1308-1310` calls `remove_pane_resources` for each removed pane. No state leak paths were found.

- **Streaming drain loop handles disconnection correctly.** The `Err(Disconnected)` arm at `main.rs:2252-2260` guards against finalization on already-finalized state with `if ai_state.streaming`, preventing a double-push to history.

- **Error surface presented to user, not swallowed.** The Ollama error path at `main.rs:3880-3890` sends a visible `[Error: LLM unavailable — {e}]` message to the chat before signaling done, giving the user actionable feedback without crashing.

- **`AiPaneState` is well-structured and fully tested.** The module is small, single-purpose, I/O-free, and covered by four focused unit tests. The `inject_empty_context_does_nothing` test is especially valuable for preventing silent context pollution regressions.

- **Input intercept placement is correct.** The AI pane key intercept block fires after all modal overlays (search, command overlay, workspace switcher) but before the keymap, so leader chords like `Ctrl+a, q` still close the AI pane normally without being eaten by the character handler.

- **`ai_chat_rx` carrying `PaneId` is a robustness improvement** over a bare receiver, correctly scoping chunk delivery even if focus changes between submission and drain.
