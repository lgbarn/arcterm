# Review: Plan 2.1 — Command Overlay

**Date:** 2026-03-17
**Reviewer:** review agent
**Branch:** main
**Commits reviewed:** b0e8ee4, 0a9746a, 579034c

---

## Stage 1: Spec Compliance

**Verdict:** PASS

### Task 1: CommandOverlay state machine

- **Status:** PASS
- **Evidence:** `arcterm-app/src/command_overlay.rs` exists (237 lines). All five specified types are present: `OverlayAction` (UpdateQuery, Submit, Accept(String), Close, Noop), `OverlayPhase` (Input, Loading, Result(String), Error(String)), `CommandOverlayState` with `query: String` and `phase: OverlayPhase`, `handle_key`, `set_result`, and `set_error`. The module is registered at `main.rs:199` as `mod command_overlay;`, placed immediately after `mod ollama;` as specified.
- **Notes:** 12 `#[test]` functions confirmed in the file, matching the plan's expected count. All phases of the state machine (Input → Loading on Submit, Loading → Result/Error via external call, Result/Error → Close on Escape) are covered.

### Task 2: Wire Command Overlay into keymap and AppState

- **Status:** PASS
- **Evidence:**
  - `keymap.rs:75` — `OpenCommandOverlay` variant added after `CrossPaneSearch`, with doc-comment.
  - `keymap.rs:220-222` — Ctrl+Space handler in Normal state fires `KeyAction::OpenCommandOverlay`.
  - `keymap.rs:515-519` — Test `ctrl_space_opens_command_overlay` verifies the binding.
  - `main.rs:666-668` — `command_overlay: Option<command_overlay::CommandOverlayState>` and `ollama_result_rx: Option<mpsc::Receiver<Result<String, String>>>` fields added to `AppState`.
  - `main.rs:1911-1912` — Both fields initialized to `None` in the constructor.
  - `main.rs:1410-1414` — `KeyAction::OpenCommandOverlay` dispatch opens overlay if not already open.
  - `main.rs:3370-3431` — Modal routing block: all five `OverlayAction` arms handled correctly; block ends with `return`.
  - `main.rs:2100-2119` — Ollama result drain in `about_to_wait`: `try_recv()`, clears `ollama_result_rx`, calls `set_result`/`set_error` on the overlay, then `request_redraw()`.
- **Notes:** The deviation noted in the summary (renaming `ctrl_space_opens_palette` to `ctrl_space_opens_command_overlay`) is correct — the test described the old behavior and was properly updated. `OpenPalette` variant is still present in `keymap.rs:65` for palette invocation from the menu; only the Ctrl+Space binding was redirected.

### Task 3: Render Command Overlay

- **Status:** PASS
- **Evidence:** `main.rs:3008-3040` — Rendering block active when `state.command_overlay.is_some()`. Background `OverlayQuad` at `[0.0, 0.0, win_w, bar_h]` with `[0.08, 0.09, 0.14, 0.95]` color. `bar_h = cell_h * 2.5` (~60px at default scale). Line 1: `"AI> {query}"`. Line 2: phase-dependent string matching the spec exactly (empty for Input, `"  ... waiting for LLM"` for Loading, `"  >> {cmd}  [Enter to accept · Esc to dismiss]"` for Result, `"  !! {msg}"` for Error). Both text lines are pushed to `palette_text` following the established pattern. The block is placed before the `search_overlay` block, consistent with the spec's instruction.

---

## Stage 2: Integration Review

### Critical

None.

### Minor

**MINOR-1: `ollama_result_rx` is orphaned when the overlay is closed during Loading**

- **File:** `arcterm-app/src/main.rs:3376-3379`
- **Description:** When `CmdAction::Close` is handled, `command_overlay` is set to `None` but `ollama_result_rx` is not cleared. If the user presses Escape while the LLM request is in-flight (Loading phase), the spawned tokio task continues running and will eventually send its result into the channel. The drain loop in `about_to_wait` (line 2103) will then attempt to call `set_result`/`set_error` on `state.command_overlay`, which is now `None`, so the result is silently discarded. This is harmless in isolation but `ollama_result_rx` remains `Some` until the task completes, holding the channel open unnecessarily. More importantly, if the user immediately opens a new overlay and submits a new query, `state.ollama_result_rx = Some(rx)` at line 3390 will overwrite the old receiver, dropping it. The old task's `tx.send()` will then observe a closed channel and the error is silently ignored (the task already discards send errors with `let _ =`). No hang or crash results, but the first-submitted orphaned task runs to completion without any benefit.
- **Remediation:** In the `CmdAction::Close` arm, add `state.ollama_result_rx = None;` before the `request_redraw()`. This drops the receiver immediately, causing the still-running task's `tx.send()` to fail fast (it already ignores the error), and cleanly terminates the channel.

**MINOR-2: `generate()` is called with `stream: false` but the response is parsed as `GenerateChunk` without checking HTTP status**

- **File:** `arcterm-app/src/main.rs:3396-3406`
- **Description:** The spawned task calls `client.generate()`, which returns `Ok(resp)` for any HTTP response including 4xx/5xx (reqwest only errors on network failures). If Ollama returns a `404` (model not found) or `500`, `resp.json::<GenerateChunk>()` will attempt to deserialize the error body as a `GenerateChunk`. Ollama's error responses are JSON objects like `{"error":"model not found"}`, which do not have a `response` field, causing a deserialization error. That error is caught and sent as `"parse error: ..."`, which is user-visible but misleading — the user sees "parse error" rather than "model not found".
- **Remediation:** Add an HTTP status check before parsing: `if !resp.status().is_success() { let status = resp.status(); let _ = tx.send(Err(format!("Ollama error: {status}"))).await; return; }`. This produces a clear error message for non-2xx responses.

**MINOR-3: Modal ordering — command overlay checked after palette and overlay_review, before search**

- **File:** `arcterm-app/src/main.rs:3370`
- **Description:** The modal priority chain is: workspace_switcher → palette → overlay_review → command_overlay → search. The spec (Task 2, Step 5) says to add the command overlay routing before the search overlay routing, which is satisfied. However, the placement after `overlay_review` is fine since both are independent modals. This is documented for awareness: if `overlay_review` and `command_overlay` are somehow both `Some` simultaneously (which should not happen in practice since no code path opens both), `overlay_review` would take priority. No code path opens both, so this is not a defect, but it is worth noting for future modal additions.
- **Remediation:** Consider adding an assertion or a debug-only check that at most one modal is open at a time.

**MINOR-4: `generate()` always uses `stream: false` but `GenerateChunk` has a `done` field that is never checked**

- **File:** `arcterm-app/src/main.rs:3398-3401` and `arcterm-app/src/ollama.rs:43-46`
- **Description:** With `stream: false`, Ollama returns a single JSON object with the full `response` and `done: true`. The code deserializes the chunk and uses `chunk.response` directly without checking `chunk.done`. For the non-streaming case this is always safe — Ollama guarantees `done: true` in the single-object response. However, if `stream` were ever accidentally set to `true`, the code would silently accept the first partial chunk. The `done` field on `GenerateChunk` currently serves no purpose.
- **Remediation:** Either assert `debug_assert!(chunk.done)` after deserialization, or add a `if !chunk.done` guard that sends an error. Low-priority but eliminates a silent assumption.

### Positive

- The state machine in `command_overlay.rs` is clean and well-isolated. All I/O is excluded; the module is pure logic with deterministic behavior, making the 12 tests comprehensive and fast.
- The `take()`-and-match pattern for modal routing (lines 3373-3430) correctly avoids double-borrow on `AppState` and is consistent with the established pattern used by `overlay_review` and `search_overlay`.
- The Ollama result drain uses `try_recv()` (non-blocking) in `about_to_wait`, which is the correct approach — it does not block the event loop waiting for LLM results.
- The `CmdAction::Close` arm in the Loading phase correctly leaves `command_overlay` as `None` (the overlay was already taken), so the drain loop's `if let Some(ref mut overlay) = state.command_overlay` guard at line 2111 silently no-ops when the result arrives after close. No panic, no stale render.
- Ctrl+Space binding conflict with the previous `OpenPalette` binding was handled explicitly and the related test was updated with a clear comment in the summary explaining the intentional change.
- The system prompt embedded at line 3395 is specific ("Return ONLY a single shell command with no explanation, no markdown, no backticks. Just the raw command."), which will reduce LLM formatting noise in practice.

---

## Verdict: MINOR_ISSUES

The spec is fully implemented and all acceptance criteria are met. Three minor integration issues are noted (orphaned channel on close-during-loading, missing HTTP status check, unused `done` field assertion), none of which block functionality. The most actionable is MINOR-1 (orphaned `ollama_result_rx` on Escape during Loading), which is a one-line fix.

---

## Issues Appended to .shipyard/ISSUES.md

See ISSUE-016-001 through ISSUE-016-003 below.
