# Phase 2 Context — Design Decisions

## Config File Location
**Decision:** `~/.config/arcterm/config.toml` (XDG-compliant)
**Rationale:** Standard Unix convention. Matches PROJECT.md spec.

## Scrollback Storage
**Decision:** Ring buffer in memory, default 10,000 lines.
**Rationale:** Bounded memory usage, fast access. Lines beyond limit are dropped. Configurable via `scrollback_lines` in config.

## Text Selection
**Decision:** Mouse-only selection for Phase 2.
- Click+drag for character selection
- Double-click for word selection
- Triple-click for line selection
- Copy: Ctrl+Shift+C (Linux) / Cmd+C (macOS)
- Paste: Ctrl+Shift+V (Linux) / Cmd+V (macOS)
**Rationale:** Covers 90% of use cases. Vi-mode selection deferred to Phase 3 (multiplexer phase has vim keybinding infrastructure).

## Color Schemes
**Decision:** Named built-in schemes + custom RGB overrides.
**Built-in schemes:** Catppuccin Mocha (default), Dracula, Solarized Dark, Solarized Light, Nord, Tokyo Night, Gruvbox Dark, One Dark.
**Config:** `color_scheme = "catppuccin-mocha"` or `[colors]` section with per-slot RGB overrides.
**Rationale:** Good UX out of the box, power users can customize everything.

## Lessons from Phase 1
- glyphon 0.10 (not 0.9) for wgpu 28 compatibility
- winit 0.30 requires ApplicationHandler pattern, surface in resumed()
- PtySession receiver returned separately from struct
- Clippy -D warnings must be clean from the start
- Review cycle caught real issues: always verify API signatures match plan
