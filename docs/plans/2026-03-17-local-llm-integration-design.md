# Local LLM Integration Design

## Overview

Add local LLM assistant capabilities to arcterm via two components: a persistent AI Pane for conversational interaction and a Command Overlay for quick one-shot command lookup. Both backed by Ollama running locally.

## Architecture

### AI Pane

A dedicated pane type opened via multiplexer command (`leader + i`). Persistent chat session with sibling pane context awareness.

- Connects to Ollama REST API (`POST /api/chat`) for conversation with history
- Streams responses token-by-token into the pane
- On open, automatically pulls context from the active sibling pane:
  - Last 30 lines of scrollback output
  - Current working directory
  - Last executed command + exit code
- User can refresh sibling context manually (`leader + c`)
- Renders Markdown responses using existing `pulldown-cmark` + `syntect` pipeline
- System prompt: terse terminal assistant, returns commands directly, flags destructive operations

### Command Overlay

A floating prompt triggered by `Ctrl+Space`. One question in, one shell command out.

- Minimal text input rendered at top of screen
- Sends query + active pane context (last 30 lines, CWD) to Ollama as one-shot `POST /api/generate`
- System prompt: "Return only a shell command. No explanation."
- Display the returned command; Enter accepts (pastes into active pane), Escape dismisses
- No conversation history, no follow-ups

## Configuration

```toml
[ai]
endpoint = "http://localhost:11434"
model = "qwen2.5-coder:7b"
```

Added to `~/.config/arcterm/config.toml`. Both fields have defaults — zero config required if Ollama is running on the standard port.

## Context Protocol Extension

Extend the context query response to include a `scrollback` field containing the last 30 lines from the target pane's terminal scrollback buffer. This is the only protocol change required.

### Updated context response shape

```json
{
  "cwd": "/home/user/project",
  "last_command": "cargo build",
  "exit_code": 1,
  "scrollback": "error[E0308]: mismatched types\n  --> src/main.rs:42:5\n..."
}
```

## System Prompts

### AI Pane

```
You are a terminal assistant embedded in a GPU-accelerated terminal emulator.
The user is a DevOps engineer. You have context from their active terminal pane
including recent output, working directory, and last command with exit code.

Be terse. Return shell commands directly when applicable. Prefer one-liners.
Flag destructive operations (rm -rf, DROP TABLE, force push, etc.) before
suggesting them. When explaining, keep it short.
```

### Command Overlay

```
You are a shell command generator. Given a question and terminal context,
return exactly one shell command. No explanation, no markdown, no backticks.
Just the command.
```

## Dependencies

- `reqwest` (with `stream` feature) — HTTP client for Ollama REST API
- `serde_json` — already present, used for Ollama request/response serialization
- `tokio` — already present, async streaming

No new heavy dependencies. `reqwest` is the only addition.

## Key Design Decisions

- **Ollama-only backend** with configurable endpoint/model — simplest path, Ollama is the standard for local LLM serving
- **No auto-detection** — if Ollama isn't running, requests fail and UI shows "LLM unavailable". No probing or state machines.
- **Scrollback context (30 lines)** — the killer feature. Lets the LLM read error messages, build output, and stack traces without the user copy-pasting.
- **Two distinct interaction modes** — AI Pane for conversation, Command Overlay for speed. Each optimized for its use case.
- **Streaming responses** — both components stream from Ollama for responsive UX

## File Impact

### New files
- `arcterm-app/src/ai_pane.rs` — AI pane state, Ollama client, chat history, context injection
- `arcterm-app/src/command_overlay.rs` — overlay input, one-shot query, command display/accept

### Modified files
- `arcterm-app/src/config.rs` — add `AiConfig` struct with endpoint/model fields
- `arcterm-app/src/context.rs` — extend context response to include scrollback field
- `arcterm-app/src/multiplexer.rs` — add AI pane creation command (`leader + i`)
- `arcterm-app/src/event.rs` — add Command Overlay toggle keybinding (`Ctrl+Space`)
- `arcterm-app/src/main.rs` — wire up overlay rendering in event loop
- `arcterm-render/src/lib.rs` — overlay rendering pass (text input + command display)
- `Cargo.toml` — add `reqwest` dependency
