---
description: "Task list for AI Integration Layer"
---

# Tasks: AI Integration Layer

**Input**: Design documents from `/specs/004-ai-integration/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, quickstart.md

**Tests**: Not explicitly requested. Verification via `cargo test --all` and manual testing with Ollama per quickstart.md.

**Organization**: Tasks grouped by user story for independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Project Initialization)

**Purpose**: Create the `arcterm-ai` crate with dependencies and establish the module structure.

- [x] T001 Create `arcterm-ai/` directory with `Cargo.toml` declaring dependencies on `ureq` (with `stream` feature), `serde`, `serde_json`, `log`, `anyhow`, `mux` (path dep), `config` (path dep) — and add to workspace `members` in root `Cargo.toml`
- [x] T002 Create `arcterm-ai/src/lib.rs` with module declarations for `backend`, `context`, `config`, `prompts`, and `destructive`
- [x] T003 Create stub files for all modules: `backend/mod.rs`, `backend/ollama.rs`, `backend/claude.rs`, `context.rs`, `config.rs`, `prompts.rs`, `destructive.rs`
- [x] T004 Verify the new crate compiles with `cargo check --package arcterm-ai`

**Checkpoint**: New crate exists and compiles with all module stubs.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Implement the LLM backend trait, config types, context extraction, and system prompts that all user stories depend on.

- [x] T005 [P] Implement `AiConfig` in `arcterm-ai/src/config.rs` — struct with `endpoint` (default `http://localhost:11434`), `model` (default `qwen2.5-coder:7b`), `api_key` (Option), `context_lines` (default 30); implement Default
- [x] T006 [P] Implement `LlmBackend` trait in `arcterm-ai/src/backend/mod.rs` — define `chat(&self, messages: &[Message]) -> Result<Box<dyn Read>>` for streaming and `generate(&self, prompt: &str) -> Result<Box<dyn Read>>` for one-shot; define `Message` struct with `role` and `content` fields
- [x] T007 [P] Implement system prompts in `arcterm-ai/src/prompts.rs` — `AI_PANE_SYSTEM_PROMPT` (terse terminal assistant, flags destructive ops) and `COMMAND_OVERLAY_SYSTEM_PROMPT` (return only a shell command, no explanation)
- [x] T008 [P] Implement destructive command detection in `arcterm-ai/src/destructive.rs` — `is_destructive(command: &str) -> bool` pattern matching against known dangerous patterns (`rm -rf`, `DROP TABLE`, `git push --force`, `chmod -R 777`, `dd if=`, `mkfs`, etc.); `WARNING_LABEL` constant
- [x] T009 Implement `PaneContext` extraction in `arcterm-ai/src/context.rs` — `extract_context(pane: &dyn Pane, lines: u32) -> PaneContext` that reads the last N lines of scrollback via `get_logical_lines()`, CWD via `get_current_working_dir()`, and foreground process via `get_foreground_process_name()`
- [x] T010 Implement `format_context_for_llm(ctx: &PaneContext) -> String` in `arcterm-ai/src/context.rs` — format the context into a user message string that the LLM can understand (CWD, last command, terminal output)
- [x] T011 Verify `cargo check --package arcterm-ai` compiles and run unit tests for destructive detection

**Checkpoint**: LLM trait defined. Config with defaults. Context extraction from panes. System prompts ready. Destructive detection working.

---

## Phase 3: User Story 1 — AI Pane (Priority: P1)

**Goal**: Conversational AI pane that reads sibling context and streams responses.

**Independent Test**: Open AI pane, ask about a build error visible in sibling pane, receive streaming answer.

- [x] T012 [US1] Implement `OllamaBackend` in `arcterm-ai/src/backend/ollama.rs` — POST to `/api/chat` with messages array, parse NDJSON streaming response line-by-line via `BufReader::lines()`, return a `Box<dyn Read>` that yields response tokens
- [x] T013 [US1] Implement Ollama connection detection in `arcterm-ai/src/backend/ollama.rs` — `is_available(&self) -> bool` that attempts a GET to `/api/tags` and returns true/false without blocking for more than 2 seconds
- [x] T0014 [US1] Implement AI pane event loop in `wezterm-gui/src/ai_pane.rs` — create a `TermWizTerminalPane` via `allocate()`, run a dedicated thread that alternates between `poll_input()` for user messages and reading streamed tokens from the backend; render tokens via `term.render(&[Change::Text(token)])`
- [x] T0015 [US1] Implement context injection in `wezterm-gui/src/ai_pane.rs` — on AI pane open, call `extract_context()` on the most recently focused sibling pane; prepend context as a system message in the conversation
- [x] T0016 [US1] Implement conversation history in `wezterm-gui/src/ai_pane.rs` — maintain a `Vec<Message>` that grows with each user message and assistant response; send full history with each request
- [x] T0017 [US1] Implement graceful degradation in `wezterm-gui/src/ai_pane.rs` — if `is_available()` returns false, render "LLM unavailable — is Ollama running?" in the pane; if a streaming connection drops mid-response, render "[Connection lost]" and allow the user to send a new message
- [x] T0018 [US1] Add `KeyAssignment::OpenAiPane` variant in `config/src/keyassignment.rs` and handle it in `wezterm-gui/src/termwindow/mod.rs` — split the active tab and insert the AI pane via `tab.split_and_insert()`
- [x] T0019 [US1] Register default keybinding `leader + i` for `OpenAiPane` in the config defaults
- [x] T020 [US1] Write unit tests in `arcterm-ai/tests/backend_tests.rs` — test Ollama request formatting, test NDJSON line parsing, test connection failure handling
- [x] T021 [US1] Verify `cargo test --package arcterm-ai` passes

**Checkpoint**: AI pane opens, reads sibling context, streams responses from Ollama, maintains conversation history. Graceful fallback when Ollama unavailable.

---

## Phase 4: User Story 2 — Command Overlay (Priority: P1)

**Goal**: Floating prompt that returns one shell command and pastes it on Enter.

**Independent Test**: Press Ctrl+Space, type a question, receive a command, Enter to paste.

- [x] T0022 [US2] Implement command overlay in `wezterm-gui/src/overlay/ai_command_overlay.rs` — create a `TermWizTerminalPane` overlay via `start_overlay()`; use `termwiz::lineedit::LineEditor` for the input prompt; on submit, send query + active pane context to `OllamaBackend::generate()`
- [x] T0023 [US2] Implement one-shot response handling in `wezterm-gui/src/overlay/ai_command_overlay.rs` — stream tokens from the LLM, collect into a single command string, display below the input prompt; strip any markdown backticks or explanation text
- [x] T0024 [US2] Implement paste-on-Enter in `wezterm-gui/src/overlay/ai_command_overlay.rs` — when the user presses Enter on the displayed command, call `pane.send_text()` to inject the command into the active pane, then close the overlay
- [x] T0025 [US2] Implement Escape-to-dismiss — pressing Escape at any point closes the overlay without pasting
- [x] T0026 [US2] Implement destructive command warning in `wezterm-gui/src/overlay/ai_command_overlay.rs` — if `is_destructive()` returns true for the returned command, render the command with a `⚠ DESTRUCTIVE` prefix in red/yellow and require a second Enter press to confirm
- [x] T0027 [US2] Add `KeyAssignment::ToggleCommandOverlay` variant in `config/src/keyassignment.rs` and handle it to show/hide the overlay
- [x] T0028 [US2] Register default keybinding `Ctrl+Space` for `ToggleCommandOverlay`
- [x] T0029 [US2] Verify `cargo test --package arcterm-ai` and overlay integration works

**Checkpoint**: Command overlay opens, accepts input, returns one command, pastes on Enter, dismisses on Escape. Destructive commands get warnings.

---

## Phase 5: User Story 3 — Multi-Model Support (Priority: P2)

**Goal**: Claude API works as an alternative backend, selectable via config.

**Independent Test**: Configure Claude API key, use AI pane, verify response comes from Claude.

- [ ] T030 [P] [US3] Implement `ClaudeBackend` in `arcterm-ai/src/backend/claude.rs` — POST to `https://api.anthropic.com/v1/messages` with API key header, messages array, and `stream: true`; parse SSE streaming response; return streamed tokens
- [ ] T031 [P] [US3] Implement backend factory in `arcterm-ai/src/backend/mod.rs` — `create_backend(config: &AiConfig) -> Box<dyn LlmBackend>` that returns `OllamaBackend` or `ClaudeBackend` based on config (if `api_key` is set and model starts with "claude", use Claude; otherwise Ollama)
- [ ] T032 [US3] Wire config-driven backend selection into AI pane and command overlay — replace hardcoded `OllamaBackend` with `create_backend()` call
- [ ] T033 [US3] Write unit tests for Claude request formatting and SSE response parsing
- [ ] T034 [US3] Verify `cargo test --package arcterm-ai` passes

**Checkpoint**: Both Ollama and Claude backends work. Config determines which is used.

---

## Phase 6: User Story 4 — Context Refresh (Priority: P2)

**Goal**: User can refresh sibling pane context while the AI pane is open.

**Independent Test**: Open AI pane, run new command in sibling, refresh context, ask about new output.

- [ ] T035 [US4] Implement context refresh in `wezterm-gui/src/ai_pane.rs` — on receiving the refresh keybinding, re-read sibling pane context via `extract_context()` and append a system message noting the updated context
- [ ] T036 [US4] Add `KeyAssignment::RefreshAiContext` variant in `config/src/keyassignment.rs` and handle it in the AI pane
- [ ] T037 [US4] Register default keybinding `leader + c` for `RefreshAiContext`
- [ ] T038 [US4] Handle edge case: sibling pane closed — render "No sibling pane available for context" when refresh is attempted but the sibling no longer exists

**Checkpoint**: Context refresh updates the AI's awareness of terminal state. Edge cases handled.

---

## Phase 7: User Story 5 — Destructive Command Warnings (Priority: P3)

**Goal**: Destructive commands are flagged with visible warnings in both AI pane and overlay.

**Independent Test**: Ask AI to suggest `rm -rf`, verify warning label appears.

- [ ] T039 [US5] Implement AI pane warning rendering in `wezterm-gui/src/ai_pane.rs` — after receiving a complete response, scan for destructive patterns; if found, prepend a bold red `⚠ DESTRUCTIVE COMMAND` warning line before the command
- [ ] T040 [US5] Write unit tests for destructive detection edge cases — test `rm -rf /`, `rm -rf .`, `DROP TABLE users`, `git push --force origin main`, `chmod -R 777 /`, `dd if=/dev/zero of=/dev/sda`; test that safe commands (`rm file.txt`, `git push`, `chmod 644`) are NOT flagged
- [ ] T041 [US5] Verify `cargo test --package arcterm-ai` passes

**Checkpoint**: Destructive commands consistently flagged. No false positives on common safe commands.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Final verification and cleanup.

- [ ] T042 Run full `cargo test --all` to verify all existing tests pass
- [ ] T043 Run `cargo fmt --all` to ensure formatting is clean
- [ ] T044 Run `cargo clippy --package arcterm-ai` for lint checks
- [ ] T045 Verify `cargo build --release` succeeds
- [ ] T046 Test full workflow end-to-end per quickstart.md (AI pane + overlay + context refresh + destructive warning)
- [ ] T047 Update `specs/004-ai-integration/spec.md` status from "Draft" to "Complete"

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup — core types and traits
- **User Story 1 (Phase 3)**: Depends on Foundational — needs backend + context
- **User Story 2 (Phase 4)**: Depends on US1 (shares OllamaBackend and context extraction)
- **User Story 3 (Phase 5)**: Depends on US1 (extends backend system)
- **User Story 4 (Phase 6)**: Depends on US1 (extends AI pane)
- **User Story 5 (Phase 7)**: Depends on US1 + US2 (adds warnings to both)
- **Polish (Phase 8)**: Depends on ALL user stories

### User Story Dependencies

- **US1 (P1)**: Depends on Foundational. Core MVP — AI pane with Ollama.
- **US2 (P1)**: Depends on US1 (reuses backend, context extraction).
- **US3 (P2)**: Depends on US1 (adds Claude as alternative backend).
- **US4 (P2)**: Depends on US1 (extends AI pane with refresh). Can parallel with US2/US3.
- **US5 (P3)**: Depends on US1 + US2 (adds warnings to both interfaces).

### Parallel Opportunities

- **Phase 2**: T005-T008 are all parallelizable (different files)
- **Phase 3**: T012-T013 (backend) can parallel with T014-T017 (GUI) after trait is defined
- **Phase 5**: T030, T031 are parallelizable (different files)
- **US3 and US4** can run in parallel after US1 completes

---

## Parallel Example: Phase 2 (Foundational)

```bash
# All four modules touch different files — fully parallel:
Task: "T005 [P] Implement AiConfig in config.rs"
Task: "T006 [P] Implement LlmBackend trait in backend/mod.rs"
Task: "T007 [P] Implement system prompts in prompts.rs"
Task: "T008 [P] Implement destructive detection in destructive.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (crate, deps)
2. Complete Phase 2: Foundational (backend trait, config, context, prompts, destructive)
3. Complete Phase 3: User Story 1 (AI pane with Ollama)
4. **STOP and VALIDATE**: Open AI pane, ask about terminal output, get streaming answer
5. Demo: AI reads build error from sibling pane and suggests fix

### Incremental Delivery

1. US1 → AI pane with Ollama (MVP — the headline feature)
2. Add US2 → Command overlay for quick lookups
3. Add US3 + US4 in parallel → Claude support + context refresh
4. Add US5 → Destructive command warnings
5. Each story adds value without breaking previous stories

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story
- Ollama must be running for integration testing — unit tests use mocked responses
- `ureq` is the HTTP client (synchronous, streaming via `Read` trait)
- AI pane and overlay run on dedicated OS threads via `spawn_into_new_thread`
- Conversation history is in-memory only — lost when AI pane is closed
- No terminal data is sent to remote APIs unless user explicitly configures a remote model
