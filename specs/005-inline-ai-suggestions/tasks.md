---
description: "Task list for Inline AI Command Suggestions"
---

# Tasks: Inline AI Command Suggestions

**Input**: Design documents from `/specs/005-inline-ai-suggestions/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, quickstart.md

**Organization**: Tasks grouped by user story for independent implementation.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup

**Purpose**: Create the suggestion module and overlay file.

- [x] T001 Create `arcterm-ai/src/suggestions.rs` with module declaration — add `pub mod suggestions;` to `arcterm-ai/src/lib.rs`
- [x] T002 Create `wezterm-gui/src/suggestion_overlay.rs` with module declaration — add `pub mod suggestion_overlay;` to `wezterm-gui/src/main.rs`
- [x] T003 Verify both crates compile with `cargo check --package arcterm-ai --package wezterm-gui`

**Checkpoint**: New files exist, crates compile.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Prompt detection and debounce infrastructure that all stories need.

- [x] T004 Implement prompt detection in `arcterm-ai/src/suggestions.rs` — `pub fn is_at_shell_prompt(pane: &dyn Pane) -> bool` that checks `pane.get_semantic_zones()` for `SemanticType::Input` at cursor position; fall back to heuristic (cursor on last row + foreground process is a shell) when no OSC 133 zones exist
- [x] T005 Implement suggestion query builder in `arcterm-ai/src/suggestions.rs` — `pub fn build_suggestion_query(partial_cmd: &str, context: &PaneContext) -> Vec<Message>` that creates a messages array with the completion system prompt + formatted context + partial command
- [x] T006 Implement response cleaner in `arcterm-ai/src/suggestions.rs` — `pub fn clean_suggestion(response: &str, partial_cmd: &str) -> String` that strips backticks, markdown, leading whitespace, and ensures the response is a valid command continuation (not a repeat of what was already typed)
- [x] T007 Implement `SuggestionConfig` struct in `arcterm-ai/src/suggestions.rs` — fields for `enabled`, `debounce_ms`, `accept_key`, `context_lines` with defaults
- [x] T008 Write unit tests for prompt detection (mock semantic zones), query building, and response cleaning
- [x] T009 Verify `cargo test --package arcterm-ai` passes

**Checkpoint**: Prompt detection, query building, and response cleaning work with tests.

---

## Phase 3: User Story 1 — Ghost Text Suggestion (Priority: P1)

**Goal**: Dimmed suggestion text appears after cursor when user pauses typing.

**Independent Test**: Type `git` at prompt, pause 500ms, see dimmed completion text.

- [x] T010 [US1] Implement ghost text overlay pane wrapper in `wezterm-gui/src/suggestion_overlay.rs` — struct `SuggestionOverlay` that wraps a `Arc<dyn Pane>`, stores current suggestion text, and overrides `with_lines_mut` to inject dimmed text (`ColorAttribute::PaletteIndex(8)`) at the cursor column on the cursor row
- [x] T011 [US1] Implement debounce timer in `wezterm-gui/src/suggestion_overlay.rs` — use `smol::Timer::after(Duration::from_millis(debounce_ms))` with a `typing_cookie: Arc<AtomicU64>` counter; each keystroke increments the cookie; the async task checks the cookie after the timer fires
- [x] T012 [US1] Implement async LLM query in `wezterm-gui/src/suggestion_overlay.rs` — on debounce fire: extract partial command from cursor line, call `build_suggestion_query()`, wrap `backend.generate()` in `smol::unblock()`, deliver result via `window.notify(TermWindowNotif::Apply(...))`
- [x] T013 [US1] Wire suggestion display — on receiving the LLM result, call `clean_suggestion()`, store in the overlay, and call `assign_overlay_for_pane()` to activate the ghost text rendering
- [x] T014 [US1] Handle stale responses — compare the `generation_id` of the response with the current `typing_cookie`; discard if they don't match (input changed since query was sent)
- [x] T015 [US1] Handle LLM unavailable — if `backend.is_available()` returns false or `generate()` fails, silently do nothing (no error, no UI change)
- [x] T016 [US1] Verify ghost text renders correctly — manual test per quickstart.md

**Checkpoint**: Ghost text appears after typing pause. Dimmed. Disappears on new input.

---

## Phase 4: User Story 2 — Tab Accept (Priority: P1)

**Goal**: Tab inserts suggestion text into the shell command line.

**Independent Test**: See suggestion, press Tab, text is inserted.

- [ ] T017 [US2] Register `ai_suggestion` key table in `wezterm-gui/src/suggestion_overlay.rs` — when suggestion overlay is active, push a key table with `Tab -> AcceptAiSuggestion` binding
- [ ] T018 [US2] Implement accept action — on `AcceptAiSuggestion`: call `pane.send_text(&suggestion.text)` to inject the completion into the shell, then `cancel_overlay_for_pane()` to remove ghost text
- [ ] T019 [US2] Ensure Tab passthrough — when no suggestion overlay is active (no ghost text visible), Tab falls through to normal shell completion at key dispatch priority 6
- [ ] T020 [US2] Verify Tab accept works and coexists with shell completion — manual test

**Checkpoint**: Tab accepts when suggestion visible, shell completion works when not.

---

## Phase 5: User Story 3 — Dismiss Suggestion (Priority: P2)

**Goal**: Escape dismisses, continued typing refines.

**Independent Test**: See suggestion, press Escape, ghost text gone.

- [ ] T021 [US3] Add `Escape -> DismissAiSuggestion` to the suggestion key table in `wezterm-gui/src/suggestion_overlay.rs`
- [ ] T022 [US3] Implement dismiss action — on `DismissAiSuggestion` or any new keystroke: call `cancel_overlay_for_pane()`, reset suggestion state to Idle, increment `typing_cookie` to invalidate pending queries
- [ ] T023 [US3] Ensure Enter executes only typed text — when suggestion is visible and user presses Enter, dismiss the suggestion first, then let Enter pass through to the shell
- [ ] T024 [US3] Ensure arrow keys dismiss — cursor movement (Left, Right, Home, End, Up, Down) dismisses the suggestion

**Checkpoint**: All dismiss paths work cleanly.

---

## Phase 6: User Story 4 — Context-Aware Suggestions (Priority: P2)

**Goal**: Suggestions reflect terminal context (errors, CWD, recent output).

**Independent Test**: Run failing command, type next command, see context-aware suggestion.

- [ ] T025 [US4] Wire PaneContext into suggestion queries in `wezterm-gui/src/suggestion_overlay.rs` — on debounce fire, call `arcterm_ai::context::PaneContext` extraction on the active pane with `config.context_lines` (default 10)
- [ ] T026 [US4] Include exit code in context — if OSC 133;D is handled (or via `get_semantic_zones` last `Output` zone), include the last command's exit status in the query
- [ ] T027 [US4] Suppress suggestions inside non-shell programs — check `is_at_shell_prompt()` before querying; if false (vim, htop, ssh running), skip the query entirely

**Checkpoint**: Suggestions are context-aware and suppressed in non-shell programs.

---

## Phase 7: User Story 5 — Configuration (Priority: P3)

**Goal**: Users can customize suggestion behavior.

**Independent Test**: Disable suggestions in config, verify they stop.

- [ ] T028 [US5] Wire `SuggestionConfig` into the Lua config system — add `ai_suggestions` table to config with `enabled`, `debounce_ms`, `accept_key`, `context_lines` fields
- [ ] T029 [US5] Respect `enabled = false` — when disabled, skip all suggestion logic entirely (no debounce, no queries, no overlay)
- [ ] T030 [US5] Respect custom `debounce_ms` and `context_lines` — pass config values to the timer and context extraction

**Checkpoint**: All config options work as documented.

---

## Phase 8: Polish & Cross-Cutting

- [ ] T031 Run `cargo test --all` to verify no regressions
- [ ] T032 Run `cargo fmt --all`
- [ ] T033 Run `cargo clippy --package arcterm-ai`
- [ ] T034 Test full workflow per quickstart.md
- [ ] T035 Update spec status to Complete

---

## Dependencies & Execution Order

- **Setup (Phase 1)**: No dependencies
- **Foundational (Phase 2)**: Depends on Setup
- **US1 (Phase 3)**: Depends on Foundational — core MVP
- **US2 (Phase 4)**: Depends on US1 — needs overlay to intercept Tab
- **US3 (Phase 5)**: Depends on US1 — needs overlay to dismiss
- **US4 (Phase 6)**: Depends on US1 — extends context in queries
- **US5 (Phase 7)**: Depends on US1 — config gates everything
- **Polish (Phase 8)**: Depends on all

US2, US3, US4, US5 can run in parallel after US1.

---

## Implementation Strategy

### MVP (US1 + US2)
1. Setup + Foundational
2. US1: Ghost text appears after typing pause
3. US2: Tab accepts the suggestion
4. **STOP**: User can see and accept suggestions — feature is usable

### Full Feature
5. US3: Dismiss paths (Escape, arrow keys, Enter)
6. US4: Context awareness + program detection
7. US5: Configuration
