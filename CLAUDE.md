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

## Architecture

This is a Rust workspace. Key crates:
- `wezterm-gui/` — Main GUI binary (entry point: `src/main.rs`)
- `wezterm/` — CLI binary
- `term/` — Core terminal model (VT parsing, escape sequences)
- `config/` — Configuration and Lua plugin system
- `mux/` — Multiplexer (tabs, panes, sessions)
- `termwiz/` — Terminal capabilities and wizardry library
- `window/` — Cross-platform windowing abstraction

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

## Planned ArcTerm Extensions

1. **WASM Plugin System** — `arcterm-wasm-plugin` crate (capability-based sandbox)
2. **AI Integration** — `arcterm-ai` crate (Ollama/Claude, cross-pane context)
3. **Structured Output** — OSC 7770 protocol for rich content rendering

## Active Feature Branch

- `001-rebrand-completion` — Completing the WezTerm → ArcTerm rebrand across all user-visible surfaces, CI pipelines, and platform assets
