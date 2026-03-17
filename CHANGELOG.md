# Changelog

All notable changes to arcterm will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-03-17

### Added

- GPU-accelerated terminal rendering via wgpu with text (glyphon) and quad pipelines
- Terminal emulation powered by alacritty_terminal 0.25
- Tmux-style multiplexer with Ctrl+a leader key
  - Horizontal/vertical splits, pane navigation (Ctrl+h/j/k/l), pane resize
  - Tab support with Leader+t (new), Leader+1-9 (switch), Leader+W (close)
  - Pane zoom toggle (Leader+z)
- Native macOS menu bar (Shell, Edit, View, Window, Help) via muda
  - New Window (Cmd+N), New Tab (Cmd+T), Split (Cmd+D), Close (Cmd+W)
  - Copy/Paste, Find with next/prev, Clear Scrollback, Command Palette
  - Font size controls, fullscreen toggle, equalize splits
- Neovim-aware pane navigation — seamless Ctrl+h/j/k/l across nvim and arcterm splits
- Command palette with fuzzy search (Leader key or Cmd+Shift+P)
- Workspace session persistence
  - Save (Leader+s) and restore sessions as human-readable TOML files
  - Auto-restore last session on launch
  - Workspace switcher (Leader+w) with fuzzy filtering
- Cross-pane regex search (Leader+/) with match highlighting
- Kitty graphics protocol support for inline images
- AI agent detection — heuristic detection of Claude, ChatGPT, and other AI agents
- Plan status strip — ambient display of .shipyard/PLAN.md/TODO.md task progress
- Config overlay review (Leader+o) for pending config changes
- WASM plugin system via wasmtime with MCP tool integration
- Configurable color schemes, font size, scrollback, and shell
- OSC 7 (CWD tracking), OSC 133 (prompt/command regions), OSC 7770 (structured content)
- Bracketed paste mode support

### Fixed

- PTY EOF now sets exit code so terminal windows close properly
- Window resize events coalesced to one resize per frame
- Render loop optimized to reduce per-frame allocations

[0.1.0]: https://github.com/lgbarn/arcterm/releases/tag/v0.1.0
