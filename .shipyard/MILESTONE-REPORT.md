# Milestone Report: Arcterm v0.1.0

**Completed:** 2026-03-16
**Phases:** 8/8 complete
**Tests:** 570 passing
**Tag:** v0.1.0

## Phase Summaries

### Phase 1: Foundation
GPU-rendered terminal window running a shell. wgpu + glyphon renderer, vte VT100 parser, portable-pty shell spawning, 5-crate Cargo workspace, GitHub Actions CI.

### Phase 2: Terminal Fidelity & Configuration
Daily-driver quality: scrollback (10K lines), DEC private modes, alternate screen, TOML config with hot-reload, 8 color schemes, mouse text selection + clipboard, 120+ FPS optimization.

### Phase 3: Multiplexer
Vim-navigable pane splits and tabs. Binary tree layout engine, Neovim-aware pane crossing via msgpack-RPC, command palette (Ctrl+Space), leader key state machine (Ctrl+a).

### Phase 4: Structured Output
OSC 7770 protocol for typed AI content. Syntect code highlighting, diff/JSON/markdown renderers, auto-detection engine, Kitty graphics protocol with inline images.

### Phase 5: Workspaces
Project-aware session persistence. TOML workspace files, auto-save/restore, clap CLI (open/save/list), fuzzy workspace switcher (Leader+w).

### Phase 6: WASM Plugins
wasmtime 42 Component Model runtime. WIT interface, capability-based sandbox, plugin manifest, event bus, MCP tool registry, hello-world + system monitor examples.

### Phase 7: AI Integration
AI tool detection (claude/codex/gemini/aider), cross-pane context API, OSC 133 shell integration, MCP tool discovery/invocation, plan status layer, error bridging.

### Phase 8: Polish & Release
Config overlay system with diff view, cross-pane regex search, lazy syntect init, cargo-dist release packaging, man page, example configs.

## Key Decisions
- Rust + wgpu (WebGPU) for cross-platform GPU rendering
- glyphon for text rendering (wraps cosmic-text + swash)
- vte crate for VT parsing (battle-tested, Alacritty heritage)
- wasmtime Component Model for WASM plugins (industry standard)
- OSC 7770 as the structured output protocol
- Catppuccin Mocha as default color scheme
- Ctrl+a as leader key (tmux-compatible)
- Auto-save session on exit, auto-restore on launch

## Security Audit
- 2 critical findings fixed before ship (plugin path traversal, WASM stdio escape)
- 64MB cap on Kitty image chunk buffer
- Bounds-safe OSC 7770 arg parsing
- No secrets found, no dependency CVEs

## Metrics
- Crates: 6 (arcterm-core, arcterm-vt, arcterm-pty, arcterm-render, arcterm-app, arcterm-plugin)
- Source files: 35+ Rust files
- Tests: 570
- Commits: 80+
- Phases: 8, Plans: 40+, Tasks: 120+
