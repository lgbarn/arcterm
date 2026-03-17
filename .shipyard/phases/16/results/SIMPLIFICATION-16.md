# Simplification Report

**Phase:** 16 — Local LLM Integration
**Date:** 2026-03-17
**Files analyzed:** 8 (ollama.rs, command_overlay.rs, ai_pane.rs, main.rs, config.rs, context.rs, keymap.rs, terminal.rs)
**Findings:** 2 high, 3 medium, 4 low

---

## High Priority

### Duplicated OllamaClient construction pattern

- **Type:** Consolidate
- **Locations:** `arcterm-app/src/main.rs:3645-3655`, `arcterm-app/src/main.rs:3821-3829`
- **Description:** Two `tokio::spawn` blocks independently clone `state.config.ai.endpoint` and `state.config.ai.model`, then construct `OllamaClient::new(endpoint, model)` inside the async block with a local `use crate::ollama::OllamaClient;` import statement. The setup is structurally identical; only the subsequent API call differs (`generate()` for the command overlay vs `chat()` for the AI pane).
- **Suggestion:** Add a helper method to `AppState` — `fn ollama_client(&self) -> ollama::OllamaClient` — that clones endpoint and model from config and returns a constructed client. Replace both spawn sites with `let client = state.ollama_client();` moved into the closure. The `use crate::ollama::OllamaClient;` local imports inside each closure also disappear.
- **Impact:** ~8 lines removed, single location to change if config fields are ever renamed or client construction gains new parameters.

### `is_some().unwrap()` double-access on `ollama_result_rx`

- **Type:** Refactor
- **Locations:** `arcterm-app/src/main.rs:2209-2225`
- **Description:** The drain block checks `if state.ollama_result_rx.is_some()` on line 2209, then immediately calls `.as_mut().unwrap()` on line 2212-2213. This is a textbook `.is_some()` + `.unwrap()` pattern that bypasses the borrow checker's help and obscures intent. In Rust the idiomatic replacement is `if let Some(rx) = state.ollama_result_rx.as_mut()`.
- **Suggestion:** Replace the outer `if state.ollama_result_rx.is_some() { if let Ok(result) = state.ollama_result_rx.as_mut().unwrap().try_recv() {` with `if let Some(rx) = state.ollama_result_rx.as_mut() { if let Ok(result) = rx.try_recv() {`. The `state.ollama_result_rx = None;` inside the block remains valid because it is reached after `rx` is no longer live.
- **Impact:** ~3 lines simplified, removes a logical redundancy, aligns with the idiomatic pattern already used for the AI chat drain at lines 2233+.

---

## Medium Priority

### `OpenAiPane` duplicates the `Split(Axis::Vertical)` geometry block verbatim

- **Type:** Consolidate
- **Locations:** `arcterm-app/src/main.rs:1175-1208` (`KeyAction::Split`), `arcterm-app/src/main.rs:1625-1654` (`KeyAction::OpenAiPane`)
- **Description:** `OpenAiPane` implements the same five-step geometry pattern as `Split(Axis::Vertical)`: compute `focused_rect`, halve the height into `new_rect`, call `grid_size_for_rect`, call `spawn_pane`, resize the existing pane, and insert into the layout tree with `Axis::Vertical`. SUMMARY-3.1 acknowledges this explicitly: "Reuses the exact geometry logic from `KeyAction::Split(Axis::Vertical)`." The comment documents duplication rather than eliminating it.
- **Suggestion:** Extract a private `fn split_pane_vertical(&mut self) -> (PaneId, PaneId)` that returns `(sibling_id_before_split, new_pane_id)`. Both `Split(Axis::Vertical)` and `OpenAiPane` call it, then `OpenAiPane` proceeds with context injection on the returned IDs. This is only two callers today (Rule of Three threshold not met), but the comment signaling intentional reuse and the likelihood of a third AI-adjacent action in a later phase make extraction worthwhile now.
- **Impact:** ~25 lines removed from the match arm, clearer separation of "create split" from "configure as AI pane".

### `last_ai_pane` cleanup is repeated in four separate close paths instead of being centralized in `remove_pane_resources`

- **Type:** Consolidate
- **Locations:** `arcterm-app/src/main.rs:1224-1225`, `arcterm-app/src/main.rs:1242-1243`, `arcterm-app/src/main.rs:1311-1312`, `arcterm-app/src/main.rs:2525-2526`
- **Description:** Every pane-removal site has the same two-line pattern:
  ```
  if self.last_ai_pane == Some(lid) {
      self.last_ai_pane = None;
  }
  ```
  This appears four times across `ClosePane` (single pane path), `ClosePane` (last pane in tab path), `CloseTab`, and the `about_to_wait` closed-panes drain. `remove_pane_resources` already centralizes cleanup for `panes`, `ai_states`, `pane_contexts`, `ai_pane_states`, and five other maps. The `last_ai_pane` guard belongs there too.
- **Suggestion:** Move the check into `remove_pane_resources(id)`:
  ```rust
  if self.last_ai_pane == Some(id) {
      self.last_ai_pane = None;
  }
  ```
  Remove the four scattered copies. `remove_pane_resources` receives the exact `id` needed at all call sites.
- **Impact:** 8 lines removed, eliminates the risk of forgetting the guard in a future fifth close path.

### Inline system prompt string for the command overlay should be a named constant

- **Type:** Refactor
- **Locations:** `arcterm-app/src/main.rs:3656`
- **Description:** The AI pane module defines its system prompt as a named public constant `ai_pane::SYSTEM_PROMPT` (ai_pane.rs:10). The command overlay's system prompt is an anonymous inline string literal passed directly inside a `tokio::spawn` closure. Both are LLM system prompts that encode behavioral contracts with the model; both deserve the same treatment for discoverability and testability.
- **Suggestion:** Define `const GENERATE_SYSTEM_PROMPT: &str = "You are a shell command assistant. ...";` at the top of `command_overlay.rs` alongside the other module-level definitions, and reference it at the spawn site. This also makes the prompt editable without navigating into a deeply nested closure inside `main.rs`.
- **Impact:** Trivial line count change; meaningful discoverability and consistency gain. Aligns with the pattern established in `ai_pane.rs`.

---

## Low Priority

- **Redundant `let lid = id;` aliases** — `arcterm-app/src/main.rs:1222` and `arcterm-app/src/main.rs:1309`: Two separate loop bodies bind `let lid = id;` and immediately use `lid` exactly as `id` would be used. These are vestigial artifacts from an earlier refactor. Replace `lid` with `id` at both sites and remove the alias lines.

- **`new_size` computed twice via `grid_size_for_rect(new_rect)`** — `arcterm-app/src/main.rs:1195-1198` and `arcterm-app/src/main.rs:1643-1646`: Both `Split(Axis::Vertical)` and `OpenAiPane` call `self.grid_size_for_rect(new_rect)` into `new_size`, then immediately call it again into `orig_size`. The two calls are identical (same `new_rect` argument) because `orig_size` was meant to be the size of the *existing* pane's remaining half, not the new pane. This appears to be a pre-existing copy-paste bug carried into `OpenAiPane`. The fix is `let orig_size = new_size;`, which also clarifies the intent. This is low priority because the numeric values happen to be the same.

- **`use crate::ollama::OllamaClient;` local imports inside spawn closures** — `arcterm-app/src/main.rs:3654`, `arcterm-app/src/main.rs:3827`: Both Ollama spawn sites bring `OllamaClient` into scope with a local `use` statement inside the closure body rather than at the module level. Since `OllamaClient` is used in two places already (and the high-priority finding above would add a third use in a helper method), the import belongs at the top of `main.rs` with the other `use` declarations.

- **`ai_pane_states.contains_key` check followed immediately by `ai_pane_states.get_mut`** — `arcterm-app/src/main.rs:3778` and `arcterm-app/src/main.rs:3788-3791`: The AI pane input intercept block first checks `contains_key(&focused_id)` to enter the branch, then uses individual `get_mut` calls for each key variant. The outer check is fine for branching, but the inner `get_mut` lookups each pay the hash cost again. This is a micro-optimization flag only — at one pane lookup per keypress, the cost is negligible.

---

## Summary

- **Duplication found:** 3 instances across 2 files (OllamaClient construction ×2, pane-close guard ×4, split geometry ×2)
- **Dead code found:** 0 unused definitions
- **Complexity hotspots:** 3 functions exceeding thresholds (`dispatch_action` at 685 lines, `about_to_wait` at 588 lines, `window_event` at 1317 lines) — these are pre-existing; Phase 16 added to them but did not create them
- **AI bloat patterns:** 1 instance (`is_some().unwrap()` double-access)
- **Estimated cleanup impact:** ~40 lines removable, 2 abstractions extractable (helper method + named constant)

---

## Recommendation

Simplification is recommended before Phase 17 begins, but none of the findings are blockers. The two high-priority items are mechanical one-for-one substitutions with no behavior change and can be done in under 30 minutes. The medium-priority `last_ai_pane` consolidation into `remove_pane_resources` is the highest-value change because it closes a real gap: a future pane-close path (e.g., closing a tab via mouse) could easily miss the guard if it is not in `remove_pane_resources`.

The three large functions (`dispatch_action`, `about_to_wait`, `window_event`) exceed every complexity threshold by a wide margin, but they predate Phase 16 and are out of scope for this review. They represent accumulated technical debt that should be addressed in a dedicated refactor phase.
