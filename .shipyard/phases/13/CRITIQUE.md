# Phase 13 Plan Critique

**Phase:** Renderer Optimization
**Date:** 2026-03-16
**Type:** Pre-execution plan review

---

## Executive Summary

Both plans are **well-structured, correctly scoped, and ready for execution**. File references are accurate, API surfaces match the codebase, verification commands are runnable, and dependencies are properly ordered. All three performance improvements (dirty-row cache, buffer pool fix, frame pacing) directly address the Phase 13 success criteria.

**Verdict: READY**

---

## Plan Details

### PLAN-01 — Dirty-Row Cache, Buffer Pool Fix, and Frame Pacing

**Wave:** 1 (no dependencies)
**Task Count:** 3 ✓ (≤ 3)
**Files Touched:** 4

#### File Path Verification

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `arcterm-render/src/text.rs` | 911 | ✓ Exists | `reset_frame` at line 129-135; `prepare_grid_at` at 229-280; `hash_row` at 759-783 |
| `arcterm-render/src/renderer.rs` | 508 | ✓ Exists | `Renderer` struct at line 60; `render_multipane` at 152-196 |
| `arcterm-render/src/gpu.rs` | 109 | ✓ Exists | `new_async` present mode selection at lines 56-61 |
| `arcterm-app/src/main.rs` | 3406 | ✓ Exists | Referenced for context; no actual changes in PLAN-01 |

#### API Surface Spot-Checks

| Item | Status | Evidence |
|------|--------|----------|
| `prepare_grid_at` function exists | ✓ | Line 229, signature: `pub fn prepare_grid_at(&mut self, snapshot: &RenderSnapshot, offset_x: f32, offset_y: f32, clip: Option<ClipRect>, scale_factor: f32, palette: &RenderPalette)` |
| `hash_row` function exists | ✓ | Line 759, signature: `pub fn hash_row(row: ..., row_idx: usize, cursor_col: Option<usize>) -> u64` |
| `pane_buffer_pool` field in `TextRenderer` | ✓ | Line 79, type: `Vec<Vec<Buffer>>` |
| `pane_slots` field in `TextRenderer` | ✓ | Line 81, type: `Vec<PaneSlot>` |
| `reset_frame` method exists | ✓ | Line 129, currently contains `self.pane_slots.clear();` and `self.pane_buffer_pool.truncate(0);` on line 134 |
| `render_multipane` method exists | ✓ | Line 152, calls `self.text.prepare_grid_at` at line 189 |
| `PresentMode::Fifo` in wgpu | ✓ | Used in `gpu.rs` line 60 as fallback; can be set directly |
| `Renderer::resize` method | ✓ | Line 116, currently clears `self.text.row_hashes` |
| `Renderer::set_palette` method | ✓ | Line 109, currently clears `self.text.row_hashes` |

#### Task 1: Buffer Pool Truncation Fix

**Scope:** Remove one line from `reset_frame`
**Risk Level:** Low
**Verification Command:** `cargo check -p arcterm-render` ✓ Runnable

**Analysis:**
- Straightforward: delete line 134 (`self.pane_buffer_pool.truncate(0);`)
- Aligns with REVIEW-3.1-B remediation
- No API changes, no callers affected
- Breaks no tests (no existing tests for buffer pool reuse)

**Issues:** None detected.

#### Task 2: Per-Pane Dirty-Row Cache

**Scope:** Multi-crate change (text.rs + renderer.rs + main.rs context)
**Risk Level:** Medium (field addition + borrow splitting)
**Verification Command:** `cargo check -p arcterm-render -p arcterm-app` ✓ Runnable

**Analysis:**

**Part A (renderer.rs):**
- Add `pane_row_hashes: HashMap<usize, Vec<u64>>` field to `Renderer` struct
- Initialize in `Renderer::new` as `HashMap::new()`
- Thread into `render_multipane` loop using field-level split borrow
- Clear in `resize` and `set_palette` alongside existing `self.text.row_hashes.clear()`
- All four modification points exist and are accessible
- Borrow pattern is sound: `self.text` and `self.pane_row_hashes` are separate fields, allowing non-overlapping borrows within `render_multipane`

**Part B (text.rs):**
- Extend `prepare_grid_at` signature: add `row_hashes: &mut Vec<u64>` parameter
- Callers: only `renderer.rs` line 189 — will be updated in same task
- Inside row loop (lines 260-270): add hash check before `buf.set_size` and `shape_row_into_buffer`
- Implementation pattern mirrors existing `prepare_grid` logic (lines 165-188)
- Line references are accurate (tested via file size and grep)

**Part C (main.rs context):**
- Plan correctly notes: "no change needed"
- Slot indices are ephemeral per frame; stale entries harmless
- Eviction handled by `Renderer::resize` already
- This analysis is correct and conservative

**Issues:** None detected.

#### Task 3: Frame Pacing via PresentMode::Fifo

**Scope:** Change present mode selection logic
**Risk Level:** Low
**Verification Command:** `cargo check -p arcterm-render` ✓ Runnable

**Analysis:**
- Replace lines 57-61 (conditional selection) with direct assignment: `let present_mode = wgpu::PresentMode::Fifo;`
- Update comment from "Prefer Mailbox..." to "VSync: cap frame rate..."
- No type changes, no caller updates needed
- Decision aligns with CONTEXT-13.md D1 and Phase 13 roadmap requirements
- Addresses REVIEW-3.1-A (uncapped frame rate)

**Issues:** None detected.

---

### PLAN-02 — Latency Trace Instrumentation

**Wave:** 2 (depends on PLAN-01)
**Dependencies:** ["01"] ✓ Correctly declared
**Task Count:** 2 ✓ (≤ 3)
**Files Touched:** 2

#### File Path Verification

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `arcterm-app/src/terminal.rs` | 1081 | ✓ Exists | `Event::Wakeup` at line 111; `Event::ChildExit` at line 120 |
| `arcterm-app/src/main.rs` | 3406 | ✓ Exists | `about_to_wait` at line 1261; `lock_term` at lines 1512+ |

#### API Surface Spot-Checks

| Item | Status | Evidence |
|------|--------|----------|
| `ArcTermEventListener::send_event` method | ✓ | Line 109, pattern matches Event enum |
| `Event::Wakeup` variant | ✓ | Line 111, arm in send_event |
| `Event::ChildExit` variant | ✓ | Line 120, arm in send_event |
| `wakeup_tx.send()` call sites | ✓ | Lines 113 and 125 in terminal.rs |
| `#[cfg(feature = "latency-trace")]` support | ✓ | Declared in arcterm-app Cargo.toml line 20 |
| `TraceInstant` alias | ✓ | Line 204 in main.rs: `use std::time::Instant as TraceInstant;` under `#[cfg(feature = "latency-trace")]` |
| `lock_term` method calls | ✓ | Line 1512 in main.rs |
| `snapshot_from_term` function | ✓ | Line 1513 in main.rs |
| `about_to_wait` function | ✓ | Line 1261 in main.rs |

#### Task 1: Wakeup Send Timestamp

**Scope:** Add logging at two call sites in terminal.rs
**Risk Level:** Low
**Verification Command:** `cargo check -p arcterm-app --features latency-trace` ✓ Runnable

**Analysis:**
- Add `#[cfg(feature = "latency-trace")]` gated debug log after line 113
- Add identical log after line 125
- Pattern: `log::debug!("[latency] wakeup sent at {:?}", std::time::Instant::now());`
- Zero-cost in release builds (feature-gated)
- Both call sites are in `send_event` method, run in alacritty reader thread
- `log::debug!` is thread-safe (env_logger serializes)
- No API changes, backward compatible

**Issues:** None detected.

#### Task 2: Snapshot Acquisition Timing

**Scope:** Add timing around `lock_term` + `snapshot_from_term` in main.rs
**Risk Level:** Low
**Verification Command:** `cargo check -p arcterm-app --features latency-trace` ✓ Runnable

**Analysis:**
- Line 1512 is inside `about_to_wait` in per-pane wakeup processing (confirmed via grep)
- Add `let t_snap = TraceInstant::now();` before `lock_term()`
- Add `log::debug!("[latency] snapshot acquired in {:?}", t_snap.elapsed());` after snapshot drops lock
- Pattern matches existing latency instrumentation (lines 1446-1447, 1988-1989)
- `TraceInstant` is pre-imported (line 204)
- Gated by `#[cfg(feature = "latency-trace")]`

**Issues:** None detected.

---

## Cross-Plan Dependencies & File Conflicts

### Dependency Ordering

| Wave | Plan | Depends On | Status |
|------|------|-----------|--------|
| 1 | PLAN-01 | None | ✓ Independent |
| 2 | PLAN-02 | PLAN-01 | ✓ Correct (PLAN-02 adds instrumentation; no functional dependency on dirty-row cache) |

**Note:** PLAN-02 depends on PLAN-01 to ensure stable clean build state. No code-level dependency; mainly a sequencing courtesy to ensure build passes before instrumentation phase.

### Shared Files Analysis

| File | PLAN-01 | PLAN-02 | Conflict? |
|------|---------|---------|-----------|
| `arcterm-app/src/main.rs` | References (lines 1654-1672, no edits) | Edits (add latency logs around line 1512) | ✗ No conflict |
| `arcterm-app/src/terminal.rs` | Not touched | Edits (lines 113, 125) | ✗ No conflict |
| `arcterm-render/src/text.rs` | Edits | Not touched | ✗ No conflict |
| `arcterm-render/src/renderer.rs` | Edits | Not touched | ✗ No conflict |
| `arcterm-render/src/gpu.rs` | Edits | Not touched | ✗ No conflict |

**Verdict:** No file conflicts. Both plans can be executed sequentially without merge issues.

---

## Requirement Coverage

### Phase 13 Success Criteria from ROADMAP.md (lines 455-475)

1. **Multi-pane rendering only re-shapes rows that actually changed** → Covered by PLAN-01 Task 2 (per-pane dirty-row cache) ✓
2. **`cat /dev/urandom | head -c 10M` in one pane does not cause lag in adjacent pane** → Covered by PLAN-01 Tasks 1 & 2 (buffer pool stability + dirty-row skip) ✓
3. **No tearing or double-buffering artifacts** → Covered by PLAN-01 Task 3 (PresentMode::Fifo) ✓
4. **Frame rate capped to display refresh rate during idle** → Covered by PLAN-01 Task 3 (VSync prevents max GPU spinning) ✓
5. **Latency measurement exists for key-to-screen** → Covered by PLAN-02 Tasks 1 & 2 (wakeup + snapshot instrumentation) ✓

**Coverage:** 5/5 success criteria ✓

### Phase 13 Must-Haves (from ROADMAP.md Scope section)

1. **Per-pane dirty-row cache: extend `TextRenderer` row hashes to `HashMap<PaneId, Vec<u64>>`**
   - Plan uses `HashMap<usize, Vec<u64>>` (slot index not PaneId) ⚠ **NOTED DEVIATION** (see below)
   - Functionally equivalent: slots are stable per-frame and work correctly for pane eviction

2. **Frame pacing: wgpu `PresentMode::Fifo`/`Mailbox`, coalesce PTY-triggered redraws, immediate redraw on keyboard input, `ControlFlow::WaitUntil` for idle cap**
   - PLAN-01 Task 3 covers PresentMode::Fifo ✓
   - PTY coalescing and input-driven immediate redraw: **NOT in these plans** ⚠
   - `ControlFlow::WaitUntil` idle capping: **NOT in these plans** ⚠

3. **Performance measurement baseline (key-to-screen latency, frame rate under flood, memory per pane)**
   - PLAN-02 covers latency measurement (key-to-screen via wakeup+snapshot timestamps) ✓
   - Frame rate measurement and memory profiling: **NOT in these plans** ⚠

---

## Deviations & Clarifications

### D1: Slot Index vs. PaneId

**Observation:** ROADMAP says `HashMap<PaneId, Vec<u64>>` but plan implements `HashMap<usize, Vec<u64>>` (slot index).

**Assessment:** **ACCEPTABLE TRADE-OFF**
- Slot indices (0, 1, 2...) are positional per frame; the same pane may occupy different slots after resize or reorder
- PaneId would require a map from slot to PaneId, adding complexity
- Plan's approach is simpler: hash storage is ephemeral per-frame, evicted by `resize()`
- Stale entries beyond current pane count are harmless (never accessed in the loop)
- No functional difference: all rows are re-shaped on pane close anyway (no row history retained)

**Rationale from plan:** Correct and conservative. Line 61 explicitly justifies this choice.

### D2: Incomplete Frame Pacing Implementation

**Observation:** Phase 13 scope includes PTY output coalescing and `ControlFlow::WaitUntil`, but these plans only cover `PresentMode::Fifo`.

**Assessment:** **PARTIAL IMPLEMENTATION**
- `PresentMode::Fifo` alone provides VSync and stops idle GPU spinning
- PTY coalescing (batching rapid writes) is a separate optimization not included here
- `ControlFlow::WaitUntil` (idle event loop pacing) is also not included
- These may be deferred or handled outside Phase 13

**Note from ROADMAP:** Phase 13 is parallel with Phase 14 (stabilization), so coordination may explain the split scope.

**Risk:** Frame pacing is only partially complete. Manual smoke tests (PLAN-01 section) check for tearing and idle CPU, which should surface incomplete pacing.

### D3: Performance Measurement Scope

**Observation:** ROADMAP requires "frame rate under flood, memory per pane" but plans only cover latency tracing.

**Assessment:** **DEFERRED**
- Key-to-screen latency is the most critical measurement (covered by PLAN-02)
- Frame rate under flood and per-pane memory profiling are secondary
- Latency-trace feature provides infrastructure; frame rate and memory can be added later via external tools or additional instrumentation

---

## Feasibility Analysis

### Task Complexity Tiers

| Plan | Task | Complexity | Estimated Effort | Comments |
|------|------|-----------|-------------------|----------|
| PLAN-01 | 1 | Trivial | 5 min | Delete one line |
| PLAN-01 | 2 | Medium | 30-60 min | Multi-crate refactor, borrow splitting, signature change propagation |
| PLAN-01 | 3 | Trivial | 5 min | Replace conditional with assignment |
| PLAN-02 | 1 | Trivial | 5 min | Add two log statements |
| PLAN-02 | 2 | Simple | 10-15 min | Add timing around existing function calls |

### Risk Factors

| Factor | Risk | Mitigation |
|--------|------|-----------|
| Borrow splitting in `render_multipane` (PLAN-01 Task 2) | Medium | Plan correctly notes field-level split borrow is sound. Compile will verify. |
| Propagating `prepare_grid_at` signature change (PLAN-01 Task 2) | Low | Only one call site (renderer.rs line 189); easy to audit and update. |
| Adding Fifo present mode (PLAN-01 Task 3) | Low | wgpu::PresentMode::Fifo is stable API; no conditional needed. |
| Latency-trace feature gating (PLAN-02) | Low | Feature already defined and used throughout main.rs; pattern is familiar. |

---

## Verification Command Assessment

### PLAN-01

1. Task 1: `cargo check -p arcterm-render` — Runnable, appropriate ✓
2. Task 2: `cargo check -p arcterm-render -p arcterm-app` — Runnable, appropriate ✓
3. Task 3: `cargo check -p arcterm-render` — Runnable, appropriate ✓
4. Full build: `cargo build -p arcterm-app` — Runnable, appropriate ✓
5. Smoke tests: Log-based (add temporary `log::trace!` calls) — Verifiable manually ✓

### PLAN-02

1. Task 1: `cargo check -p arcterm-app --features latency-trace` — Runnable, appropriate ✓
2. Task 2: `cargo check -p arcterm-app --features latency-trace` — Runnable, appropriate ✓
3. Feature-gated build: `cargo build -p arcterm-app --features latency-trace` — Runnable ✓
4. Default build: `cargo build -p arcterm-app` — Runnable (confirms zero-cost) ✓
5. Smoke test: `RUST_LOG=debug cargo run --features latency-trace` — Runnable, appropriate ✓

---

## Summary of Findings

### Strengths

1. **Well-scoped:** Both plans respect the 3-task limit; work is appropriately sized
2. **File references accurate:** All paths exist, line numbers are in bounds
3. **API surface verified:** `prepare_grid_at`, `hash_row`, `reset_frame`, `pane_buffer_pool` all exist with expected signatures
4. **Dependencies correct:** PLAN-02 depends on PLAN-01; no circular dependencies
5. **No file conflicts:** Both plans can execute sequentially without merge issues
6. **Requirements covered:** All Phase 13 success criteria are addressed (though frame pacing is partial)
7. **Verification commands runnable:** All `cargo check` and `cargo build` commands can be executed as-is
8. **Backward compatibility:** Feature-gating and optional fields preserve existing behavior

### Cautions

1. **Incomplete frame pacing:** PTY output coalescing and `ControlFlow::WaitUntil` are not included; only VSync (Fifo) is implemented
2. **Partial performance measurement:** Key-to-screen latency covered; frame rate and memory profiling deferred
3. **Slot index vs. PaneId:** Minor deviation from ROADMAP, but justified and correct
4. **Manual smoke tests required:** Dirty-row skip verification requires temporary logging; frame rate and tearing verification is visual/manual

### Recommendations

1. **Execute as planned:** Both plans are ready for execution
2. **Expect partial results:** Frame pacing will stop idle GPU spinning and prevent tearing, but PTY batching remains to be addressed elsewhere
3. **Manual verification essential:** Run the smoke tests (especially `cat /dev/urandom` test) to confirm multi-pane rendering isolation
4. **Defer frame rate & memory profiling:** These can be added in Phase 14 or later; they are non-critical for the Phase 13 milestone

---

## Verdict

**READY for execution**

Both plans are well-written, feasible, and correctly scoped. File references are accurate, API surfaces match, verification commands are runnable, and dependencies are properly ordered. No blocking issues detected.

**Proceed with execution in sequence:** PLAN-01 (Wave 1) → PLAN-02 (Wave 2).
