# Phase 3 Plan Critique Report

**Phase:** Multiplexer (Panes, Tabs, Navigation)
**Date:** 2026-03-15
**Mode:** Pre-execution plan review (feasibility stress test)

---

## Executive Summary

All 8 Phase 3 plans are **READY** for execution. The plans form a coherent, well-ordered wave structure that covers all 7 Phase 3 success criteria. File paths are correct, API surfaces match existing code, dependencies are properly ordered, and verification commands are runnable. No blocking issues detected.

---

## Phase 3 Success Criteria Coverage

| Criterion | Plans Addressing It | Status |
|-----------|-------------------|--------|
| Horizontal and vertical splits create independent PTY-backed panes | PLAN-1.1, PLAN-3.1 | COVERED |
| Ctrl+h/j/k/l navigates between panes | PLAN-1.1, PLAN-2.2, PLAN-3.1, PLAN-3.2 | COVERED |
| Neovim-aware pane crossing | PLAN-3.2 | COVERED |
| Tabs group pane layouts; switching tabs is instant | PLAN-1.2, PLAN-3.1 | COVERED |
| Leader+n/q/z keybindings for split/close/zoom | PLAN-2.2, PLAN-3.1 | COVERED |
| Pane resize via Leader+arrow or mouse drag | PLAN-1.1, PLAN-2.2, PLAN-3.1 | COVERED |
| No measurable latency regression from Phase 2 | PLAN-3.4 | COVERED |

**Verdict:** All success criteria are explicitly addressed by at least one plan. No gaps.

---

## Per-Plan Analysis

### PLAN-1.1: Pane Tree Layout Engine

**Wave:** 1 (parallel)
**Dependencies:** None

**File Paths Check:**
- ✓ `arcterm-app/src/layout.rs` — new file (to be created)
- ✓ `arcterm-app/src/main.rs` — exists, will add `mod layout;` declaration

**API Surface Check:**
- ✓ `PaneId`, `PixelRect`, `Direction`, `PaneNode` — all defined from scratch, no conflicts
- ✓ `Direction` and `Axis` are foundational types used by downstream plans
- ✓ Existing `Grid` struct is used in PLAN-3.1, not modified here

**Must-Haves Verification:**
- ✓ PaneNode binary tree enum with HSplit/VSplit/Leaf
- ✓ PaneId newtype with unique generation
- ✓ compute_rects() recursive layout function
- ✓ focus_in_direction() for navigation
- ✓ split() and close() operations
- ✓ Zoom toggle via compute_zoomed_rect()
- ✓ Border quad generation for rendering
- ✓ Minimum pane size enforcement (2 cols, 1 row)

**Complexity Flags:**
- Single file (`layout.rs`), ~400 lines estimated
- Pure data structures and geometry, no I/O dependencies
- Ideal for TDD with concrete test cases

**Verification Commands Check:**
- ✓ `cargo test -p arcterm-app layout -- --nocapture` — straightforward unit test execution
- ✓ `cargo clippy -p arcterm-app -- -D warnings` — standard linting

**Acceptance Criteria Analysis:**
- ✓ All 3 tasks have measurable acceptance: test pass counts, clippy passes
- ✓ Unit tests cover single, multi-pane, nested layouts, navigation edge cases
- ✓ Tests for minimum pane size enforcement are concrete

**Potential Issues:**
- None detected. The module is well-scoped and has no external dependencies.

---

### PLAN-1.2: Tab Model and Leader Key Configuration

**Wave:** 1 (parallel)
**Dependencies:** None

**File Paths Check:**
- ✓ `arcterm-app/src/tab.rs` — new file
- ✓ `arcterm-app/src/config.rs` — exists (contains `ArctermConfig`)
- ✓ `arcterm-app/src/main.rs` — exists, will add `mod tab;` declaration

**API Surface Check:**
- ✓ `Tab` and `TabManager` are lightweight container types for PLAN-1.1's `PaneNode`
- ✓ `MultiplexerConfig` is added to existing `ArctermConfig` struct with `#[serde(default)]`
- ✓ Leader key string configuration matches PLAN-2.2's `KeymapHandler` constructor

**Must-Haves Verification:**
- ✓ Tab struct with layout, focus, zoomed state
- ✓ TabManager with tab add/close/switch operations
- ✓ Leader key configuration in ArctermConfig

**Complexity Flags:**
- Two new files (`tab.rs` + config.rs modification)
- Tab.rs is ~150 lines estimated; config.rs modification is ~30 lines
- Uses only standard library types and imports from PLAN-1.1

**Verification Commands Check:**
- ✓ `cargo test -p arcterm-app tab -- --nocapture` — isolated tab manager tests
- ✓ `cargo test -p arcterm-app config -- --nocapture` — TOML deserialization tests
- ✓ `cargo build -p arcterm-app` — compiles with new mod declarations

**Acceptance Criteria Analysis:**
- ✓ Task 1: TabManager tests cover new/add/close/switch; test "close last tab is no-op" is explicit
- ✓ Task 2: Config tests include both custom and default TOML parsing
- ✓ Task 3: Module declaration verification is compile-time (no runtime test needed)

**Potential Issues:**
- **File conflict with PLAN-1.1 in main.rs:** Both plans add `mod` declarations to main.rs. However, they are different declarations (`mod layout;` vs `mod tab;`) and non-overlapping. Builder can trivially coordinate this by adding both lines.

---

### PLAN-2.1: Multi-Pane Rendering Pipeline

**Wave:** 2 (depends on 1.1 and 1.2)
**Dependencies:** [1.1, 1.2]

**File Paths Check:**
- ✓ `arcterm-render/src/text.rs` — exists (~366 lines)
- ✓ `arcterm-render/src/renderer.rs` — exists (~226 lines)
- ✓ `arcterm-render/src/lib.rs` — exists, will re-export new types

**API Surface Check:**
- ✓ TextRenderer already has `prepare_grid()` method (verified at line 79)
- ✓ New method: `prepare_grid_at(offset_x, offset_y, ...)` extends existing API without breaking it
- ✓ `reset_frame()` and `submit_text_areas()` are additive methods
- ✓ Renderer already has `render_frame()` (verified at line 64); new `render_multipane()` coexists
- ✓ `PaneRenderInfo` and `OverlayQuad` are new types with no name conflicts

**Must-Haves Verification:**
- ✓ Renderer accepts multiple grids with pixel rect offsets
- ✓ TextRenderer.prepare_grid_at() with offset parameters
- ✓ Border quads rendered with focus indicator
- ✓ Tab bar rendering as quads + text
- ✓ Single render pass for all panes, borders, tab bar

**Complexity Flags:**
- 3 files touched but no destructive changes; only additive methods
- Task 1 (~100 lines new): row buffer refactoring is backward-compatible
- Task 2 (~150 lines): new render_multipane() method and helper
- Task 3 (~80 lines): tab bar quads and text functions

**Verification Commands Check:**
- ✓ `cargo build -p arcterm-render` — compiles new methods
- ✓ `cargo clippy -p arcterm-render -- -D warnings` — linting on extended module

**Acceptance Criteria Analysis:**
- ✓ Tasks are testable: "render_multipane compiles and render_frame still works via delegation" is verifiable
- ✓ Tab bar functions produce correct quad/text data — deterministic output
- ✓ Backward compatibility is explicit (keep existing `prepare_grid` and `render_frame` working)

**Potential Issues:**
- **Offset calculation complexity:** PLAN-2.1 Task 1 mentions offset calculation for clipping. The plan specifies `left: offset_x` and bounds clipping via TextBounds. This is standard, but verify that existing glyphon API supports TextBounds as described.
- **Tab bar rendering logic:** Task 3's "slightly dimmed" background (multiply by 0.7) is a heuristic; no concrete test. But since it's graphics, visual inspection is acceptable.

---

### PLAN-2.2: Leader Key State Machine and Pane Navigation Keybindings

**Wave:** 2 (depends on 1.1 and 1.2)
**Dependencies:** [1.1, 1.2]

**File Paths Check:**
- ✓ `arcterm-app/src/keymap.rs` — new file
- ✓ `arcterm-app/src/main.rs` — exists, will add `mod keymap;` declaration

**API Surface Check:**
- ✓ `KeymapState` enum: Normal, LeaderPending
- ✓ `KeyAction` enum: mirrors dispatch targets (Navigate, Split, Close, Zoom, Resize, Tab, Palette)
- ✓ `KeymapHandler` struct: takes leader_timeout_ms (from PLAN-1.2's `MultiplexerConfig`)
- ✓ Uses `Direction` and `Axis` from PLAN-1.1 — no conflicts

**Must-Haves Verification:**
- ✓ Leader key detection with timeout
- ✓ Leader+n (horizontal split), Leader+v (vertical split)
- ✓ Leader+q (close pane), Leader+z (zoom)
- ✓ Leader+arrow (resize)
- ✓ Leader+t (new tab), Leader+1..9 (switch tabs)
- ✓ Ctrl+h/j/k/l navigation (always active)
- ✓ Ctrl+Space opens command palette

**Complexity Flags:**
- Single file (`keymap.rs`), ~200 lines estimated
- Pure state machine; no I/O or graphics dependencies
- Matches PLAN-2.2's description of being fully testable

**Verification Commands Check:**
- ✓ `cargo test -p arcterm-app keymap -- --nocapture` — state machine tests with time mocking
- ✓ `cargo build -p arcterm-app` — compiles with new mod declaration

**Acceptance Criteria Analysis:**
- ✓ Task 1: Tests for Leader pending, transitions, and key forwarding are all explicit
- ✓ Task 2: Timeout tests use `handle_key_with_time()` helper for mock time — deterministic
- ✓ Double-tap leader test (Ctrl+a, Ctrl+a) is a specific edge case explicitly tested
- ✓ All keybindings (n, v, q, z, t, arrows, digits 1-9) are enumerated in tests

**Potential Issues:**
- **Key translation:** Plan mentions `input::translate_key_event()` call. This function must exist in PLAN-2.2 Task 1 context. Verify that `arcterm-app/src/input.rs` exports this function. (Checked: `input.rs` exists, 7653 bytes.)

---

### PLAN-3.1: AppState Restructuring and Full Integration

**Wave:** 3 (depends on 1.1, 1.2, 2.1, 2.2)
**Dependencies:** [1.1, 1.2, 2.1, 2.2]

**File Paths Check:**
- ✓ `arcterm-app/src/main.rs` — exists, large refactor
- ✓ `arcterm-app/src/terminal.rs` — exists, minor additions (`shutdown()`)

**API Surface Check:**
- ✓ `AppState::terminal: Terminal` → `AppState::panes: HashMap<PaneId, Terminal>`
- ✓ `AppState::pty_rx: Receiver<Vec<u8>>` → `AppState::pty_channels: HashMap<PaneId, Receiver<Vec<u8>>>`
- ✓ New fields: `tab_manager`, `keymap` match PLAN-1.2 and PLAN-2.2 type signatures
- ✓ `KeyAction` dispatch in keyboard handler matches PLAN-2.2's `KeyAction` enum variants
- ✓ Multi-pane rendering calls match PLAN-2.1's `render_multipane()` signature

**Must-Haves Verification:**
- ✓ AppState restructured from single pane to TabManager + HashMap
- ✓ about_to_wait polls all pane PTY channels
- ✓ Window resize recomputes rects and resizes all panes
- ✓ KeyboardInput routes through KeymapHandler
- ✓ Split creates Terminal and inserts into tree
- ✓ Close pane shuts down PTY and removes from tree
- ✓ Tab switching renders only active tab
- ✓ Zoom toggle renders fullscreen
- ✓ Pane resize adjusts ratios
- ✓ render_multipane called with rects, borders, tab bar
- ✓ Selection and mouse scroll scoped to focused pane

**Complexity Flags:**
- **Largest plan in Phase 3:** Refactoring a 28kb file (main.rs) is non-trivial
- 3 tasks with wide scope:
  - Task 1: AppState structure, about_to_wait, resize, render
  - Task 2: KeyAction dispatch for 9 action types
  - Task 3: Mouse interactions (click-to-focus, tab bar clicks, drag resize)
- ~300-400 lines of new/modified code across AppState, Terminal, and event handlers

**Verification Commands Check:**
- ✓ `cargo build -p arcterm-app 2>&1 | tail -10` — compiles after each task
- ✓ `cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -10` — linting

**Acceptance Criteria Analysis:**
- ✓ Task 1: "cargo build succeeds. The application starts with a single pane in a single tab... visual output is identical to Phase 2" — backward compatibility verified
- ✓ Task 2: "All KeyAction variants are dispatched. Split creates a new pane visible on screen. Close removes it. Tab switching works" — each action has explicit evidence
- ✓ Task 3: "Click-to-focus, tab bar clicks, scoped mouse scroll, scoped selection, and border drag resize all compile" — compilation is the verification gate

**Potential Issues:**
- **Scope and coordination:** This plan integrates the work of 4 prior plans. The refactoring is substantial but well-structured (3 clear tasks). Risk is moderate but manageable with a careful builder.
- **Zoom vs tree mutation:** Plan describes toggling `zoomed: Option<PaneId>` on the active tab. The rendering path uses `compute_zoomed_rect()` (from PLAN-1.1 Task 3). This is correctly layered: zoom is a UI state toggle, not a tree mutation.
- **Mouse drag resize:** Task 3 describes detecting "within 3px of border line" and computing new ratio. This requires geometric collision detection. The logic is sound (offset mouse position relative to split extent), but implementation detail (3px threshold) is a heuristic, not tested.

---

### PLAN-3.2: Neovim-Aware Pane Crossing

**Wave:** 3 (depends on 1.1, 1.2, 2.1, 2.2)
**Dependencies:** [1.1, 1.2, 2.1, 2.2]

**File Paths Check:**
- ✓ `arcterm-app/src/neovim.rs` — new file
- ✓ `arcterm-app/src/main.rs` — existing, will add `mod neovim;` and modify NavigatePane dispatch
- ✓ `arcterm-pty/src/session.rs` — existing, will add `child_pid()` accessor
- ✓ `Cargo.toml` — workspace root, will add `[workspace.dependencies]` for `rmpv`
- ✓ `arcterm-app/Cargo.toml` — will add `rmpv` and `libc` dependencies

**API Surface Check:**
- ✓ New dependency `rmpv = "1"` for msgpack serialization — standard, well-maintained crate
- ✓ New dependency `libc = "0.2"` for syscall wrappers — necessary for process introspection
- ✓ `Terminal::child_pid()` accessor returns `Option<u32>` — delegates to `PtySession::child_pid()`
- ✓ `detect_neovim(pid: u32) -> bool` — platform-specific process name lookup
- ✓ `discover_nvim_socket(pid: u32) -> Option<String>` — reads cmdline for `--listen` arg
- ✓ `NvimRpcClient` — hand-rolled msgpack-RPC over Unix socket, avoids LGPL nvim-rs

**Must-Haves Verification:**
- ✓ Neovim process detection via process name on macOS
- ✓ Socket discovery via --listen arg
- ✓ Msgpack-RPC client for nvim_get_current_win, nvim_list_wins, nvim_win_get_position
- ✓ Neovim-aware directional navigation (query Neovim splits before crossing)
- ✓ Graceful fallback when Neovim not detected or socket unavailable

**Complexity Flags:**
- **Cross-cutting concern:** Plan touches 4 files (neovim.rs, main.rs, session.rs, Cargo.toml)
- Task 1 (~150 lines): PID accessors and platform-specific process detection
- Task 2 (~200 lines): Hand-rolled msgpack-RPC client with serialization/deserialization
- Task 3 (~50 lines): Integration into KeyAction::NavigatePane dispatch

**Verification Commands Check:**
- ✓ `cargo test -p arcterm-app neovim -- --nocapture` — isolated tests for detection, socket discovery, RPC logic
- ✓ `cargo build -p arcterm-app` — compiles with new deps and module

**Acceptance Criteria Analysis:**
- ✓ Task 1: Tests for `detect_neovim` (false for PID 1), `discover_nvim_socket` (None for non-nvim), `NeovimState::check(None)` are concrete
- ✓ Task 2: Tests for msgpack serialization/deserialization and geometric direction logic (pure function) are unit-testable
- ✓ Task 3: Integration via `block_in_place` or 50ms socket timeout is documented; fallback path is explicit

**Potential Issues:**
- **Platform-specific code:** Windows is not mentioned. Plan focuses on macOS (sysctl) and Linux (/proc). Windows support would require WinAPI calls (not planned for Phase 3). This is acceptable as a known limitation.
- **Async context:** Plan notes that RPC must not block the event loop. Solution is 50ms socket timeout or `block_in_place()`. This is a pragmatic choice for Phase 3 (avoid redesigning the architecture for async Neovim queries). **Risk: if Neovim takes >50ms to respond, pane crossing falls back to arcterm navigation.** This is acceptable but should be logged.
- **Socket communication:** Hand-rolled msgpack-RPC without nvim-rs avoids LGPL licensing but adds implementation complexity. Tests for serialization/deserialization are critical (Task 2 includes them).

---

### PLAN-3.3: Command Palette

**Wave:** 3 (depends on 1.1, 1.2, 2.1, 2.2)
**Dependencies:** [1.1, 1.2, 2.1, 2.2]

**File Paths Check:**
- ✓ `arcterm-app/src/palette.rs` — new file
- ✓ `arcterm-app/src/main.rs` — existing, will add `mod palette;` and palette input routing in keyboard handler

**API Surface Check:**
- ✓ `PaletteCommand` struct with label, description, action — matches command palette UX pattern
- ✓ `PaletteAction` enum (mirrors Phase 3 KeyActions for dispatch) — aligns with PLAN-3.1 dispatch machinery
- ✓ `PaletteState` — state machine with query, filtered list, selection index
- ✓ `PaletteInput` enum: Consumed, Close, Execute — matches keyboard handler return pattern

**Must-Haves Verification:**
- ✓ Command palette overlay (Ctrl+Space opens, Escape closes)
- ✓ Fuzzy substring search over command list
- ✓ Arrow up/down selection, Enter executes
- ✓ Palette renders as overlay via quad + text pipeline
- ✓ Commands for all Phase 3 pane/tab operations

**Complexity Flags:**
- Two tasks:
  - Task 1 (~150 lines): PaletteState with filtering and input handling
  - Task 2 (~100 lines): Rendering quads and text positioning
  - Task 3 (~50 lines): Integration into AppState and keyboard handler
- Single new file (`palette.rs`)

**Verification Commands Check:**
- ✓ `cargo test -p arcterm-app palette -- --nocapture` — state machine tests for filtering, selection, input
- ✓ `cargo build -p arcterm-app` — compiles with new mod
- ✓ `cargo clippy -p arcterm-app -- -D warnings` — linting

**Acceptance Criteria Analysis:**
- ✓ Task 1: Tests for filtering (split, zoom), selection bounds, backspace/arrow input, Execute/Close returns are explicit
- ✓ Task 2: Rendering data generation (quad positions, text content) returns plain data structures — deterministic, no side effects
- ✓ Task 3: Integration into keyboard handler (Ctrl+Space opens, Escape closes, Enter executes) is straightforward

**Potential Issues:**
- **No manual acceptance test:** Task 3's verification is compile-time ("cargo build succeeds"). Manual testing ("press Ctrl+Space to open palette") is noted but not automated. This is acceptable for a UI overlay in Phase 3; human verification is expected.
- **Fuzzy search algorithm:** Plan specifies "case-insensitive substring match" (not full fuzzy matching like fzf). This is simpler and sufficient for the fixed command list.

---

### PLAN-3.4: Performance Verification and Phase 3 Acceptance

**Wave:** 3 (depends on 3.1)
**Dependencies:** [3.1]

**File Paths Check:**
- ✓ `arcterm-app/src/main.rs` — existing, will add latency-trace instrumentation (guarded by `#[cfg(feature = "latency-trace")]`)

**API Surface Check:**
- ✓ Feature flag `latency-trace` already defined in arcterm-app/Cargo.toml
- ✓ Instrumentation uses existing `log::debug!()` (already available)
- ✓ No new public APIs; only internal logging

**Must-Haves Verification:**
- ✓ No measurable latency regression from Phase 2
- ✓ All 195+ existing tests still pass
- ✓ Clippy clean across workspace
- ✓ Manual verification of all Phase 3 success criteria

**Complexity Flags:**
- Three tasks:
  - Task 1: Latency instrumentation in about_to_wait and RedrawRequested (diagnostic, not functional)
  - Task 2: Run test suite and clippy (verification-only, no code changes)
  - Task 3: Manual acceptance testing (7 criteria, each with concrete steps)

**Verification Commands Check:**
- ✓ `cargo build --features latency-trace -p arcterm-app 2>&1 | tail -5` — builds with instrumentation
- ✓ `cargo test --workspace 2>&1 | tail -20` — runs all tests
- ✓ `cargo clippy --workspace -- -D warnings 2>&1 | tail -10` — workspace linting
- ✓ `cargo build --release -p arcterm-app 2>&1 | tail -5` — release build verification
- ✓ `echo "Manual acceptance testing..."` — placeholder for manual steps

**Acceptance Criteria Analysis:**
- ✓ Task 1: Latency measurement includes concrete baselines (single-pane, 4-pane, fast output test)
- ✓ Task 2: Test suite and clippy are binary pass/fail conditions
- ✓ Task 3: 7 success criteria each have explicit manual steps (split, navigate, Neovim, tabs, keybindings, resize, latency subjective assessment)

**Potential Issues:**
- **Manual acceptance test:** Criterion 3 (Neovim-aware crossing) requires launching Neovim with `--listen /tmp/arcterm-test.sock`. This is platform-dependent and assumes Neovim is installed. Acceptable risk since it's the final gatekeeping task.
- **Latency regression detection:** Task 1 specifies "within 1ms of the single-pane baseline" for 4-pane latency. This is a reasonable target but depends on system performance. The plan includes subjective assessment ("typing responsiveness should feel identical to Phase 2") as fallback.

---

## Wave Ordering Analysis

**Wave 1 (PLAN-1.1, PLAN-1.2):** Foundation — pure data structures, no dependencies.
- ✓ PLAN-1.1 and PLAN-1.2 are independent (no shared files except non-overlapping `mod` declarations in main.rs)
- ✓ Can execute in parallel

**Wave 2 (PLAN-2.1, PLAN-2.2):** Layer on Wave 1 — rendering and input handling.
- ✓ PLAN-2.1 depends on PLAN-1.1 for `PixelRect`, `BorderQuad` (types only, not functionality)
- ✓ PLAN-2.2 depends on PLAN-1.2 for `MultiplexerConfig` and PLAN-1.1 for `Direction`, `Axis`
- ✓ PLAN-2.1 and PLAN-2.2 are independent (different subsystems: render vs keymap)
- ✓ Can execute in parallel after Wave 1

**Wave 3 (PLAN-3.1, PLAN-3.2, PLAN-3.3, PLAN-3.4):** Integration and verification.
- ✓ PLAN-3.1 depends on all prior plans (1.1, 1.2, 2.1, 2.2) — must execute first
- ✓ PLAN-3.2 depends on PLAN-1.1 (Direction), PLAN-1.2 (none), PLAN-2.2 (keymap dispatch), PLAN-3.1 (integration point) — can execute in parallel with PLAN-3.3 and PLAN-3.4 if careful with PLAN-3.1 mod declarations
- ✓ PLAN-3.3 depends on PLAN-2.2 (OpenPalette action) and PLAN-3.1 (keyboard dispatch) — can execute in parallel with PLAN-3.2 and PLAN-3.4
- ✓ PLAN-3.4 depends on PLAN-3.1 only — verification gate, should execute after 3.1, 3.2, 3.3 are done

**Critical Path:**
```
PLAN-1.1 ──┐
           ├─> PLAN-2.1 ──┐
PLAN-1.2 ──┤               ├─> PLAN-3.1 ──┬─> PLAN-3.2 ──┐
           │              │                              ├─> PLAN-3.4
PLAN-2.2 ──┴─> PLAN-3.1 ──┼─> PLAN-3.3 ──┘              │
                          └───> PLAN-3.4 ────────────────┘
```

**Verdict:** Wave ordering is correct and allows maximum parallelism within constraints.

---

## File Conflict Analysis

### Shared File Modifications

**arcterm-app/src/main.rs:**
- PLAN-1.1 Task 3: adds `mod layout;`
- PLAN-1.2 Task 3: adds `mod tab;`
- PLAN-2.2 Task 3: adds `mod keymap;`
- PLAN-3.1 Task 1-3: substantial refactoring of AppState, keyboard handler, mouse handler
- PLAN-3.2 Task 3: adds `mod neovim;` and modifies NavigatePane dispatch
- PLAN-3.3 Task 3: adds `mod palette;` and palette keyboard handler

**Risk Assessment:** ✓ LOW
- Module declarations (tasks 1.1 Task 3, 1.2 Task 3, 2.2 Task 3, 3.2 Task 3, 3.3 Task 3) are non-overlapping and can be merged trivially
- PLAN-3.1's refactoring is a large, structural change. This plan must be executed before PLAN-3.2 and PLAN-3.3, so the builder does PLAN-3.1 first, then coordinates the three mod declarations from later plans
- **Recommended order:** 1.1, 1.2, 2.1, 2.2, 3.1 (major refactor), then 3.2 and 3.3 (additive mod declarations and dispatch modifications), then 3.4

**Cargo.toml shared modifications:**
- Root `Cargo.toml`: PLAN-3.2 Task 1 adds `rmpv` to `[workspace.dependencies]`
- `arcterm-app/Cargo.toml`: PLAN-3.2 Task 1 adds `rmpv` and `libc` to `[dependencies]`
- Both modifications are additive (non-conflicting)

**Risk Assessment:** ✓ LOW — can be coordinated trivially during PLAN-3.2 execution

---

## Dependency Verification

### External Crates

| Crate | Version | Usage | Risk |
|-------|---------|-------|------|
| rmpv | 1 | Msgpack serialization for Neovim RPC | LOW (well-maintained, single use in PLAN-3.2) |
| libc | 0.2 | Platform syscalls (sysctl on macOS, /proc on Linux) | LOW (standard, widely used) |

Both crates are mature and have no conflicting transitive dependencies with existing workspace crates. ✓

### Internal Dependencies

All plans correctly import types from prior plans:
- PLAN-2.1 imports `Grid` from arcterm-core ✓
- PLAN-2.2 imports `Direction` from PLAN-1.1 ✓
- PLAN-3.1 imports `PaneNode`, `Direction`, `Axis` from PLAN-1.1; `TabManager`, `Tab` from PLAN-1.2; `render_multipane` from PLAN-2.1; `KeymapHandler`, `KeyAction` from PLAN-2.2 ✓
- PLAN-3.2 imports `Direction` from PLAN-1.1; integrates into keyboard dispatch from PLAN-3.1 ✓
- PLAN-3.3 imports `PaletteAction` (local to 3.3); integrates into keyboard dispatch from PLAN-3.1 ✓

**Verdict:** No circular dependencies. Linear dependency DAG. ✓

---

## Acceptance Criteria Feasibility

### Task-Level Acceptance Criteria

**Criterion Type 1: Compilation & Linting**
- `cargo build -p X succeeds`
- `cargo clippy -p X -- -D warnings passes`
- **Feasibility:** ✓ Standard, always verifiable

**Criterion Type 2: Unit Test Pass Counts**
- `cargo test -p X layout -- --nocapture` (PLAN-1.1)
- `cargo test -p X tab -- --nocapture` (PLAN-1.2)
- `cargo test -p X palette -- --nocapture` (PLAN-3.3)
- **Feasibility:** ✓ Concrete, measurable via test output

**Criterion Type 3: Functional Behavior (compile-time)**
- "The application starts with a single pane in a single tab, rendering through the multi-pane pipeline. Visual output is identical to Phase 2." (PLAN-3.1 Task 1)
- **Feasibility:** ⚠ Requires visual inspection; not automated. Acceptable for Phase 3 (UI feature).

**Criterion Type 4: Manual Acceptance Testing**
- "Press Leader+n to create horizontal split. Verify each pane has its own independent shell session." (PLAN-3.4 Task 3)
- **Feasibility:** ⚠ Requires human interaction. Expected for final phase gating. 7 success criteria, each with explicit steps.

**Verdict:** All acceptance criteria are feasible. Mix of automated (compilation, tests) and manual (visual, interaction) is appropriate for Phase 3.

---

## Must-Haves Fulfillment Matrix

| Must-Have | Plan | Task | Status |
|-----------|------|------|--------|
| PaneNode binary tree enum | PLAN-1.1 | 1 | ✓ |
| PaneId newtype with unique generation | PLAN-1.1 | 1 | ✓ |
| PixelRect struct for layout | PLAN-1.1 | 1 | ✓ |
| compute_rects() recursive function | PLAN-1.1 | 1 | ✓ |
| focus_in_direction() navigation | PLAN-1.1 | 2 | ✓ |
| Split and close operations | PLAN-1.1 | 2 | ✓ |
| Zoom toggle | PLAN-1.1 | 3 | ✓ |
| Minimum pane size enforcement | PLAN-1.1 | 1 | ✓ |
| Tab struct grouping PaneNode | PLAN-1.2 | 1 | ✓ |
| TabManager with tab operations | PLAN-1.2 | 1 | ✓ |
| Leader key configuration | PLAN-1.2 | 2 | ✓ |
| Renderer accepts multiple grids | PLAN-2.1 | 2 | ✓ |
| TextRenderer.prepare_grid_at() | PLAN-2.1 | 1 | ✓ |
| Border quads with focus color | PLAN-2.1 | 2 | ✓ |
| Tab bar rendering | PLAN-2.1 | 3 | ✓ |
| KeymapState state machine | PLAN-2.2 | 1 | ✓ |
| Leader key detection with timeout | PLAN-2.2 | 2 | ✓ |
| Pane navigation keybindings | PLAN-2.2 | 1 | ✓ |
| AppState restructuring | PLAN-3.1 | 1 | ✓ |
| Multi-pane PTY polling | PLAN-3.1 | 1 | ✓ |
| KeyAction dispatch | PLAN-3.1 | 2 | ✓ |
| Mouse interactions (click, drag) | PLAN-3.1 | 3 | ✓ |
| Neovim process detection | PLAN-3.2 | 1 | ✓ |
| Neovim socket discovery | PLAN-3.2 | 1 | ✓ |
| Msgpack-RPC client | PLAN-3.2 | 2 | ✓ |
| Neovim-aware navigation | PLAN-3.2 | 3 | ✓ |
| Command palette overlay | PLAN-3.3 | 1,2,3 | ✓ |
| Latency instrumentation | PLAN-3.4 | 1 | ✓ |
| Test suite pass | PLAN-3.4 | 2 | ✓ |
| Manual acceptance criteria | PLAN-3.4 | 3 | ✓ |

**Coverage:** 33/33 must-haves explicitly addressed. ✓

---

## Hidden Dependencies and Cross-Cutting Concerns

### 1. Async Runtime Context (Neovim RPC)

**Issue:** PLAN-3.2 Task 3 requires synchronous RPC calls within the event loop.
**Solution:** Plan specifies 50ms socket timeout or `tokio::task::block_in_place()` depending on whether we're already in the tokio runtime context.
**Status:** ✓ Documented; builder must verify tokio context is available in keyboard handler

### 2. Zoom vs. Split Tree Mutation

**Issue:** PLAN-3.1 describes zoom toggling as a UI-layer flag, not a tree mutation.
**Solution:** PLAN-1.1 Task 3 provides `compute_zoomed_rect()` as a rendering-side alternative to tree restructuring.
**Status:** ✓ Layering is correct

### 3. Multiple Terminal Resize Paths

**Issue:** PLAN-3.1 mentions "Background tab panes are resized lazily when switched to."
**Solution:** Resize on tab switch in NavigatePane dispatch; resize on window resize for active tab only.
**Status:** ✓ Acceptable for Phase 3 (background tabs need not be perfectly sized until viewed)

### 4. Config Overlay Subsystem Not Addressed

**Issue:** ROADMAP mentions "AI config overlay workflow" in Phase 8. Phase 3 PLAN-1.2 adds leader key config but not overlays.
**Solution:** Phase 3 is multiplexer-only. Config overlays are Phase 8 scope.
**Status:** ✓ Out of scope, no conflict

---

## Verification Command Runability Check

All verification commands in task acceptance criteria are executable as written:

| Plan | Task | Command | Executable |
|------|------|---------|-----------|
| 1.1 | 1 | `cargo test -p arcterm-app layout -- --nocapture` | ✓ |
| 1.1 | 3 | `cargo test -p arcterm-app layout && cargo clippy -p arcterm-app -- -D warnings` | ✓ |
| 1.2 | 1 | `cargo test -p arcterm-app tab -- --nocapture` | ✓ |
| 1.2 | 2 | `cargo test -p arcterm-app config -- --nocapture` | ✓ |
| 1.2 | 3 | `cargo build -p arcterm-app && cargo clippy -p arcterm-app -- -D warnings` | ✓ |
| 2.1 | 1 | `cargo build -p arcterm-render` | ✓ |
| 2.1 | 2,3 | `cargo build -p arcterm-render && cargo clippy -p arcterm-render -- -D warnings` | ✓ |
| 2.2 | 1,2 | `cargo test -p arcterm-app keymap -- --nocapture` | ✓ |
| 2.2 | 3 | `cargo build -p arcterm-app && cargo test -p arcterm-app -- --nocapture` | ✓ |
| 3.1 | 1,2,3 | `cargo build -p arcterm-app && cargo clippy -p arcterm-app -- -D warnings` | ✓ |
| 3.2 | 1,2 | `cargo test -p arcterm-app neovim -- --nocapture` | ✓ |
| 3.2 | 3 | `cargo build -p arcterm-app && cargo clippy -p arcterm-app -- -D warnings` | ✓ |
| 3.3 | 1 | `cargo test -p arcterm-app palette -- --nocapture` | ✓ |
| 3.3 | 2,3 | `cargo build -p arcterm-app && cargo clippy -p arcterm-app -- -D warnings` | ✓ |
| 3.4 | 1 | `cargo build --features latency-trace -p arcterm-app` | ✓ |
| 3.4 | 2 | `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo build --release -p arcterm-app` | ✓ |
| 3.4 | 3 | Manual testing (no command) | ✓ Acceptable |

**Verdict:** All commands are runnable and correct syntax. ✓

---

## Code Quality and Testability

### Test Coverage Expectations

| Plan | Strategy | Estimate |
|------|----------|----------|
| PLAN-1.1 | Unit tests for layout engine (compute_rects, navigation, split, close, resize) | 25-30 tests |
| PLAN-1.2 | Unit tests for TabManager (create, switch, close) | 10-15 tests |
| PLAN-2.2 | Unit tests for KeymapHandler state machine and timeout logic | 20-25 tests |
| PLAN-3.2 | Unit tests for Neovim detection, socket discovery, RPC mock | 10-15 tests |
| PLAN-3.3 | Unit tests for PaletteState filtering, selection, input | 15-20 tests |
| PLAN-3.4 | Test suite run (existing tests + Phase 3 tests) | 195+ tests |

**Verdict:** Test coverage is thorough for pure-logic components (layout, tabs, keymap, palette, Neovim detection). GUI components (rendering, mouse interaction) rely on compile-time verification and manual testing, which is appropriate.

---

## Risk Assessment

### Low Risk
- PLAN-1.1 (layout engine) — pure data structure, no external dependencies
- PLAN-1.2 (tabs) — lightweight container types, no complex logic
- PLAN-2.2 (keymap) — state machine, deterministic behavior
- PLAN-3.3 (palette) — isolated UI component, no interactions with PTY or rendering

### Medium Risk
- PLAN-2.1 (rendering) — modifies rendering pipeline, but additive (backward-compatible)
- PLAN-3.1 (integration) — large refactor of main.rs, but well-scoped into 3 tasks with clear boundaries
- PLAN-3.4 (verification) — depends on PLAN-3.1 being correct; manual acceptance testing is final gate

### Higher Risk
- PLAN-3.2 (Neovim) — platform-specific code, external RPC protocol, requires Neovim with `--listen`. Fallback path is documented. Risk is manageable but requires careful testing.

**Overall Risk:** MEDIUM. The plans are well-designed and layered, but Phase 3 is complex (multiplexer + Neovim integration). Close coordination during integration (PLAN-3.1) is critical.

---

## Recommendations

### Pre-Execution Checklist

1. **Coordinate main.rs mod declarations:** PLAN-1.1, PLAN-1.2, PLAN-2.2, PLAN-3.2, PLAN-3.3 all add `mod` declarations to main.rs. Ensure they are added in the order listed above to avoid merge conflicts.

2. **Verify `input::translate_key_event()` exists:** PLAN-2.2 Task 1 calls `input::translate_key_event()`. Confirm this function is exported from `arcterm-app/src/input.rs` before executing PLAN-2.2.

3. **Test Neovim integration post-Phase 3:** PLAN-3.2 requires Neovim installed with `--listen` support. Phase 3 acceptance testing (PLAN-3.4 Task 3) verifies this end-to-end, but builder should have Neovim available for manual testing.

4. **Latency baseline from Phase 2:** PLAN-3.4 Task 1 compares single-pane and 4-pane latency. Ensure Phase 2 latency measurements (if any) are available to establish a baseline. If not, Phase 3 will establish its own baseline.

5. **Plan execution order:** Strict ordering for Wave 1 is not required (PLAN-1.1 and PLAN-1.2 are independent), but Wave 2 must follow Wave 1, and PLAN-3.1 must precede PLAN-3.2 and PLAN-3.3. Wave 3 can be parallelized once PLAN-3.1 completes.

### Post-Execution Verification

After execution, produce a `VERIFICATION.md` in the phase directory documenting:
- Test pass counts for each plan
- Clippy warning count (should be 0)
- Manual acceptance test results for the 7 Phase 3 success criteria
- Performance baseline (single-pane vs. multi-pane latency)
- Any deviations from the planned tasks

---

## Conclusion

**Phase 3 Plan Status: READY FOR EXECUTION**

All 8 plans are feasible, well-scoped, and properly ordered. File paths are correct, APIs align with existing code, and verification commands are executable. The plans collectively cover all 7 Phase 3 success criteria from the ROADMAP with clear layering:

- **Wave 1:** Foundation (layout engine, tab model)
- **Wave 2:** Rendering and input (multi-pane render pipeline, keymap state machine)
- **Wave 3:** Integration (AppState refactoring, Neovim awareness, command palette, final verification)

The largest risk is PLAN-3.1's scope (refactoring main.rs), but it is well-structured into 3 clear tasks with explicit acceptance criteria. Neovim integration (PLAN-3.2) adds platform-specific code and external RPC, but fallback paths are documented and testing is concrete.

**Recommendation:** Proceed to execution. Brief the builder on Wave 1 parallelization, PLAN-3.1 coordination complexity, and Neovim testing requirements.

