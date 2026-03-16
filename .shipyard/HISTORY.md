# Shipyard History

## 2026-03-15 — Project Initialized
- **Phase:** 1
- **Status:** ready
- **Message:** Project initialized. Interactive mode, per-task commits, detailed reviews.

## 2026-03-15 — Phase 1 Planned
- **Phase:** 1
- **Status:** planned
- **Message:** 6 plans across 3 waves. Verdict: READY. Research completed (crate APIs, rendering architecture). Critique passed feasibility stress test.

## 2026-03-15 — Phase 1 Build Complete
- **Phase:** 1
- **Status:** complete
- **Plans:** 6/6 complete (1.1, 2.1, 2.2, 2.3, 3.1, 3.2)
- **Tests:** 98 passing (40 core, 52 vt, 6 pty)
- **Reviews:** All PASS after fixes (2 retry cycles for plans 2.2, 2.3, 3.1, 3.2)
- **Key Decisions:**
  - glyphon upgraded 0.9→0.10 (wgpu 28 compatibility)
  - PtySession returns receiver separately (better for app integration)
  - Cursor rendered via inverse-video on glyph color
- **Notes:**
  - Plan 2.1 (VT parser): PASS first try, 92 tests
  - Plan 2.2 (PTY): required retry for missing test
  - Plan 2.3 (GPU): required retry for begin_frame Result signature
  - Plan 3.1 (App): required retry for input/exit handling
  - Plan 3.2 (CI): required retry for config completeness
  - Clippy violations fixed post-verification

## 2026-03-15 — Phase 2 Planned
- **Phase:** 2
- **Status:** planned
- **Message:** 7 plans across 3 waves. Verdict: CAUTION (file conflicts manageable via wave ordering). Research identified bg color rendering gap and DEC private mode gaps.

## 2026-03-15 — Phase 2 Build Complete
- **Phase:** 2
- **Status:** complete
- **Plans:** 7/7 complete (1.1, 1.2, 2.1, 2.2, 2.3, 3.1, 3.2)
- **Tests:** 195 passing (55 app, 51 core, 6 pty, 6 render, 77 vt)
- **Key Features:**
  - Scrollback buffer, scroll regions, alternate screen
  - DEC private modes, app cursor keys, bracketed paste
  - wgpu quad pipeline for backgrounds + cursor
  - TOML config with hot-reload
  - 8 color schemes + custom RGB overrides
  - Mouse selection + clipboard + scroll viewport
  - DSR/DA responses, Mailbox present mode, FPS counter
- **Retry cycles:** Plans 1.2 (esc_dispatch guard + missing modes), 2.1-2.3 (clippy fixes)

## 2026-03-15 — Phase 3 Planned
- **Phase:** 3
- **Status:** planned
- **Message:** 8 plans across 3 waves. Verdict: READY. Full Neovim integration included. Research confirmed no new GPU infrastructure needed.

## 2026-03-15 — Phase 3 Build Complete
- **Phase:** 3
- **Status:** complete
- **Plans:** 7/8 complete (1.1, 1.2, 2.1, 2.2, 3.1, 3.2, 3.3; 3.4 acceptance deferred)
- **Tests:** 298 passing (158 app, 51 core, 6 pty, 6 render, 77 vt)
- **Key Features:**
  - Binary tree pane layout with directional navigation
  - TabManager with add/close/switch
  - Multi-pane rendering (single render pass)
  - Leader key state machine (Ctrl+a, configurable)
  - Neovim-aware pane crossing via msgpack-RPC
  - Command palette with fuzzy search
  - Click-to-focus, tab bar, border drag resize

## 2026-03-15 — Phase 4 Planned
- **Phase:** 4
- **Status:** planned
- **Message:** 6 plans across 3 waves. Verdict: READY. OSC 7770 protocol, syntect highlighting, auto-detection, Kitty graphics (with APC pre-scanner workaround).

## 2026-03-15 — Phase 4 Build Complete
- **Phase:** 4
- **Status:** complete
- **Plans:** 6/6 complete (1.1, 1.2, 2.1, 2.2, 3.1, 3.2)
- **Tests:** 398 passing (176 app, 51 core, 6 pty, 38 render, 127 vt)
- **Key Features:**
  - OSC 7770 structured output protocol
  - APC pre-scanner for Kitty graphics (vte APC workaround)
  - Syntect code highlighting, diff/JSON/markdown renderers
  - Auto-detection with conservative regex heuristics
  - Kitty graphics: parser, chunk assembler, textured wgpu quads
  - Terminal upgraded to GridState + ApcScanner pipeline
  - Copy button on code blocks

## 2026-03-15 — Phase 5 Planned
- **Phase:** 5
- **Status:** planned
- **Message:** 6 plans across 3 waves. Verdict: PASS. Workspace TOML, auto-save/restore, clap CLI, workspace switcher.

## 2026-03-15 — Phase 5 Build Complete
- **Phase:** 5
- **Status:** complete
- **Tests:** 400+ passing (216 app, 51 core, 10 pty, 38 render, 127 vt)
- **Key Features:** Workspace TOML model, CWD capture, clap CLI, auto-save/restore, workspace switcher, Leader+s save

## 2026-03-15 — Phase 6 Planned
- **Phase:** 6
- **Status:** planned
- **Message:** 3 plans across 3 waves. Verdict: CONDITIONAL PASS. wasmtime 42, wit-bindgen Component Model, plugin manifest, event bus, CLI, example plugins.

## 2026-03-15 — Phase 6 Build Complete
- **Phase:** 6
- **Status:** complete
- **Tests:** 457 passing (216 app, 51 core, 12 plugin, 3 plugin-integration, 10 pty, 38 render, 127 vt)
- **Key Features:** wasmtime 42 runtime, WIT Component Model, plugin manifest + permissions, event bus, CLI, MCP tool registry, plugin pane rendering, example plugins

## 2026-03-15 — Phase 7 Planned
- **Phase:** 7
- **Status:** planned
- **Message:** 3 plans across 3 waves. Verdict: PASS. AI detection, cross-pane context, MCP discovery, plan status, error bridging.

## 2026-03-15 — Phase 7 Build Complete
- **Phase:** 7
- **Status:** complete
- **Tests:** 517 passing (257 app, 51 core, 12 plugin, 3 plugin-int, 10 pty, 38 render, 146 vt)
- **Key Features:** AI detection, PaneContext, OSC 133, MCP tool discovery/invocation, plan status layer, Leader+a/+p, error bridging, context query/response

## 2026-03-15 — Phase 8 Planned
- **Phase:** 8
- **Status:** planned
- **Message:** 3 plans across 2 waves. Verdict: PASS WITH AMENDMENTS. Config overlays, cross-pane search, performance optimization, release packaging, documentation.

## 2026-03-16 — Phase 8 Build Complete — ALL PHASES DONE
- **Phase:** 8
- **Status:** complete
- **Tests:** 562 passing (298 app, 55 core, 12 plugin, 3 plugin-int, 10 pty, 38 render, 146 vt)
- **Tag:** v0.1.0
- **Key Features:** Config overlays + flatten, cross-pane regex search, lazy syntect init, deferred plugin loading, man page, cargo-dist release workflow, example configs
