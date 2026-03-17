# Summary: Plan 1.1 — Foundation (Config, Ollama Client, Context Extension)

**Phase:** 16
**Plan:** 1.1
**Branch:** main
**Date:** 2026-03-17
**Status:** COMPLETE

---

## Tasks Executed

### Task 1: Add `[ai]` config section

**Files modified:** `arcterm-app/src/config.rs`

Added `AiConfig` struct with `endpoint` and `model` fields, both with `#[serde(default)]` and a `Default` impl providing `http://localhost:11434` and `qwen2.5-coder:7b` as defaults. Added `pub ai: AiConfig` field to `ArctermConfig` struct and its `Default` impl.

TDD: wrote 3 failing tests (`ai_config_defaults`, `ai_config_toml_overrides`, `ai_config_omitted_uses_defaults`) before implementing. All 3 passed after implementation with no regressions to the 25 pre-existing config tests.

**Commit:** `2d3d6bd` — `shipyard(phase-16): add [ai] config section with endpoint and model fields`

---

### Task 2: Create Ollama HTTP client module

**Files created/modified:** `arcterm-app/src/ollama.rs`, `arcterm-app/src/main.rs`, `arcterm-app/Cargo.toml`, `Cargo.lock`

Created `ollama.rs` with full type definitions (`ChatMessage`, `ChatRequest`, `ChatChunk`, `GenerateRequest`, `GenerateChunk`) and `OllamaClient` wrapping `reqwest::Client`. Provides `url()`, `chat()` (streaming POST to `/api/chat`), and `generate()` (one-shot POST to `/api/generate`).

Added `reqwest = { version = "0.12", features = ["json", "stream"] }` to `arcterm-app/Cargo.toml`. Registered `mod ollama;` in `main.rs` after `mod workspace;` (line 197).

All 8 tests passed: `chat_message_serializes`, `chat_request_serializes_with_stream`, `chat_chunk_deserializes`, `chat_chunk_done_deserializes`, `generate_request_serializes`, `generate_chunk_deserializes`, `client_url_building`, `client_url_strips_trailing_slash`.

**Commit:** `5505ffc` — `shipyard(phase-16): add Ollama REST API client with chat and generate endpoints`

---

### Task 3: Extend `SiblingContext` with scrollback field

**Files modified:** `arcterm-app/src/context.rs`, `arcterm-app/src/ollama.rs`

Added `pub scrollback: Vec<String>` to `SiblingContext`. Updated `collect_sibling_contexts` to call `Terminal::all_text_rows()` and take the last 30 lines. Updated `format_context_osc7770` to include `"scrollback":[...]` in the JSON payload with proper escape handling.

Fixed the 2 existing `SiblingContext` test constructions in `format_context_osc7770_valid_json` by adding `scrollback: vec![]`.

Both new tests passed: `sibling_context_has_scrollback`, `format_context_osc7770_includes_scrollback`.

Also fixed 2 clippy `borrowed-expression-implements-required-traits` warnings in `ollama.rs` (removed unnecessary `&` from `.post(&self.url(...))` calls).

**Commit:** `88fc27f` — `shipyard(phase-16): add scrollback field to SiblingContext (last 30 lines)`

---

## Final State

- All tests passing: 335 (bin) + 21 (lib) + 3 (integration) = 359 tests
- `cargo clippy --package arcterm-app` clean except for 7 expected `dead_code` warnings on `ollama.rs` types/methods (not yet wired into application logic — will be resolved in Phase 16 Plan 1.2/1.3)
- No regressions

## Deviations

None. All tasks implemented exactly as specified in the plan. The clippy fix (removing `&` from `.post()` calls in `ollama.rs`) was done inline during the post-all-tasks clippy run as required by the instructions.
