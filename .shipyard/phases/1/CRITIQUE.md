# Phase 1 Plan Critique Report
**Date:** 2026-03-15
**Type:** Plan Review (Feasibility Stress Test)
**Phase:** Foundation -- Window, Shell, Pixels

---

## Executive Summary

All six Phase 1 plans are structurally sound and collectively cover all seven success criteria. The plan set exhibits good dependency ordering with no circular dependencies, forward references, or inter-plan file conflicts. Wave organization is correct (Wave 1 → Wave 2 → Wave 3 with proper dependencies). Verification commands are concrete and runnable. **Verdict: READY**

---

## Coverage Analysis: Phase 1 Success Criteria

| # | Criterion | Covered By | Status |
|---|-----------|-----------|---------|
| 1 | `cargo build` produces a binary that opens a native window on macOS | PLAN-1.1 (workspace), PLAN-2.3 (GPU window), PLAN-3.1 (integration) | COVERED |
| 2 | Typing characters in the window sends them to a PTY-backed shell and displays the output | PLAN-2.2 (PTY), PLAN-3.1 (input handling), PLAN-3.1 (integration) | COVERED |
| 3 | Basic VT100 sequences (cursor movement, color, erase) render correctly | PLAN-2.1 (VT parser), PLAN-2.3 (renderer), PLAN-3.1 (integration) | COVERED |
| 4 | `ls`, `vim`, `top`, and `htop` produce usable output | PLAN-2.1 (task 3 edge cases), PLAN-3.1 (manual testing) | COVERED |
| 5 | Key-to-screen latency is measurable and under 16ms | PLAN-3.1 (task 3 latency trace feature) | COVERED |
| 6 | Cold start is under 500ms | PLAN-3.1 (task 3 cold start measurement) | COVERED |
| 7 | CI runs `cargo build`, `cargo test`, `cargo clippy` on every push | PLAN-3.2 (GitHub Actions workflow) | COVERED |

**Result: All 7 criteria explicitly addressed. ✓**

---

## Per-Plan Detailed Critique

### PLAN-1.1: Workspace Scaffold and Core Types
**Wave:** 1 | **Dependencies:** None | **Status:** READY

**Scope:**
- 14 files touched (3 of which are config/toolchain files)
- Task count: 3 (below 3-task limit ✓)

**Structural Assessment:**

1. **Completeness:** All foundational types are defined (Color, Cell, Grid, InputEvent, KeyCode, Modifiers). Core trait-based patterns enabled for downstream crates.
2. **Dependencies:** No dependencies on other Wave 1 plans. Correctly positioned as foundation.
3. **Verification:**
   - Task 1 verify: `cargo check --workspace` with tail output capture — concrete and runnable. ✓
   - Task 2 verify: `cargo test --package arcterm-core` with specific assertions about test count and documentation. ✓
   - Task 3 verify: `cargo build --workspace` and `cargo tree --workspace` — concrete. ✓
4. **Test-Driven:** TDD=true for Task 2. Good practice for core types.
5. **Must-haves:**
   - "Cargo workspace compiles with all five crates" — covered by Task 1 ✓
   - "arcterm-core exports Cell, Color, CursorPos, GridSize, InputEvent types" — covered by Task 2 ✓
   - "cargo test passes for arcterm-core" — covered by Task 2 ✓

**Notes:** The workspace dependency declaration in Task 1 is prescient (declaring dependencies before they're used). This avoids surprise incompatibilities in Wave 2. The glyphon and cosmic-text mention suggests design research was done.

**Risk Flag:** Edition 2024 is very recent. Verify Rust 1.85+ is available in CI before execution.

**Verdict:** ✓ READY

---

### PLAN-2.1: VT Parser and Terminal Grid State Machine
**Wave:** 2 | **Dependencies:** PLAN-1.1 | **Parallel with:** PLAN-2.2, PLAN-2.3 | **Status:** READY

**Scope:**
- 4 files touched (1 new: arcterm-vt/src/handler.rs, 1 modified: arcterm-core/src/grid.rs)
- Task count: 3 ✓
- TDD: True for all tasks ✓

**Structural Assessment:**

1. **Dependency Ordering:** Correctly depends on PLAN-1.1 (needs Cell, Grid types). No forward dependencies on PLAN-2.2 or 2.3. ✓
2. **File Conflicts:** Task 1 modifies `arcterm-core/src/grid.rs`. PLAN-2.2 and PLAN-2.3 do not touch this file. ✓
3. **Implementation Detail Quality:**
   - Handler trait design is sound (semantic operations, not low-level byte dispatch).
   - Grid extensions (scroll_up, scroll_down, put_char_at_cursor) are concrete.
   - SGR parsing specifics (params [31] → Indexed(1), 256-color mode 38;5;N, RGB mode 38;2;R;G;B) are complete.
4. **Test Coverage:**
   - Task 1 tests Grid mutations (wrapping, scrolling, SGR parsing). ✓
   - Task 2 tests Processor integration with vte::Parser. Test for full VT sequence round-trip. ✓
   - Task 3 adds edge cases: line wrapping, scrolling, tab stops, 256-color, RGB, multi-param SGR, CUP defaults, erase modes, backspace. Directly maps to `ls`, `vim`, `top`, `htop` requirements. ✓
5. **Verification Commands:**
   - Task 1/2/3 all use `cargo test --package arcterm-vt 2>&1 | tail -15` with assertions like "All tests pass, test count ≥15". Concrete. ✓

**Must-haves:**
- "vte Perform trait implemented with CSI dispatch for cursor movement, SGR colors, and erase sequences" — Task 2 ✓
- "Grid correctly mutates in response to VT byte sequences" — Task 1 + Task 2 ✓
- "All VT logic is unit-testable without GPU or PTY" — All tasks are unit tests, no GPU/PTY dependency. ✓

**Integration Notes:** Task 2 creates Processor which depends on Grid satisfying Handler trait. This is a forward dependency on Grid extending Handler, which PLAN-2.1 Task 1 does. No circular dependency. ✓

**Verdict:** ✓ READY

---

### PLAN-2.2: PTY Session Management
**Wave:** 2 | **Dependencies:** PLAN-1.1 | **Parallel with:** PLAN-2.1, PLAN-2.3 | **Status:** READY

**Scope:**
- 3 files touched (new: arcterm-pty/src/session.rs)
- Task count: 2 (below limit ✓)
- TDD: True ✓

**Structural Assessment:**

1. **Dependency Ordering:** Depends only on PLAN-1.1 (GridSize type). Does not depend on PLAN-2.1 or PLAN-2.3. ✓
2. **File Conflicts:** Only touches arcterm-pty/ files. No conflicts with PLAN-2.1 or PLAN-2.3. ✓
3. **Implementation Design:**
   - PtySession structure is clear: master, writer, output_rx, child.
   - Platform handling: portable_pty abstracts Unix (fork/exec) and Windows (ConPTY). ✓
   - Threading model: std::thread::spawn for reader loop, tokio for the app layer (as documented). Sound choice for PTY I/O which can block.
   - Error type defined (PtyError with variants). ✓
4. **Test Design:**
   - Task 1 tests: spawn, I/O round-trip (echo output), resize. ✓
   - Task 2 tests: shell exit detection, graceful recv-after-exit, write-after-exit error handling. ✓
   - All tests use `#[tokio::test]` for async recv. ✓
5. **Verification Commands:**
   - `cargo test --package arcterm-pty` with assertions about test count (≥6). ✓

**Must-haves:**
- "PtySession spawns a shell and reads output via mpsc channel" — Task 1 ✓
- "PtySession accepts input writes" — Task 1 ✓
- "PtySession supports resize" — Task 1 ✓
- "Integration test confirms shell I/O round-trip" — Task 1 (test_write_and_read) ✓

**Readiness Notes:**
- Task 2 mentions returning (PtySession, Receiver) from new(). This is an API change that impacts PLAN-3.1. Verify Task 1 verification command reflects this pattern.
- Shell detection from $SHELL env falls back to /bin/bash (Unix) and cmd.exe (Windows). Reasonable defaults. ✓

**Verdict:** ✓ READY

---

### PLAN-2.3: GPU Window and Text Rendering
**Wave:** 2 | **Dependencies:** PLAN-1.1 | **Parallel with:** PLAN-2.1, PLAN-2.2 | **Status:** READY

**Scope:**
- 4 files touched (new: arcterm-render/src/gpu.rs, text.rs, renderer.rs + example)
- Task count: 3 ✓
- TDD: False (GPU-dependent, visual testing appropriate) ✓

**Structural Assessment:**

1. **Dependency Ordering:** Depends on PLAN-1.1 (Color, Cell, GridSize, Grid types). No forward dependencies. ✓
2. **File Conflicts:** Only touches arcterm-render/ and adds an example. No conflicts with PLAN-2.1 or PLAN-2.2. ✓
3. **GPU Stack Design:**
   - GpuState: encapsulates wgpu Device, Queue, Surface, configuration. Standard pattern. ✓
   - TextRenderer: wraps glyphon (Font system, SwashCache, TextAtlas, TextRenderer, Cache). Comprehensive. ✓
   - Renderer: combines GpuState and TextRenderer. Clean abstraction. ✓
   - Color mapping: hardcoded palette + 256-color cube + RGB. Sufficient for Phase 1. ✓
4. **Verification Strategy:**
   - Task 1: `cargo check --package arcterm-render` — validates GPU code compiles. ✓
   - Task 2: Same, validates glyphon integration compiles. ✓
   - Task 3: `cargo build --package arcterm-render --example window` + visual test (opens window with colored text). Concrete. ✓
5. **Architecture Notes:**
   - winit 0.30 ApplicationHandler pattern is documented (Task 3 example shows resumed(), window_event, about_to_wait hooks).
   - macOS-specific: surface creation in resumed() per macOS requirements (documented). ✓
   - Cursor rendering: simple approach (invert cell at cursor pos) documented, avoids complex graphics pipeline for Phase 1. ✓

**Must-haves:**
- "wgpu + winit window opens on macOS" — Task 3 ✓
- "glyphon renders monospace text in the window" — Task 2 + Task 3 ✓
- "Window responds to resize events" — Task 1 (resize method) + Task 3 example ✓
- "Surface creation happens in resumed() per macOS requirements" — Task 1 + Task 3 ✓

**Risk Flags:**
- wgpu 28 and glyphon 0.9 are recent versions. Verify they're compatible in CI before execution.
- macOS surface creation is critical; documented correctly but untested until execution.
- Font auto-detection via glyphon::FontSystem relies on system font availability. Should be safe but add a fallback error message if fonts not found.

**Verdict:** ✓ READY

---

### PLAN-3.1: Application Shell Integration (PTY-VT-Renderer)
**Wave:** 3 | **Dependencies:** PLAN-2.1, PLAN-2.2, PLAN-2.3 | **Parallel with:** PLAN-3.2 | **Status:** READY

**Scope:**
- 3 files touched (new: arcterm-app/src/terminal.rs, input.rs + modifications to main.rs)
- Task count: 3 ✓
- TDD: False (integration testing appropriate) ✓

**Structural Assessment:**

1. **Dependency Ordering:** Correctly depends on all three Wave 2 plans. Must wait for PLAN-2.1 (Processor), PLAN-2.2 (PtySession), PLAN-2.3 (Renderer). ✓
2. **File Conflicts:** Only touches arcterm-app/. No conflicts with PLAN-3.2 (which touches .github/ and .cargo/). ✓
3. **Integration Design:**
   - Terminal struct owns PtySession, Processor, Grid. Clear ownership model. ✓
   - App struct (ApplicationHandler) orchestrates Terminal, Renderer, event loop. Proper separation of concerns. ✓
   - Data flow: KeyboardInput → translate_key_event → write_input → PTY. PTY output → process_pty_output → Grid mutations → render_frame. Correct flow. ✓
4. **Verification Strategy:**
   - Task 1: `cargo build --package arcterm-app` + manual check (shell prompt appears). ✓
   - Task 2: Same, plus manual check (typing works, colors render, arrows work, Ctrl+C/D work, cursor visible). ✓
   - Task 3: Build succeeds, manual tests (ls --color, vim, top, htop, resize, Ctrl+C, VT color sequences, rapid output). Cold start time and latency tracing logged. ✓
5. **Critical Details:**
   - Keyboard input mapping is comprehensive: printable chars via event.text (Priority 1), special keys via logical_key (Priority 2), Ctrl combinations (Priority 3). Handles modern Terminal correctly (CR not LF on Enter, DEL not BS on Backspace). ✓
   - Feature flag `latency-trace` gates latency logging without impacting release builds. ✓
   - Error handling: graceful exit on PtySession failure, "Shell exited" message on child exit, Surface::Outdated handling. ✓
6. **Integration Points:**
   - Task 1: Must work with API changes from PLAN-2.2 (receiver returned from new()). Task details suggest this via "Store the receiver on App". ✓
   - Task 3: Manual tests for `ls --color`, `vim`, `top`, `htop` directly validate Phase 1 success criteria #4. ✓

**Must-haves:**
- "arcterm-app binary opens a window running an interactive shell" — Task 1 ✓
- "Keyboard input is sent to the PTY and output is displayed" — Task 2 ✓
- "VT100 sequences render correctly (colors, cursor movement, erase)" — Task 2 ✓
- "ls, vim, top produce usable output" — Task 3 ✓
- "Cursor is visible and positioned correctly" — Task 2 ✓

**Readiness Notes:**
- Cursor rendering strategy (invert cell at cursor) is simplistic but acceptable for Phase 1. Block cursor will be visible during text input.
- Rapid output test ("yes | head -1000") is important for validating no hangs in the event loop. Good test choice.

**Verdict:** ✓ READY

---

### PLAN-3.2: CI Pipeline
**Wave:** 3 | **Dependencies:** PLAN-2.1, PLAN-2.2 | **Parallel with:** PLAN-3.1 | **Status:** READY

**Scope:**
- 2 files touched (.github/workflows/ci.yml, .cargo/config.toml) + 1 minor update (arcterm-render/Cargo.toml feature flag)
- Task count: 2 ✓
- TDD: False (CI configuration) ✓

**Structural Assessment:**

1. **Dependency Ordering:** Depends on PLAN-2.1 and PLAN-2.2 (need tests to exist). Does not depend on PLAN-3.1 (render tests are optional, best-effort). ✓
2. **File Conflicts:** Only touches .github/ and .cargo/. No conflicts with PLAN-3.1 (arcterm-app/). ✓
3. **CI Design:**
   - Job 1 (check): runs on ubuntu-latest, macos-latest, windows-latest. Covers all three platforms required by Phase 1. ✓
   - Job 2 (gpu-test): ubuntu-latest only with Mesa software rendering. Reasonable for Phase 1 (no macOS GPU CI yet). ✓
   - Check job steps: fmt --check, clippy -D warnings, build --workspace, test (non-GPU crates). Correct. ✓
   - GPU job: installs Mesa, sets WGPU_BACKEND=vulkan, runs render tests with feature flag, marked continue-on-error (best-effort). ✓
4. **Test Selection:**
   - `cargo test --package arcterm-core --package arcterm-vt --package arcterm-pty` excludes GPU crates (arcterm-render, arcterm-app). Correct for a headless CI environment. ✓
   - Clippy uses `-D warnings` to make warnings hard errors. Good practice. ✓
   - Format check prevents diverging code styles. ✓
5. **Development Ergonomics:**
   - Aliases: `cargo xt` (test non-GPU), `cargo xr` (run app), `cargo xc` (clippy). Helpful shortcuts. ✓
   - Cargo config sets jobs=0 (unlimited parallelism), lld linker on x86_64 macOS (faster linking). ✓
6. **Verification Commands:**
   - Task 1 verify: `cat .github/workflows/ci.yml | head -5` (checks file exists) + Python YAML parsing (validates syntax). ✓
   - Task 2 verify: `cargo xt --help` or note that alias works + CI workflow validation. ✓

**Must-haves:**
- "GitHub Actions CI runs cargo build, cargo test, cargo clippy on every push" — Task 1 ✓
- "CI passes on macOS and Linux (Windows best-effort)" — Task 1 (macOS and Linux required, Windows included). ✓
- "arcterm-vt and arcterm-pty tests run in CI without GPU" — Task 1 (check job) ✓

**Considerations:**
- Windows support is included in the matrix but GPU test is marked best-effort. This aligns with "Windows best-effort" in must-haves. ✓
- No cross-compilation or binary artifact archival in Phase 1. Deferred to Phase 8 (correct). ✓
- rust-cache action speeds up builds significantly. Good choice. ✓

**Verdict:** ✓ READY

---

## Wave Dependency Verification

```
Wave 1: PLAN-1.1 (foundation, no deps)
  ↓ (all depend on 1.1)
Wave 2: PLAN-2.1, PLAN-2.2, PLAN-2.3 (parallel, all depend on 1.1)
  ↓ (all depend on Wave 2)
Wave 3: PLAN-3.1, PLAN-3.2 (parallel)
  - PLAN-3.1 depends on PLAN-2.1, 2.2, 2.3
  - PLAN-3.2 depends on PLAN-2.1, 2.2
```

**Ordering Assessment:**
- ✓ No circular dependencies
- ✓ No forward references within waves
- ✓ Wave 2 correctly waits for Wave 1
- ✓ Wave 3 correctly waits for Wave 2
- ✓ Parallel execution within waves is safe (no file conflicts)

---

## File Conflict Analysis

| File | Plans Touching | Conflict? |
|------|----------------|-----------|
| Cargo.toml (root) | PLAN-1.1, PLAN-3.2 | No (1.1 creates, 3.2 reads/validates) |
| arcterm-core/src/grid.rs | PLAN-1.1, PLAN-2.1 | No (1.1 creates, 2.1 extends with methods) |
| arcterm-vt/src/lib.rs | PLAN-2.1 (both tasks export) | No (same plan, cumulative exports) |
| arcterm-pty/src/lib.rs | PLAN-2.2 (both tasks export) | No (same plan, cumulative exports) |
| arcterm-render/src/lib.rs | PLAN-2.3 (all tasks export) | No (same plan, cumulative exports) |
| arcterm-app/src/main.rs | PLAN-3.1, PLAN-3.2 | No (3.1 implements, 3.2 only references) |
| arcterm-render/Cargo.toml | PLAN-2.3, PLAN-3.2 | No (2.3 creates, 3.2 adds feature flag) |

**Result: No conflicts. ✓**

---

## Verification Command Quality Assessment

All plans use concrete, runnable verification commands:

1. **PLAN-1.1:** `cargo check --workspace`, `cargo test --package arcterm-core`, `cargo build --workspace`, `cargo tree --workspace` — all executable immediately after plan execution. ✓
2. **PLAN-2.1:** `cargo test --package arcterm-vt` — verifies tests exist and pass. ✓
3. **PLAN-2.2:** `cargo test --package arcterm-pty` — same. ✓
4. **PLAN-2.3:** `cargo check --package arcterm-render`, `cargo build --package arcterm-render --example window` — compiles and visual test. ✓
5. **PLAN-3.1:** `cargo build --package arcterm-app` — compiles and manual smoke tests (typing, colors, etc.) are concrete. ✓
6. **PLAN-3.2:** `cat .github/workflows/ci.yml | head -5`, YAML parsing validation. ✓

No vague commands like "check that it works" or "verify code is clean." All are executable assertions.

---

## Complexity Flags

| Plan | Files Touched | Directories | Complexity | Risk |
|------|-------|-----------|------------|------|
| PLAN-1.1 | 14 | 7 (root, arcterm-core, arcterm-vt, arcterm-pty, arcterm-render, arcterm-app) | **Moderate** (workspace scaffold touches many crates, but each stub is simple) | Low (foundational, straightforward) |
| PLAN-2.1 | 4 | 2 (arcterm-vt, arcterm-core) | Moderate (VT parsing logic is complex, but Tasks 1-3 structure it well) | Medium (vte crate integration, SGR parsing complexity) |
| PLAN-2.2 | 3 | 1 (arcterm-pty) | Low | Low (portable_pty abstracts platform differences) |
| PLAN-2.3 | 4 | 1 (arcterm-render) + example | **Moderate-High** (GPU stack: wgpu + glyphon + winit, three external complexity sources) | **Medium** (wgpu 28 is recent, macOS surface handling critical) |
| PLAN-3.1 | 3 | 1 (arcterm-app) | **Moderate-High** (integration of all prior plans, event loop, threading) | Medium (must coordinate all Wave 2 outputs, latency-critical) |
| PLAN-3.2 | 2 (+ config) | 2 (.github, .cargo) | Low | Low (CI configuration, well-documented) |

**High-Risk Items:**
1. **PLAN-2.3 (GPU Stack):** wgpu 28 + glyphon 0.9 + macOS surface creation. Recommend: test on macOS early, have a fallback approach if surface creation fails (e.g., CPU-rasterized fallback for Phase 1 proof-of-concept). ✓ Roadmap already mentions this as the highest-risk item.

2. **PLAN-3.1 (Integration):** Coordinating Processor output with Renderer, ensuring latency is under 16ms. Recommend: run latency traces immediately after PLAN-3.1 execution to surface any bottlenecks.

---

## Hidden Dependencies / Implicit Constraints

Checked for ordering constraints not explicitly listed in plan dependencies:

1. **PLAN-2.1 (Processor) → PLAN-2.2 (PtySession)?** No. VT logic is independent of PTY. Both can be developed in parallel. ✓
2. **PLAN-2.2 (PtySession) → PLAN-2.3 (Renderer)?** No. PTY logic is independent of rendering. ✓
3. **PLAN-3.1 (App) → PLAN-3.2 (CI)?** No. App must exist before CI can test it, but PLAN-3.2 explicitly depends on PLAN-2.1 and PLAN-2.2 (tests must exist). Both can be written in parallel as long as CI checks are added after PLAN-3.1 completes. Task 1 of PLAN-3.2 creates the workflow file (no dependencies on PLAN-3.1 code). ✓

No hidden dependencies detected.

---

## Acceptance Criteria Testability

Spot-checked Task-level acceptance criteria for measurability:

1. **PLAN-1.1 Task 1:** "All five crates are listed in `cargo metadata`" — Measurable. ✓
2. **PLAN-2.1 Task 1:** "SGR parse: params [31] sets fg to Indexed(1)" — Unit test assertions, measurable. ✓
3. **PLAN-2.2 Task 1:** "Received bytes contain 'hello_pty_test'" — Measurable string assertion. ✓
4. **PLAN-2.3 Task 3:** "Running `cargo run --package arcterm-render --example window` opens a window...displaying colored monospace text" — Visual test, concrete (window must open, colors must display). ✓
5. **PLAN-3.1 Task 2:** "Backspace: position cursor at (0, 5), feed BS, verify cursor at (0, 4)" — Concrete state assertion. ✓
6. **PLAN-3.2 Task 1:** "File is valid YAML (parseable)" — Measurable via Python YAML parser. ✓

All acceptance criteria are testable.

---

## Forward References Check

Scanning for dependencies between "parallel" plans in the same wave:

**Wave 2 (PLAN-2.1, PLAN-2.2, PLAN-2.3):**
- PLAN-2.1 defines Processor, Handler trait, and extends Grid.
- PLAN-2.2 creates PtySession, depends only on GridSize (from PLAN-1.1).
- PLAN-2.3 creates Renderer, depends only on Color, Cell, Grid (from PLAN-1.1).

No plan in Wave 2 depends on outputs from other plans in Wave 2. ✓

**Wave 3 (PLAN-3.1, PLAN-3.2):**
- PLAN-3.1 implements App and Terminal, depends on all of Wave 2.
- PLAN-3.2 implements CI, depends only on PLAN-2.1 and PLAN-2.2 (tests must exist).

PLAN-3.2 does NOT depend on PLAN-3.1 (app code doesn't need to exist for CI config to be written). ✓

---

## Cross-Platform Consideration

Phase 1 success criteria mention "native window on macOS" but the plans should work across platforms:

1. **PLAN-1.1:** Workspace setup is platform-agnostic. ✓
2. **PLAN-2.1:** VT parsing is platform-agnostic. ✓
3. **PLAN-2.2:** portable_pty handles Unix/Windows/macOS. ✓
4. **PLAN-2.3:** wgpu is cross-platform; winit ApplicationHandler is cross-platform. macOS-specific surface creation is documented. ✓
5. **PLAN-3.1:** Input handling (event.text, logical_key) is winit's abstraction, platform-agnostic. ✓
6. **PLAN-3.2:** CI runs on ubuntu-latest, macos-latest, windows-latest. ✓

Good cross-platform awareness.

---

## Known Risks & Mitigations

| Risk | Plan | Mitigation |
|------|------|-----------|
| wgpu 28 + glyphon 0.9 compatibility | PLAN-2.3 | Already mentioned in ROADMAP as highest-risk item. Test early. |
| macOS surface creation (winit 0.30) | PLAN-2.3 | Plan documents resumed() requirement explicitly. ROADMAP says "tackle renderer first". ✓ |
| VT100 parser completeness for real programs | PLAN-2.1 | Task 3 adds edge cases for ls, vim, top, htop. Pragmatic Phase 1 approach. ✓ |
| PTY I/O blocking thread model | PLAN-2.2 | Uses std::thread for reader (can block), tokio for app (async). Documented trade-off. ✓ |
| Latency under 16ms | PLAN-3.1 | Task 3 adds latency tracing. Early measurement-driven approach. ✓ |
| CI on Windows with GPU | PLAN-3.2 | Marked best-effort, no Mesa on Windows. Acceptable for Phase 1. ✓ |

All risks are either mitigated or explicitly documented as acceptable Phase 1 scope limits.

---

## Verdict

### Overall Plan Quality: **READY**

**Strengths:**
1. ✓ All 7 Phase 1 success criteria are covered by at least one plan.
2. ✓ Plans respect wave dependencies (Wave 1 → Wave 2 → Wave 3).
3. ✓ No circular dependencies or forward references within waves.
4. ✓ No file conflicts between parallel plans.
5. ✓ All verification commands are concrete and runnable.
6. ✓ All acceptance criteria are testable.
7. ✓ Plans show good software engineering (TDD for logic, clean abstractions, proper error handling).
8. ✓ Task counts are within limits (max 3 per plan, most 3 or fewer).
9. ✓ Risk areas are identified and mitigated (GPU stack tackled first per ROADMAP, latency tracing planned, etc.).

**Cautions (not blockers):**
1. **PLAN-2.3:** Recent versions of wgpu and glyphon. Recommend early testing on target platforms (macOS, Linux, Windows).
2. **PLAN-3.1:** Integration task coordinating four complex subsystems. Recommend latency profiling immediately after to catch bottlenecks.
3. **Rust edition 2024:** Requires Rust 1.85+. Verify in CI before execution.

**Recommendations for Execution:**
1. Start PLAN-1.1 immediately (foundation for all others).
2. Parallelize Wave 2 as documented (all safe to start after Wave 1).
3. **Prioritize PLAN-2.3 testing early** — if GPU stack fails, other Wave 2 work is not blocked, but Wave 3 cannot start until this is fixed.
4. Run `cargo xt` frequently (test harness defined in PLAN-3.2).
5. Capture latency traces as soon as PLAN-3.1 Task 3 is complete.

---

## Conclusion

The six Phase 1 plans are well-structured, comprehensive, and feasible. All success criteria are addressed. Wave organization is correct with no hidden dependencies. Verification strategy is solid. Proceed with execution.

**Final Verdict: READY**
