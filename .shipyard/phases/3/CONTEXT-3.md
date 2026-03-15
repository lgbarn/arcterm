# Phase 3 Context — Design Decisions

## Pane Layout Engine
**Decision:** Binary tree splits (like tmux).
**Rationale:** Each split divides a region into two children. Simple, predictable. Split ratios configurable via drag or Leader+arrow.

## Leader Key
**Decision:** Ctrl+a (tmux-style default, configurable in config.toml).
**Key bindings:**
- Leader+n → new split (horizontal by default)
- Leader+v → vertical split
- Leader+q → close focused pane
- Leader+z → zoom/fullscreen toggle
- Leader+arrow → resize focused pane
- Leader+w → workspace switcher (future, stub for now)
- Ctrl+h/j/k/l → navigate between panes (always active, no leader prefix)

## Neovim-Aware Pane Crossing
**Decision:** Full Neovim integration in Phase 3.
- Detect Neovim by process name in the PTY child process tree
- Connect to Neovim's `--listen` socket (or `$NVIM` env var)
- Query Neovim for split count in the target direction
- If Neovim has a split in that direction, pass the key through; otherwise, cross to arcterm pane
- Fall back gracefully to standard pane crossing if Neovim not detected or socket unavailable

## Pane Borders
**Decision:** 1px colored line borders between panes.
- Focused pane: bright border (e.g., palette accent color)
- Unfocused panes: dim border (e.g., dark gray)
- Rendered via the quad pipeline from Phase 2

## Tab Model
- Tabs group independent pane trees
- Tab bar at the top of the window (minimal, configurable visibility)
- Keyboard: Leader+1..9 switch tabs, Leader+t new tab, Leader+w close tab
- Each tab has its own set of PTY-backed panes

## Command Palette
- Ctrl+Space opens overlay with fuzzy-searchable commands
- Phase 3 scope: pane/tab management commands only
- Future phases add workspace, plugin, and AI commands
