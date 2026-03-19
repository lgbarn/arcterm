# Data Model: AI Integration Layer

**Date**: 2026-03-19
**Feature**: 004-ai-integration

## AI Pane

A special pane type that connects to an LLM instead of a PTY.

**Attributes**:
- `pane_id: PaneId` — unique pane identifier (from mux system)
- `sibling_pane_id: Option<PaneId>` — the pane whose context is read
- `backend: LlmBackend` — which LLM provider to use
- `conversation: Vec<Message>` — chat history for the session
- `system_prompt: String` — the system prompt sent with every request
- `is_streaming: bool` — whether a response is currently being streamed
- `context: Option<PaneContext>` — most recent sibling context snapshot

**State transitions**:
```
Idle → Querying → Streaming → Idle
                → Error → Idle (on retry or new message)
```

## Command Overlay

A floating UI element for one-shot command generation.

**Attributes**:
- `visible: bool` — whether the overlay is shown
- `input: String` — the user's current query text
- `result: Option<String>` — the returned command (if any)
- `is_loading: bool` — whether waiting for LLM response
- `target_pane_id: PaneId` — the pane to paste the command into

**State transitions**:
```
Hidden → InputActive → Loading → ShowingResult → Hidden
                     → Error → InputActive (user can retry)
```

## PaneContext

A snapshot of a terminal pane's state, sent with each LLM query.

**Attributes**:
- `scrollback: String` — last 30 lines of terminal output
- `cwd: String` — current working directory
- `last_command: Option<String>` — the most recently executed command
- `exit_code: Option<i32>` — exit code of the last command
- `pane_dimensions: (u32, u32)` — rows and columns

## Message

A single message in an AI pane conversation.

**Attributes**:
- `role: MessageRole` — `System`, `User`, or `Assistant`
- `content: String` — the message text

## LlmBackend

An abstraction over LLM providers.

**Variants**:
- `Ollama { endpoint: String, model: String }` — local inference
- `Claude { api_key: String, model: String }` — Anthropic API

**Shared interface**:
- `send(messages: &[Message]) -> Stream<String>` — send messages, receive streaming tokens
- `generate(prompt: &str, context: &PaneContext) -> Stream<String>` — one-shot generation (for command overlay)

## AiConfig

User configuration for the AI subsystem.

**Attributes**:
- `endpoint: String` — LLM endpoint URL (default: `http://localhost:11434`)
- `model: String` — model identifier (default: `qwen2.5-coder:7b`)
- `api_key: Option<String>` — API key for remote providers (None for Ollama)
- `ai_pane_shortcut: String` — keybinding (default: `leader + i`)
- `overlay_shortcut: String` — keybinding (default: `Ctrl+Space`)
- `context_refresh_shortcut: String` — keybinding (default: `leader + c`)
- `context_lines: u32` — number of scrollback lines to include (default: 30)

## Relationships

```
AiConfig → creates → LlmBackend (Ollama or Claude)
AI Pane → reads → PaneContext (from sibling pane)
AI Pane → sends → Message[] → LlmBackend → Stream<tokens>
Command Overlay → reads → PaneContext (from active pane)
Command Overlay → sends → prompt → LlmBackend → Stream<tokens>
LlmBackend → detects → destructive commands → warning labels
```
