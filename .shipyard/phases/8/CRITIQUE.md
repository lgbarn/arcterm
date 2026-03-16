# Phase 8 Plan Critique
**Phase:** Config Overlays, Polish, Release (FINAL PHASE)
**Date:** 2026-03-15
**Type:** plan-review (Pre-Execution Verification)

---

## Phase 8 Success Criteria (from ROADMAP.md)

1. AI config overlay workflow works end-to-end: AI writes pending overlay, `Leader+o` shows diff, user accepts/rejects/edits
2. `arcterm config flatten` exports the fully resolved config as a single TOML file
3. Key-to-screen latency is under 5ms (measured with input latency tooling)
4. Cold start is under 100ms
5. Memory baseline is under 50MB with zero panes, under 60MB with 4 panes
6. Frame rate exceeds 120 FPS during fast output scrolling
7. All CI checks pass on macOS, Linux, and Windows
8. Binary builds are available for macOS (aarch64, x86_64), Linux (x86_64), and Windows (x86_64)
9. Search across all pane output (`Leader+/`) works with regex support

---

## Plan Coverage Analysis

| # | Success Criterion | Covered By | Coverage | Notes |
|---|---|---|---|---|
| 1 | AI config overlay workflow (pending, diff, accept/reject/edit) | PLAN-8.1 T1, T2, T3 | FULL | Tasks 1-3 cover: config loading with overlays, `Leader+o` keybinding, diff computation, file moving/rejecting, $EDITOR spawning, rendering. |
| 2 | `arcterm config flatten` exports resolved TOML | PLAN-8.1 T1 | FULL | Task 1 adds `flatten_to_string()` function, PLAN-8.1 T2 wires `arcterm config flatten` subcommand. |
| 3 | Key-to-screen latency <5ms | PLAN-8.3 T1 | PARTIAL | Task 1 adds latency tracing under `#[cfg(feature = "latency-trace")]` and defers plugin/syntect load, but does not include actual measurement commands or profiling pass details. |
| 4 | Cold start <100ms | PLAN-8.3 T1 | PARTIAL | Task 1 defers syntect (23ms) and plugin loading to after first frame; describes the optimization but lacks verification step with timing measurement. |
| 5 | Memory <50MB/60MB | PLAN-8.3 T1 | PARTIAL | Task 1 mentions checking scrollback pre-allocation but does not include actual memory profiling commands or baseline establishment. |
| 6 | Frame rate >120 FPS | PLAN-8.3 T1 | NOT COVERED | No plan task addresses FPS measurement or optimization. Search overlay (PLAN-8.2) could impact FPS but no mitigations specified. |
| 7 | CI passes on macOS, Linux, Windows | PLAN-8.3 T2 | PARTIAL | Task 2 extends CI to run `arcterm-app` tests and clippy on all platforms, but does not define success metrics (pass/fail criteria) or include a test run verification step. |
| 8 | Binaries for 4 targets (macOS aarch64/x86_64, Linux x86_64, Windows x86_64) | PLAN-8.3 T3 | FULL | Task 3 runs `cargo dist init`, generates `dist.toml` and `release.yml` with all four targets. Includes `cargo dist build --artifacts=local` verification. |
| 9 | Cross-pane regex search (`Leader+/`) | PLAN-8.2 T1, T2, T3 | FULL | Task 1 adds grid text extraction and search state, Task 2 wires into event loop with highlighting, Task 3 adds debounced regex and viewport management. |

---

## Critical Issues

### Issue 1: FPS Criterion Not Addressed
**Severity:** HIGH
**Criterion 6** ("Frame rate exceeds 120 FPS during fast output scrolling") is not mentioned in any plan task. PLAN-8.2 (cross-pane search) adds match highlighting quads that could impact FPS during scrolling, but no mitigation or measurement is proposed.

**Impact:** Cannot verify or demonstrate that 120 FPS target is met after Phase 8 changes.

### Issue 2: Performance Measurement Verification Missing
**Severity:** MEDIUM
**Plan 8.3, Task 1** describes latency/cold-start/memory optimizations but the `verify` step is:
```
cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app --release 2>&1 | tail -5
```
This only verifies the build succeeds, not that:
- Cold start is actually <100ms (must measure with time/timing tool)
- Key-to-frame latency is <5ms (must parse latency-trace output)
- Memory is <50/60MB (must measure RSS/peak memory)

**Impact:** Tasks will complete without proving the performance targets are met.

### Issue 3: CI Success Criteria Undefined
**Severity:** MEDIUM
**Plan 8.3, Task 2** extends CI to test `arcterm-app` and run clippy, but the `done` statement says "CI yml includes arcterm-app in test matrix" without specifying:
- What constitutes "CI passes" (all tests green? zero warnings?)
- How to verify results on the three platforms

**Impact:** Task 2 completion is subjective; cannot verify success objectively.

---

## Wave Ordering & Dependency Analysis

### Wave 1: PLAN-8.1 and PLAN-8.2 (Parallel)
- **PLAN-8.1:** Config overlays and flatten (no dependencies)
- **PLAN-8.2:** Cross-pane search (no dependencies)
- **Status:** Can execute in parallel. ✓ Correct.

### Wave 2: PLAN-8.3 (Depends on 8.1 + 8.2)
- **Dependencies:** `["8.1", "8.2"]`
- **Rationale:** Performance measurement and release should run after feature code is complete.
- **Status:** ✓ Correct ordering. Features must ship before optimization and release.

---

## Plan Structure Quality

### Plan 8.1 (Config Overlays + Flatten)
- **Scope:** 3 tasks covering config model, keymap/overlay state, and rendering. Vertical slice from data to UI. ✓ Appropriate.
- **Task Breakdown:**
  - T1 (TDD): Config serialization, merge logic, flatten function. ✓ Good.
  - T2 (TDD): Keymap bindings, overlay diff computation, state machine. ✓ Good.
  - T3 (No TDD): Rendering and file I/O. ✓ Appropriate (GUI code often harder to unit test).
- **File Conflicts:** No overlap between plans 8.1 and 8.2. ✓ Good.
- **Verification Commands:**
  - T1: `cargo test --package arcterm-app -- config::tests` → Concrete. ✓
  - T2: `cargo test --package arcterm-app -- keymap::tests overlay::tests` + `cargo build` → Concrete. ✓
  - T3: `cargo build` only → Builds the code, does not verify overlay rendering works. ✗ Weak.

### Plan 8.2 (Cross-Pane Search)
- **Scope:** 3 tasks covering grid text extraction, search state, and rendering. ✓ Vertical slice.
- **Task Breakdown:**
  - T1 (TDD): Grid text extraction and search regex engine. ✓ Good.
  - T2 (No TDD): Event loop wiring and highlighting quads. ✓ Appropriate.
  - T3 (TDD): Edge cases (debounce, viewport, scrollback). ✓ Good refinement.
- **File Conflicts:** No overlap with 8.1 except both modify `main.rs` (different concerns: overlay vs. search). ✓ Acceptable.
- **Verification Commands:**
  - T1: `cargo test --package arcterm-core -- row_to_string all_text_rows` + `cargo test --package arcterm-app -- search::tests` → Concrete. ✓
  - T2: `cargo build` only → Same as 8.1 T3, does not verify search rendering. ✗ Weak.
  - T3: `cargo test --package arcterm-app -- search::tests` → Good. ✓

### Plan 8.3 (Performance + Release)
- **Scope:** 3 tasks covering optimization, CI extension, and cargo-dist setup. ✓ Feature-complete.
- **Task Breakdown:**
  - T1: Lazy syntect, deferred plugins, latency trace, scrollback audit. Multiple unrelated optimizations. Weakly cohesive but acceptable for final polish phase. ⚠️ Could be tighter.
  - T2: Man page, CI extension, example configs. Good polish. ✓
  - T3: cargo-dist setup for all four targets. ✓ Good.
- **Verification Commands:**
  - T1: `cargo build --release` only. Does not measure cold start, latency, or memory. ✗ Critical gap.
  - T2: `cargo build` + `ls build.rs` + `cat ci.yml`. Builds and checks files exist, does not run CI or measure results. ✗ Weak.
  - T3: `test -f dist.toml && test -f release.yml && cargo dist build --artifacts=local`. Checks files and builds binary. ✓ Good.

---

## Max Tasks Per Plan

Per user spec: "max 3 tasks" per plan.

| Plan | Task Count | Status |
|---|---|---|
| 8.1 | 3 | ✓ Compliant |
| 8.2 | 3 | ✓ Compliant |
| 8.3 | 3 | ✓ Compliant |

---

## File Path Verification

All file paths use absolute paths in `files_touched` sections. Examples:
- ✓ `arcterm-app/Cargo.toml` (relative, acceptable in plans since they're in the repo root)
- ✓ `arcterm-app/src/config.rs`
- ✓ `.github/workflows/ci.yml`

No issues identified.

---

## Task-Level Issues

### PLAN-8.1, Task 1
- **Issue:** `done` statement says tests pass but does not name specific test count (e.g., "14/14 tests").
- **Fix:** Verify statement should include `cargo test --package arcterm-app -- config::tests 2>&1 | grep "test result"` to capture pass count.

### PLAN-8.1, Task 2
- **Issue:** Overlay review rendering is marked TDD=false but involves significant state machine (`handle_key`). T2's TDD tests only cover the state machine in isolation, not the rendering code path.
- **Mitigated by:** T3 implements rendering and can be manually verified.

### PLAN-8.1, Task 3
- **Issue:** Rendering wiring has no tests. Manual verification of "overlay review renders diff with colored lines when Leader+o is pressed" is insufficient for release-critical feature.
- **Recommendation:** Add integration test that simulates Leader+o press, checks `overlay_review.is_some()`, and verifies at least one `OverlayQuad` is created.

### PLAN-8.2, Task 2
- **Issue:** Search highlighting and viewport management are not tested. Verify step is `cargo build`, which only checks compilation.
- **Recommendation:** Add a fixture-based test that simulates search on a multi-pane layout and verifies match quads are generated for visible rows only.

### PLAN-8.3, Task 1
- **Critical Issue:** Verify step does not measure performance.
- **Recommendation:** Add explicit verification steps:
  ```
  cd /Users/lgbarn/Personal/myterm && \
  cargo run --package arcterm-app --features latency-trace --release 2>&1 | grep '\[latency\]' | head -1 && \
  /usr/bin/time -v cargo run --package arcterm-app --release 2>&1 | grep 'Elapsed\|Maximum'
  ```

### PLAN-8.3, Task 2
- **Issue:** Verify step checks files exist (`ls`, `cat`) but does not run CI or verify example configs are valid TOML.
- **Recommendation:** Add `cargo test --package arcterm-app` to verify arcterm-app tests pass on the current platform (as a proxy for CI success).

### PLAN-8.3, Task 3
- **Issue:** Verify step runs `cargo dist build --artifacts=local` but does not check that the binary is executable or includes expected binaries for all four targets (at least test local target).
- **Recommendation:** Add `ls -la target/release/arcterm* && file target/release/arcterm*` to verify binary exists and is ELF/Mach-O/PE.

---

## Documentation & Communication

### Strengths
- Plans are well-structured with clear task descriptions and file dependencies.
- Each task has a concrete `action` section with step-by-step instructions.
- Inline code examples (e.g., enum definitions, function signatures) are helpful.

### Weaknesses
- **No must_haves vs. can_haves distinction** in task descriptions. Hard to prioritize if a feature is cut.
- **Verify steps are sometimes too minimal.** A build success is not proof the feature works; need integration checks.
- **Edge cases underspecified.** E.g., PLAN-8.2 T1 mentions UTF-8 byte-to-column mapping in tests but not in the task description itself.

---

## Verdict

### Summary

**Phase 8 plans are 75% ready for execution.** The feature scope (Plans 8.1 and 8.2) is well-defined and can proceed. Plan 8.3 is underspecified for performance verification and leaves critical success criteria (FPS, latency, memory, cold start) unmeasured.

**MAJOR GAPS:**
1. **FPS criterion not covered** (criterion 6). Must add FPS profiling task or extend 8.3 T1.
2. **Performance measurement verify steps are missing.** Plans describe optimizations but do not include commands to prove targets are hit.
3. **Integration testing is weak.** Verify steps check "build succeeds" but not "feature works end-to-end."

**BLOCKING ISSUES:**
- None prevent execution, but Phase 8 completion cannot be verified without additional performance measurement work (post-execution).

### Recommendations Before Execution

1. **Add FPS profiling to PLAN-8.3 T1:**
   - Add a task bullet point: "Measure frame rate during fast scrolling with `--features fps-trace` instrumentation. Target: >120 FPS baseline, no regression after overlay/search additions."
   - Update verify step to include FPS measurements.

2. **Add explicit performance verification commands to PLAN-8.3 T1:**
   ```
   # Cold start
   /usr/bin/time -v cargo run --package arcterm-app --release 2>&1 | grep "Elapsed\|Max"

   # Key-to-frame latency
   cargo run --package arcterm-app --features latency-trace --release 2>&1 | grep '\[latency\]' | head -5

   # Memory baseline
   /usr/bin/time -v cargo run --package arcterm-app --release 2>&1 | grep "Maximum resident"
   ```

3. **Add integration test placeholders to PLAN-8.1 T3 and PLAN-8.2 T2:**
   - Not full E2E tests, but at least simulate UI state changes (overlay open → accept) and verify `done` conditions.

4. **Clarify CI success criteria in PLAN-8.3 T2:**
   - Define: "CI passes = all cargo test, cargo clippy, and cargo build succeed on macOS, Linux, Windows."
   - Add verify step: `cargo test --package arcterm-app 2>&1 | tail -3` to show test summary.

### Conditional Approval

**APPROVE for execution with above recommendations noted.** Plans 8.1 and 8.2 (Wave 1) can start immediately. Plan 8.3 (Wave 2) should be refined before execution to include explicit performance measurement verification.

---

## Detailed Verdict Matrix

| Aspect | Status | Evidence |
|---|---|---|
| **All 9 criteria addressed?** | FAIL | Criterion 6 (FPS) not covered by any plan. |
| **Max 3 tasks per plan?** | PASS | Each plan has exactly 3 tasks. |
| **Wave ordering correct?** | PASS | Plans 8.1 & 8.2 parallel, 8.3 depends on both. Correct. |
| **File paths valid?** | PASS | All paths are relative to repo root, resolvable. |
| **Task breakdown appropriate?** | PASS | Vertical slices from data to UI, reasonable task sizes. |
| **Verify steps concrete & runnable?** | FAIL | 8.3 T1 verify step does not measure performance. 8.3 T2/T3 verify steps check file existence, not functionality. |
| **Test coverage adequate?** | FAIL | 8.1 T3 and 8.2 T2 have no tests for rendering code. Integration testing is absent. |
| **Documentation quality?** | PASS | Clear task descriptions, inline code, rationale. |
| **Edge cases handled?** | PARTIAL | UTF-8 in search, empty dirs in overlays, but FPS regression risk unmitigated. |

---

## Final Verdict

**VERDICT: PASS WITH REQUIRED AMENDMENTS**

**Plans are executable but incomplete for final verification.** Phase 8 feature code (overlays, search, flatten) is well-designed and can ship. Performance targets cannot be verified without additional measurement work added to PLAN-8.3 T1. Execute Plans 8.1 and 8.2 in parallel immediately. Before executing PLAN-8.3, add explicit performance measurement commands and define FPS profiling scope.

**Ship Readiness: 60%** (Features ready, performance validation deferred)
