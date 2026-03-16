# v0.2.0 — Terminal Engine Migration

## Vision

Replace arcterm's custom terminal internals (`arcterm-core`, `arcterm-vt`, `arcterm-pty`) with `alacritty_terminal`, a battle-tested Rust terminal emulation crate. This eliminates 17+ open bugs, replaces the O(n×rows) grid with a ring buffer, and gives arcterm proven VT220/xterm compliance — while preserving full control of the wgpu renderer and the OSC 7770 structured content protocol.

## Motivation

Analysis of [Calyx](https://github.com/yuuichieguchi/Calyx) (a Swift terminal wrapping libghostty) revealed the core lesson: performance comes from leveraging proven terminal engines, not building from scratch. Arcterm's custom VT+grid+PTY layer has accumulated critical bugs (ISSUE-014 panics, O(n×rows) scroll in ISSUE-010) and lacks features mature emulators provide (mouse modes, alt-screen variants, proper resize handling). Rather than fixing these one by one, replacing the layer eliminates entire bug classes at once.

## What Changes

- Terminal emulation, grid, scrollback, PTY handling → `alacritty_terminal`
- OSC 7770 interception → pre-filter on PTY byte stream (before alacritty sees it)
- Renderer data source → reads from alacritty's grid instead of custom `Grid`
- Three crates deleted: `arcterm-core`, `arcterm-vt`, `arcterm-pty`
- Two renderer fixes: per-pane dirty cache, frame pacing/VSync

## What Stays the Same

- `arcterm-render` (wgpu + glyphon pipeline, 4-phase render)
- `arcterm-plugin` (WASM plugin system)
- `arcterm-app` (event loop, pane tree, AI detection, config)
- OSC 7770 protocol and structured content rendering
- Kitty graphics protocol support

## Decisions Made

| Decision | Choice | Alternatives Considered |
|---|---|---|
| Migration target | `alacritty_terminal` crate | libghostty via FFI (fights custom renderer), surgical fixes only (doesn't eliminate bug classes) |
| Crate disposition | Remove `arcterm-core`, `arcterm-vt`, `arcterm-pty` entirely | Keep as adapter layers (unnecessary indirection) |
| OSC 7770 strategy | Pre-filter PTY byte stream | Hook alacritty's event system (less control), post-process grid (lossy) |
| Renderer approach | Keep wgpu+glyphon, rewire data source, fix two issues | Replace glyphon with atlas (separate project), port alacritty's GL renderer (scope explosion) |
| Phasing | Two phases: swap then optimize | Single phase (no checkpoint), three phases (too slow) |

---

## Phase 12: Engine Swap

**Goal:** Replace `arcterm-core`, `arcterm-vt`, and `arcterm-pty` with `alacritty_terminal` while maintaining functional parity.

### Tasks (sequential)

1. **Add `alacritty_terminal` dependency** — Add the crate to the workspace. Pin a specific version. Verify it builds alongside existing code.

2. **Build the OSC 7770 pre-filter** — A byte stream scanner that sits between PTY reads and alacritty's input. Scans for `ESC ] 7770` sequences, extracts them into a side channel, passes all other bytes through unmodified. Also intercepts Kitty graphics APC sequences (replacing the existing `ApcScanner`). Must handle partial sequences across read boundaries.

3. **Wire alacritty_terminal into `arcterm-app`** — Replace `Terminal` (which wraps `PtySession` + `GridState`) with alacritty's `Term<T>` + its PTY. Update `AppState` to own an alacritty `Term` per pane. Feed PTY output through the pre-filter, then into alacritty. Map arcterm's input handling to alacritty's input API.

4. **Rewire the renderer to read alacritty's grid** — Update `arcterm-render` to iterate alacritty's `term.grid()` instead of the custom `Grid`. Map alacritty's `Cell` type (character, fg, bg, flags) to the quad + text renderer inputs. Handle cursor position, selection, and scrollback viewport from alacritty's types.

5. **Reconnect AI features** — Rewire AI agent detection (process info from alacritty's PTY child), context sharing (CWD, exit code from alacritty's event system), and structured content accumulation (fed by the pre-filter's side channel).

6. **Remove old crates** — Delete `arcterm-core`, `arcterm-vt`, `arcterm-pty` from the workspace. Remove all references from `Cargo.toml`. Verify build, test, clippy pass.

### Success Criteria

- `ls`, `vim`, `top`, `htop`, `tmux` render correctly
- OSC 7770 structured content still renders (code blocks, diffs, markdown)
- Kitty inline images still display
- Multi-pane splits work with independent PTY sessions
- AI agent detection still works
- All existing `arcterm-app` and `arcterm-render` tests pass (or are updated for new types)
- `arcterm-core`, `arcterm-vt`, `arcterm-pty` directories no longer exist
- No panics from grid operations (ISSUE-007 through ISSUE-014 class eliminated)

### Risks

- Alacritty's `Term` API may not expose everything arcterm needs (e.g., per-cell attribute access). **Mitigation:** Audit the API before starting task 3.
- Pre-filter must handle split reads (OSC 7770 across two 16KB chunks). **Mitigation:** Stateful scanner with explicit buffer, modeled on existing `ApcScanner`.
- Alacritty's event model may differ from arcterm's current `mpsc` PTY channel. **Mitigation:** Task 3 includes mapping the event flow.

---

## Phase 13: Renderer Optimization

**Goal:** Fix the two known renderer performance issues now that the engine swap is stable.

### Tasks

1. **Per-pane dirty-row cache** — Extend `TextRenderer`'s `row_hashes: Vec<u64>` to multi-pane via `HashMap<PaneId, Vec<u64>>`. Each pane gets its own hash vector. Only re-shape rows whose hash changed. Cursor row always re-shaped. Evict entries when panes close. Invalidate on pane resize.

2. **Frame pacing / VSync** — Add proper frame rate management:
   - Use wgpu's `PresentMode::Fifo` (VSync) or `Mailbox` (adaptive) instead of `Immediate`
   - Coalesce rapid PTY output into a single frame (multiple `try_recv`, one `request_redraw`)
   - Always redraw immediately on keyboard input (preserve input latency)
   - Cap idle redraws to display refresh rate via `winit` `ControlFlow::WaitUntil`

3. **Verify performance** — Measure key-to-screen latency, frame rate during `cat` flood, memory per pane. Compare against Phase 12 baseline. Establish measurement baseline.

### Success Criteria

- Multi-pane rendering only re-shapes rows that actually changed
- `cat /dev/urandom | head -c 10M` in one pane does not cause visible lag in adjacent pane
- No tearing or double-buffering artifacts
- Frame rate capped to display refresh rate during idle
- Latency measurement exists for key-to-screen

### Risks

- Per-pane hash cache must handle resize (invalidate all hashes). Low risk.
- Frame pacing interacts with PTY drain timing. **Mitigation:** Always redraw immediately on keyboard input; only coalesce PTY-triggered redraws.

---

## Phase 14: Remaining Stabilization

**Goal:** Fix surviving issues from v0.1.1 that were not eliminated by the engine swap. These affect `arcterm-app`, `arcterm-render`, and `arcterm-plugin`.

### Surviving App/Render Issues

- ISSUE-002: Add `request_redraw()` after keyboard input
- ISSUE-003: Handle Ctrl+\ (0x1c) and Ctrl+] (0x1d)
- ISSUE-004: Graceful error on terminal creation failure
- ISSUE-005: Display "Shell exited" indicator
- ISSUE-006: Visible cursor on blank cells

### Surviving Plugin Issues

- H-1: Epoch-increment background task for WASM runtime
- H-2: Real WASM function dispatch in `call_tool()`
- M-1: Fix `KeyInput` event kind mapping
- M-2: Validate plugin manifest `wasm` field for path traversal
- M-6: Guard plugin file copy against symlink following
- ISSUE-015: Fix backslash validation test
- ISSUE-016: Epoch ticker thread cleanup
- ISSUE-017: Double-lock TOCTOU in `call_tool`
- ISSUE-018: Canonicalize fallback in `load_from_dir`

### Surviving Config/Runtime Issues

- M-3: Async Kitty image decode
- M-5: GPU init returns Result instead of panicking
- ISSUE-019: Window creation `.expect()` → graceful error

### Notes

- M-4 (scrollback cap) becomes trivial — alacritty_terminal has its own config for this
- ISSUE-007 through ISSUE-014 eliminated by Phase 12
- ISSUE-001 (PTY writer shutdown) eliminated by Phase 12
- ISSUE-011 through ISSUE-013 (VT parser) eliminated by Phase 12

---

## Impact on Existing Roadmap

- **v0.1.1 Phase 9** (grid, VT, PTY, plugin fixes): Skip entirely — grid/VT/PTY crates are deleted
- **v0.1.1 Phase 10** (app input/UX fixes): Absorbed into Phase 14
- **v0.1.1 Phase 11** (config/runtime hardening): Absorbed into Phase 14
- **v0.2.0 Phases 12-14** replace the v0.1.1 stabilization approach

### New Milestone Order

| Phase | Description | Depends On |
|---|---|---|
| 12 | Engine swap (alacritty_terminal migration) | v0.1.0 complete |
| 13 | Renderer optimization (dirty cache, frame pacing) | Phase 12 |
| 14 | Remaining stabilization (app, plugin, runtime fixes) | Phase 12 |

Phases 13 and 14 are independent of each other and can execute in parallel.
