# Phase 15 Plan Critique: Event Handling & Exit Hardening

**Date:** 2026-03-16
**Reviewer:** Verification Engineer
**Status:** REVIEW REQUIRED — Critical Issue Found

---

## Executive Summary

Phase 15 consists of 2 plans across 2 waves (6 tasks total). The plans have **good overall structure and coverage** of the 8 success criteria, but **PLAN-01 Task 1 contains a critical code reference error** that will cause it to fail. The error must be corrected before execution. Additionally, several plans would benefit from minor clarifications and stronger verification commands.

---

## Coverage Analysis

### Success Criteria Mapping

All 8 Phase 15 success criteria are covered:

| Criterion | Covered By | Status |
|-----------|-----------|--------|
| `exit` in single-pane closes window immediately | PLAN-01 Task 2 | ✓ Covered |
| Closing split side promotes sibling to full size | PLAN-02 Task 1, 3 | ✓ Covered |
| Closing last pane closes window | PLAN-01 Task 2, PLAN-02 Task 1 | ✓ Covered |
| Multiple panes exit simultaneously without panic | PLAN-02 Task 1 | ✓ Covered |
| Drag-resize doesn't cause visible lag | PLAN-02 Task 2 | ✓ Covered |
| PresentMode logged at startup | PLAN-01 Task 3 | ✓ Covered |
| Reader thread EOF sends final wakeup | PLAN-01 Task 1 | ✓ Covered |
| All tests pass, no latency regressions | Both plans (implicit) | ✓ Covered |

**Verdict:** All success criteria have at least one task assigned. Coverage is complete.

---

## Plan Structure & Complexity

### PLAN-01: EOF Wakeup, Auto-Close Exit, Fifo Logging (Wave 1)

**Tasks:** 3
**Wave:** 1 (no dependencies)
**Files touched:**
- `arcterm-app/src/terminal.rs` (Task 1)
- `arcterm-app/src/main.rs` (Task 2)
- `arcterm-render/src/gpu.rs` (Task 3)

**Complexity assessment:**
- Task 1: Small (add clone + 2 wakeup sends) — **5-10 lines**
- Task 2: Medium (remove banner + exit field + handlers) — **~50 lines deleted** ✓
- Task 3: Small (upgrade logging + add Fifo check) — **5-10 lines**

**Overall:** 3 tasks, all small-to-medium, independent changes. Good parallelization candidate. ✓

### PLAN-02: Layout Cleanup, Multi-Pane Exit, Resize Coalescing (Wave 2)

**Tasks:** 3
**Wave:** 2 (depends on PLAN-01)
**Files touched:**
- `arcterm-app/src/main.rs` (Tasks 1, 2)
- `arcterm-app/src/layout.rs` (Task 3)

**Complexity assessment:**
- Task 1: Medium (rewrite pane removal loop, add layout.close + focus update) — **20-30 lines**
- Task 2: Medium (add pending_resize field, defer resize) — **15-25 lines**
- Task 3: Small (audit close() for edge cases, add comment) — **5 lines**

**Overall:** 3 tasks, Tasks 1-2 modify the same file (`main.rs`) but different logical sections. Task 3 is a code audit/documentation task. Sequencing is appropriate. ✓

---

## Wave Ordering & Dependencies

**Wave 1 (PLAN-01):** No dependencies. Can execute immediately. ✓

**Wave 2 (PLAN-02):** Depends on PLAN-01. Correctly specified in `dependencies: [01]`. ✓

**Rationale for dependency:** Task 1 in PLAN-02 modifies the same pane removal loop that Task 2 in PLAN-01 rewrites (the shell_exited removal and immediate event_loop.exit). Executing in sequence avoids merge conflicts and ensures the loop structure is stable before PLAN-02 adds layout.close() logic. ✓

---

## File Conflict Analysis

### Wave 1 (PLAN-01)

No conflicts — each task touches a different file:
- Task 1: `terminal.rs`
- Task 2: `main.rs`
- Task 3: `gpu.rs`

### Wave 2 (PLAN-02)

Tasks 1 and 2 both touch `main.rs` but at **different logical sections**:
- Task 1: Pane removal loop (~line 1682-1687 + ~line 1679)
- Task 2: WindowEvent::Resized handler (~line 1751) + about_to_wait (~line 1689 area)

**Minimal conflict risk** — both sections are separated. Sequential execution or careful merge handles this. ✓

Task 3 touches `layout.rs` (different file). ✓

---

## Feasibility Check: API Surface & File Paths

All referenced files exist and contain the code described. Verification via `ls`:

```
✓ /Users/lgbarn/Personal/arcterm/arcterm-app/src/terminal.rs (45,571 bytes)
✓ /Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs (161,035 bytes)
✓ /Users/lgbarn/Personal/arcterm/arcterm-app/src/layout.rs (37,179 bytes)
✓ /Users/lgbarn/Personal/arcterm/arcterm-render/src/gpu.rs (3,972 bytes)
```

### API Surface Verification

**PLAN-01 Task 1 (Reader thread EOF wakeup):**
- ✓ `std_mpsc::Sender<()>` exists and is cloneable (standard library)
- ✓ `reader.read()` returns `Ok(0)` on EOF (standard library)
- ✓ Reader thread at line 361-432 (verified in terminal.rs)
- ✓ Break at line 377 (Ok(0) arm) and line 388 (Err arm)
- **CRITICAL ISSUE:** `wakeup_tx` is moved into `listener` at line 289, not in scope at line 361 (see below)

**PLAN-01 Task 2 (Auto-close exit):**
- ✓ `event_loop.exit()` is called in other locations (line 1021, 1030, 1270, 1741, 2397, etc.)
- ✓ `event_loop` parameter available in `about_to_wait(&mut self, event_loop: &ActiveEventLoop)`
- ✓ `state.panes.is_empty()` is a HashMap check (safe)
- ✓ `shell_exited` field exists at line 560
- ✓ Banner code block at lines 2083+ exists for removal
- ✓ Keyboard exit handler at lines 2394-2399 exists for removal

**PLAN-01 Task 3 (PresentMode logging):**
- ✓ `GpuState::new()` exists at line 21 in gpu.rs
- ✓ `surface.get_capabilities()` called at line 53
- ✓ `wgpu::PresentMode::Fifo` is a standard enum variant
- ✓ `log::debug!` exists at line 58 (can upgrade to info!)

**PLAN-02 Task 1 (Layout tree cleanup):**
- ✓ `state.tab_manager.active` exists (tab index)
- ✓ `state.tab_layouts[active]` is accessible (Vec<PaneNode>)
- ✓ `PaneNode::close(id)` exists at line 398 in layout.rs
- ✓ `all_pane_ids()` method exists at line 216 in layout.rs, returns `Vec<PaneId>`
- ✓ `state.set_focused_pane()` method exists at line 673 in main.rs

**PLAN-02 Task 2 (Resize coalescing):**
- ✓ `AppState` struct exists
- ✓ `WindowEvent::Resized` handler exists at line 1751
- ✓ `about_to_wait` exists at line 1264
- ✓ `state.window.request_redraw()` is used elsewhere

**PLAN-02 Task 3 (Audit close() method):**
- ✓ `PaneNode::close()` exists at line 398 in layout.rs

---

## Critical Issues

### ISSUE-1: PLAN-01 Task 1 — `wakeup_tx` Not in Scope

**Severity:** BLOCKING ❌

**Description:**
The plan says to clone `wakeup_tx` at line ~361 (before the reader thread builder):
```rust
let wakeup_tx_for_reader = wakeup_tx.clone();
```

However, `wakeup_tx` is moved into `listener` at line 289:
```rust
let listener = ArcTermEventListener {
    wakeup_tx,  // <-- MOVED HERE
    write_tx: write_tx.clone(),
    ...
};
```

By line 361, `wakeup_tx` is no longer in scope and cannot be cloned. This is a **use-after-move error**.

**Fix required:**
Either:
1. Clone `wakeup_tx` **before** the listener is created (line ~287):
   ```rust
   let wakeup_tx_for_reader = wakeup_tx.clone();
   let listener = ArcTermEventListener { wakeup_tx, ... };
   ```
   Then use `wakeup_tx_for_reader` in the reader thread.

2. Or: Make the listener creation more explicit by storing `wakeup_tx_clone` separately:
   ```rust
   let wakeup_tx_clone = wakeup_tx.clone();
   let listener = ArcTermEventListener { wakeup_tx, ... };
   // ...
   // In reader thread closure, use wakeup_tx_clone
   ```

**Recommendation:** Update PLAN-01 Task 1's action to add the clone at line ~284 (immediately after `wakeup_tx` is created), before the listener is constructed. Update the line reference from "~361" to "~284".

---

### ISSUE-2: PLAN-02 Task 2 — `pending_resize` Field Initialization

**Severity:** LOW 🟢

**Description:**
The plan adds `pending_resize: Option<PhysicalSize<u32>>` to `AppState` and initializes it to `None`. However, it does not specify where to add this field initialization in `AppState::new()` or other constructors.

**Impact:** Builder will need to find and update all `AppState` construction sites.

**Recommendation:** Either:
1. Add exact line numbers for all constructor locations, or
2. Specify that `Option::default()` (which is `None`) handles this implicitly if the field is added to the struct.

---

## Verification Commands Quality

### PLAN-01

| Task | Verify Command | Quality |
|------|---|---|
| Task 1 | `cargo build -p arcterm-app 2>&1 \| tail -5` | Good (build only) — should also grep for `wakeup_tx_for_reader` send call |
| Task 2 | `cargo build -p arcterm-app 2>&1 \| tail -5 && grep -c "shell_exited" arcterm-app/src/main.rs \| grep "^0$"` | **EXCELLENT** — verifies both build and field removal |
| Task 3 | `cargo build -p arcterm-render 2>&1 \| tail -5` | Good (build only) — should also grep for log::info output |

**Recommendation for Task 1 & 3:** Add grep to verify the actual change took effect, e.g.:
- Task 1: `cargo build && grep "wakeup_tx_for_reader.send" arcterm-app/src/terminal.rs`
- Task 3: `cargo build && grep "log::info.*fifo" arcterm-render/src/gpu.rs`

### PLAN-02

| Task | Verify Command | Quality |
|------|---|---|
| Task 1 | `cargo build -p arcterm-app 2>&1 \| tail -5` | Good (build) — should verify `layout.close(id)` call exists |
| Task 2 | `cargo build -p arcterm-app 2>&1 \| tail -5` | Good (build) — should verify `pending_resize` field and deferred resize logic |
| Task 3 | `cargo build -p arcterm-app 2>&1 \| tail -5` | Good (build) — audit is documentation task, so build suffices |

**Recommendation:** For Tasks 1-2, add grep verification of the actual code changes rather than relying solely on build success.

---

## Task Count & Scope

| Plan | Tasks | Scope | Status |
|------|-------|-------|--------|
| PLAN-01 | 3 | Small (~80 lines total) | ✓ Reasonable |
| PLAN-02 | 3 | Medium (~40-50 lines total) | ✓ Reasonable |

**Max 3 tasks per plan:** Both comply. ✓

---

## Forward References & Circular Dependencies

**Wave 1 (PLAN-01):** No forward references. All tasks are self-contained. ✓

**Wave 2 (PLAN-02):**
- Task 1 depends on Task 2 from PLAN-01 (pane removal loop rewrite) ✓
- Task 2 (resize coalescing) is independent within PLAN-02 ✓
- Task 3 (audit) is independent and documents existing code ✓

**No circular dependencies detected.** ✓

---

## Additional Observations

### Design Document
The phase references a design document: `.shipyard/designs/2026-03-17-event-handling-design.md`
Verification: This file exists ✓

### Test Coverage
Neither plan explicitly includes new tests, but the plans are small bug-fixes/refactors that should be covered by existing test suite. Appropriate for Phase 15.

### Risk Assessment
The plan's own risk section is accurate:
> "Layout tree mutations during pane close are the most complex item."

PLAN-02 Task 1 & Task 3 address this directly with careful refactoring and auditing. ✓

### Manual Verification
PLAN-01 Task 2 has manual verification instructions in the plan (implied in task description), but could be more explicit. For example:
- Launch single-pane session
- Type `exit`
- Verify window closes immediately without banner

---

## Summary of Findings

| Item | Status | Notes |
|------|--------|-------|
| Success criteria coverage | ✓ PASS | All 8 criteria covered |
| Plan structure (≤3 tasks) | ✓ PASS | Both plans within limit |
| Wave ordering | ✓ PASS | Correct dependencies |
| File conflicts | ✓ SAFE | No blocking conflicts |
| File paths exist | ✓ PASS | All 4 files verified |
| API surface match | ⚠ CRITICAL | wakeup_tx not in scope at line 361; all_pane_ids() verified ✓ |
| Verification commands | ✓ GOOD | PLAN-01 Task 2 is excellent example |

---

## Recommendations Before Execution

### MUST FIX

1. **PLAN-01 Task 1:** Update the action text to clone `wakeup_tx` at line ~284 (immediately after creation, before listener construction), not at line ~361. The current line reference is incorrect and will cause a compile error.

   Current (WRONG):
   ```
   Before the `std::thread::Builder::new().name("arcterm-pty-reader")` block (~line 361),
   clone `wakeup_tx`: `let wakeup_tx_for_reader = wakeup_tx.clone();`
   ```

   Should be:
   ```
   After `let (wakeup_tx, wakeup_rx) = std_mpsc::channel::<()>();` (~line 284),
   and BEFORE the `let listener = ArcTermEventListener { wakeup_tx, ... }` block (~line 288),
   clone `wakeup_tx`: `let wakeup_tx_for_reader = wakeup_tx.clone();`

   Then move the reader thread closure creation to use `wakeup_tx_for_reader` instead.
   ```

### SHOULD DO

2. **PLAN-02 Task 1:** Verify that `PaneNode::all_pane_ids()` method exists in `layout.rs` (line references provided in plan assume it exists but plan does not cite it). If not found, update the plan task to construct the pane ID list explicitly or find the correct method name.

3. **Both plans:** Enhance verification commands to grep for the actual code changes, not just build success. Examples:
   - Task 1: `grep "wakeup_tx_for_reader.send" arcterm-app/src/terminal.rs`
   - Task 3: `grep "log::info.*fifo\|fifo_available" arcterm-render/src/gpu.rs`

4. **PLAN-02 Task 2:** Specify all `AppState` constructor locations that need `pending_resize: None` initialization, or confirm that the Option default handles it implicitly.

---

## Verdict

**STATUS: REVIEW REQUIRED ❌**

**Reason:** PLAN-01 Task 1 contains a code reference error that will cause compilation to fail. The error is straightforward to fix but must be corrected before execution.

**Action:** Update PLAN-01 Task 1's action text to correct the line reference (from ~361 to ~284) and clarify that `wakeup_tx` must be cloned before it's moved into the listener. Once corrected, both plans are well-structured and feasible.

**Confidence:** HIGH — The fix is a simple line-number correction to existing logic that is otherwise sound.
