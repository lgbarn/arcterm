# ArcTerm - CLAUDE.md

## Project Overview

ArcTerm is a fork of [WezTerm](https://github.com/wez/wezterm) (MIT license), rebranded and extended with AI-powered features.

## Build

```bash
cargo build --release          # Full release build
cargo check --package wezterm-gui  # Quick type-check of GUI
cargo run --bin wezterm-gui    # Run in debug mode
cargo test --all               # Run all tests
cargo fmt --all                # Format code
```

## Running with AI Features

AI features require Ollama running locally. Quick setup:

```bash
# Install Ollama: https://ollama.com/download
ollama pull qwen2.5-coder:7b   # Default model
ollama serve                   # Start server (if not already running as a service)
cargo run --bin wezterm-gui    # Launch ArcTerm
```

See `docs/local-llm-setup.md` for full Ollama setup and Claude API alternative.

## Configuration

ArcTerm loads config from (in priority order):
1. `~/.arcterm.lua`
2. `$XDG_CONFIG_HOME/arcterm/arcterm.lua`
3. `~/.wezterm.lua` (fallback, with a deprecation notice)
4. `$XDG_CONFIG_HOME/wezterm/wezterm.lua` (fallback)

Prefer `arcterm.lua` for new setups. Existing `wezterm.lua` files continue to work.

## Architecture

This is a Rust workspace. Key crates:
- `wezterm-gui/` — Main GUI binary (entry point: `src/main.rs`)
- `wezterm/` — CLI binary
- `term/` — Core terminal model (VT parsing, escape sequences)
- `config/` — Configuration and Lua plugin system
- `mux/` — Multiplexer (tabs, panes, sessions)
- `termwiz/` — Terminal capabilities and wizardry library
- `window/` — Cross-platform windowing abstraction

### ArcTerm-Specific Crates

- `arcterm-wasm-plugin/` — WASM plugin system using wasmtime Component Model with capability-based sandboxing. Plugins are `.wasm` files declared in `arcterm.lua`.
- `arcterm-ai/` — LLM backend abstraction (Ollama, Claude API), pane context extraction, system prompts, destructive command detection, and inline suggestion logic.
- `arcterm-structured-output/` — OSC 7770 escape sequence parser and renderer. Converts JSON payloads into syntax-highlighted terminal output (code, JSON trees, diffs, images).

## Upstream Relationship

- `upstream` remote → `wez/wezterm` (original)
- `origin` remote → `lgbarn/arcterm` (our fork)
- Keep ArcTerm-specific code in dedicated `arcterm-*` crates to minimize merge conflicts
- Periodically merge upstream: `git fetch upstream && git merge upstream/main`

## Rebrand Notes

User-facing strings changed from "WezTerm" to "ArcTerm":
- `TERM_PROGRAM` env var → "ArcTerm"
- Menu bar app name → "ArcTerm"
- About dialog, quit dialog, update notifications
- Windows app model ID → "com.lgbarn.arcterm"
- Terminal identification in mux, SSH, tmux integration

Internal crate names (`wezterm-gui`, `wezterm-font`, etc.) are NOT renamed to keep upstream merges clean.

## ArcTerm Features

1. **Rebrand** — WezTerm → ArcTerm across all user-visible surfaces; internal crate names preserved for upstream merge hygiene.
2. **WASM Plugin System** — `arcterm-wasm-plugin` crate; wasmtime v36 Component Model; capability strings like `"fs:read:/home/user"` declared per-plugin in config; `terminal:read` granted by default.
3. **AI Integration** — `arcterm-ai` crate; Ollama (default: `qwen2.5-coder:7b`) and Claude API backends; interactive AI pane (`OpenAiPane` action) and command overlay (`ToggleCommandOverlay` action); cross-pane context via scrollback and CWD.
4. **Inline AI Suggestions** — ghost-text command completions using `arcterm-ai`; debounced 300ms after keystroke; accept with Tab, dismiss with Escape; requires OSC 133 shell integration or heuristic fallback.
5. **Structured Output** — `arcterm-structured-output` crate; OSC 7770 escape sequence; renders code (syntax-highlighted via syntect), JSON trees, diffs, and images natively in the terminal.

## Active Technologies
- Rust (edition 2021) + arcterm-ai (existing), termwiz, mux (006-warp-style-ai-ux)
- N/A — ephemeral UI state (006-warp-style-ai-ux)

## Recent Changes
- 006-warp-style-ai-ux: Added Rust (edition 2021) + arcterm-ai (existing), termwiz, mux
