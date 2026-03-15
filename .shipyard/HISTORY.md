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
