# Research: AI Integration Layer

**Date**: 2026-03-19
**Feature**: 004-ai-integration

## Decision 1: Pane Context Reading

**Decision**: Use the existing `Pane` trait methods from `mux/src/pane.rs`:
- `get_lines_as_text(lines)` — read scrollback
- `get_current_working_dir()` — read CWD
- `get_foreground_process_info()` — read process info (for last command)
- `get_cursor_position()` — read cursor position

The Lua API in `lua-api-crates/mux/src/pane.rs` already wraps these with
`get_lines_as_text`, `get_logical_lines_as_text`, `get_current_working_dir`.

**Rationale**: No new APIs needed. The mux layer already exposes everything
required for pane context extraction. Focus pane tracking uses `pane_id`
from the mux's focus notification system.

## Decision 2: HTTP Client

**Decision**: Use `ureq 2.x` as a synchronous HTTP client with streaming
support. The AI pane and overlay run on dedicated OS threads (via
`promise::spawn::spawn_into_new_thread`), so a synchronous client is
correct. `ureq` returns a `Read` impl, enabling NDJSON streaming via
`BufReader::lines()` in a loop — no async runtime needed.

**Rationale**: `http_req` (already present) cannot stream responses.
`reqwest` would pull tokio into the GUI process, conflicting with smol.
`ureq` is lightweight (pure Rust), supports streaming, and works
perfectly on plain OS threads.

**Alternatives considered**:
- `http_req` — already present but no streaming support
- `reqwest` — pulls tokio, conflicts with smol runtime
- `hyper` — too low-level for this use case

## Decision 3: AI Pane Implementation

**Decision**: Implement the AI pane as a `TermWizTermTab` — the same mechanism
used for the search overlay, debug overlay, and other built-in "virtual" panes.
`TermWizTermTab` creates a pane backed by a `termwiz::Terminal` renderer
instead of a PTY. It supports keyboard input, text rendering, and integrates
with the mux system.

**Rationale**: `TermWizTermTab` is the proven pattern for interactive
non-PTY panes in WezTerm. It handles input routing, rendering, and focus
management out of the box. The AI pane becomes a `TermWizTermTab` that
maintains a chat history and renders streamed LLM responses.

## Decision 4: Command Overlay Implementation

**Decision**: Implement as a new overlay type in `wezterm-gui/src/overlay/`,
following the pattern of the existing search overlay (`search_overlay.rs`)
and command palette. The overlay captures keyboard input, displays a text
field + result area, and dismisses on Escape.

**Rationale**: The overlay system already handles focus capture, keyboard
routing, and rendering on top of the terminal content. Following the
existing pattern requires minimal new infrastructure.

## Decision 5: Keybinding Registration

**Decision**: Add new `KeyAssignment` variants:
- `KeyAssignment::OpenAiPane` — opens/focuses the AI pane split
- `KeyAssignment::ToggleCommandOverlay` — shows/hides the command overlay
- `KeyAssignment::RefreshAiContext` — re-reads sibling pane context

Register default bindings in the config layer. Users can override via
their `arcterm.lua` config.

**Rationale**: `KeyAssignment` is the standard enum for all keyboard
actions in WezTerm. Adding variants follows the existing pattern and
integrates with the Lua config system automatically.

## Decision 6: LLM Backend Abstraction

**Decision**: Create an `LlmBackend` trait with two implementations:
`OllamaBackend` and `ClaudeBackend`. The trait defines:
- `chat(messages, context) -> impl Stream<Item=String>` — conversation
- `generate(prompt, context) -> impl Stream<Item=String>` — one-shot

**Rationale**: The trait abstraction keeps the AI pane and command overlay
provider-agnostic. Adding new backends (OpenAI, local llama.cpp, etc.)
only requires implementing the trait.

## Decision 7: Crate Structure

**Decision**: Create `arcterm-ai` as a new workspace member crate containing:
- `backend/mod.rs` — `LlmBackend` trait
- `backend/ollama.rs` — Ollama REST API client
- `backend/claude.rs` — Claude Messages API client
- `context.rs` — `PaneContext` extraction from mux panes
- `config.rs` — `AiConfig` configuration types
- `prompts.rs` — system prompt templates
- `destructive.rs` — destructive command detection patterns

The AI pane and command overlay live in `wezterm-gui/src/` since they
are GUI components that interact with the windowing system.

**Rationale**: Keeps the AI logic in an `arcterm-*` crate per constitution.
GUI-specific code (pane rendering, overlay drawing) stays in the GUI crate.

## Decision 8: Destructive Command Detection

**Decision**: Pattern-match on a static list of dangerous commands/flags:
`rm -rf`, `rm -r /`, `DROP TABLE`, `DROP DATABASE`, `git push --force`,
`git reset --hard`, `chmod -R 777`, `dd if=`, `mkfs`, `:(){ :|:& };:`.

Display a `⚠ DESTRUCTIVE` label in the AI pane response, or highlight
the command in the overlay with a yellow/red background color.

**Rationale**: Static pattern matching is simple, fast, and covers the
most common dangerous operations. It's not meant to be comprehensive
security — just a helpful guardrail.
