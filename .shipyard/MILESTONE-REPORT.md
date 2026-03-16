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

---

# Milestone Report: Arcterm v0.1.1 — Stabilization Release

**Completed:** 2026-03-16
**Phases:** 3/3 complete (Phases 9, 10, 11)
**Tests:** 606 passing (up from 558 baseline, +48 new)
**Commits:** 29

## Summary

v0.1.1 resolves all 13 open review issues from v0.1.0, both High-severity concerns, and all 6 Medium-severity concerns. No new features — every fix makes existing features robust, correct, and safe.

## Phase Summaries

### Phase 9: Foundation Fixes — Grid, VT, PTY, Plugin (Parallel)
16 fixes across 4 independent crates executed as 4 parallel plans. Grid: scroll region bounds, alt_grid resize, scroll_offset encapsulation, in-place scroll. VT/PTY: regression tests. Plugin: epoch interruption, full WASM tool dispatch, KeyInput WIT variant, path traversal + symlink guards. Review caught usize underflow regression.

### Phase 10: Application Input and UX Fixes
scroll_offset API migration (8 compile errors). Regression tests for ISSUE-002–005. ISSUE-006: cursor block glyph (U+2588) for blank cells. ctrl_char_byte helper extraction.

### Phase 11: Config and Runtime Hardening
M-4: scrollback_lines capped at 1M. M-5: GPU init returns Result (no panics). M-3: async Kitty image decode via spawn_blocking + mpsc channel.

## Key Decisions
- KeyInput: dedicated WIT variant (not unreachable)
- 30-second epoch deadline for WASM calls
- U+2588 block glyph for cursor (defer wgpu quad pass)
- spawn_blocking + bounded mpsc(32) for async image decode
- 1,000,000 line scrollback cap

## Security
All 3 phase audits: PASS. No critical findings. Plugin path traversal hardened. Epoch interruption activated. GPU init no longer panics.

## Known Issues (Carried Forward)
9 non-blocking issues logged in ISSUES.md for v0.2.0 consideration.

## Metrics
- Files changed: 56
- Lines added: 5,617 | Lines removed: 172
- Commits: 29
- Tests: 606 (baseline 558, +48 new)
- Clippy: clean across workspace
