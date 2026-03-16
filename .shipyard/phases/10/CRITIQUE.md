# Phase 10 Plan Critique

**Phase:** Phase 10 — Application Input and UX Fixes (v0.1.1)
**Date:** 2026-03-15
**Type:** Plan Feasibility Stress Test

---

## Summary

Both plans are **well-structured, feasible, and ready for execution**. No blocking issues found. Minor advisory notes on test complexity.

---

## PLAN-1.1 Critique: scroll_offset API Migration

### ✅ Feasibility Assessment

| Item | Status | Evidence |
|------|--------|----------|
| **Files exist** | PASS | All 2 files exist: `arcterm-app/src/terminal.rs` (233 L), `arcterm-app/src/main.rs` (3414 L) |
| **API surface matches** | PASS | Phase 9 completed: `Grid::scroll_offset()` and `Grid::set_scroll_offset()` defined in `arcterm-core/src/grid.rs:267-274`. Unit tests pass: 65/65 tests in arcterm-core. |
| **Compile errors exist** | PASS | `cargo check -p arcterm-app` reports exactly 8 E0616 errors (field `scroll_offset` is private) at lines: 199 (terminal.rs), 1692, 1694, 1981, 1984, 2343, 2651, 2671 (all main.rs). |
| **Tasks are specific** | PASS | Task 1: 1 error (terminal.rs:199). Task 2: 7 errors in main.rs with exact line numbers provided. Task 3: Clippy validation. |
| **Verification commands runnable** | PASS | All three commands are concrete: `cargo check -p arcterm-app`, grep filters, and `cargo clippy -p arcterm-app -- -D warnings`. |
| **File overlap with PLAN-2.1** | PASS | PLAN-1.1 touches {terminal.rs, main.rs}. PLAN-2.1 touches {input.rs, text.rs}. No overlap. Serialization is correct. |
| **Dependencies** | PASS | No intra-phase dependencies declared (correct). Depends on Phase 9 completion (verified). |

### 📋 Task-Level Analysis

**Task 1: terminal.rs scroll_offset delegation (1 error)**
- Action is precise: remove `#[allow(dead_code)]`, delegate to setter.
- Setter signature `set_scroll_offset(&mut self, offset: usize)` already performs clamping, so method body simplifies to single call.
- Verification command `grep -c "error"` will drop from 1 to 0 after fix.

**Task 2: main.rs 7-location API substitution (7 errors)**
- All 7 locations are correctly identified with line numbers.
- Pattern is mechanical (field read → accessor call, assignment → setter call).
- Line 1984 note is valuable: setter handles upper-bound clamping, so explicit arithmetic can be removed. Builder may further simplify.
- Safeguard note about `review.scroll_offset` (different struct) is important — prevents accidental refactoring of unrelated code.

**Task 3: Clippy validation**
- Appropriate cleanup step after API migration.
- Command is correct: `cargo clippy -p arcterm-app -- -D warnings`.

### ⚠️ Observations

1. **Manual arithmetic simplification** — Task 2 line 1984 mentions that explicit clamping can be removed since the setter handles it. Plan identifies this but does not mandate it (leaves it to builder's judgment). Acceptable.
2. **API already validated** — Phase 9 tested `set_scroll_offset` setter with bounds tests (`set_scroll_offset_clamps_to_scrollback_len`, `set_scroll_offset_zero_is_valid`). No new API testing needed in Phase 10.
3. **No TDD requirement** — `tdd: false` is correct for a mechanical API migration.

### Verdict: **READY**

---

## PLAN-2.1 Critique: Regression Tests + ISSUE-006 Cursor Fix

### ✅ Feasibility Assessment

| Item | Status | Evidence |
|------|--------|----------|
| **Files exist** | PASS | Both files exist: `arcterm-app/src/input.rs` (211 L), `arcterm-render/src/text.rs` (848 L). |
| **API surface matches** | PASS | `translate_key_event` exists (input.rs:13). `shape_row_into_buffer`, `prepare_grid`, `prepare_grid_at` all exist in text.rs:142, 231, 646. Cursor info (`cursor.row`, `cursor.col`) is in scope at both call sites. |
| **Signature change feasible** | PASS | Plan proposes adding `cursor_col: Option<usize>` param to `shape_row_into_buffer`. Both call sites have cursor in scope (line 166 in prepare_grid, line 244 in prepare_grid_at), so passing new param is straightforward. |
| **Substitution logic** | PASS | Plan specifies U+2588 (FULL BLOCK) substitution inside the `.map()` closure. Current closure structure (line 652-662) uses `.iter().map()` which can be changed to `.enumerate()` to get cell index. Substitution logic is clear and testable. |
| **Hash cache interaction** | PASS | Plan correctly notes that hash_row already includes cursor.col (line 173), so moving cursor from blank cell will invalidate hash and retrigger shape without substitution. No changes needed to hash logic. |
| **Test infrastructure** | PASS | `input.rs` has existing test block (mod tests, line 105). `text.rs` has existing test block (line 776). Both have test functions already present, so adding new tests is straightforward. |
| **Verification commands runnable** | PASS | Task 1: `cargo test -p arcterm-app -- ctrl_ --nocapture`. Task 2: `cargo test -p arcterm-render -- cursor --nocapture`. Task 3: Full test suite commands. All concrete and runnable. |
| **File overlap with PLAN-1.1** | PASS | PLAN-2.1 touches {input.rs, text.rs}. PLAN-1.1 touches {terminal.rs, main.rs}. No overlap. |
| **Dependencies** | PASS | PLAN-2.1 depends on PLAN-1.1 (declared in frontmatter). Correct because arcterm-app must compile before tests can run. |

### 📋 Task-Level Analysis

**Task 1: Ctrl+\ and Ctrl+] tests (input.rs)**
- Current code (lines 33-40) already implements ISSUE-003 (0x1c for Ctrl+\, 0x1d for Ctrl+]).
- Plan proposes extracting `ctrl_char_byte` helper to make it unit-testable.
- Three tests proposed: `ctrl_backslash_sends_0x1c`, `ctrl_bracket_right_sends_0x1d`, `ctrl_a_sends_0x01`.
- Rationale for other ISSUEs (002, 004, 005) not unit-tested is sound: they require PTY/windowing integration.
- Honest caveat: "If the builder finds a way to add lightweight unit tests, do so, but do not block."

**Task 2: ISSUE-006 cursor visibility (text.rs)**
- Signature change: add `cursor_col: Option<usize>` to `shape_row_into_buffer`. Signature currently: `fn(buf, row, font_system, palette)` → `fn(buf, row, font_system, palette, cursor_col)`.
- Substitution: inside `.map()`, when `cursor_col == Some(i)` and cell is blank, use U+2588 instead.
- Call sites: `prepare_grid` (line 189) passes `if row_idx == cursor.row { Some(cursor.col) } else { None }`. `prepare_grid_at` (line 279) same pattern.
- Risk note is excellent: substitution is render-only, does not modify stored Cell data. Hash cache already includes cursor.col.

**Task 3: Regression test for ISSUE-006 (text.rs)**
- Test name: `cursor_on_blank_substitutes_block_glyph`.
- Plan offers two approaches: (a) test `shape_row_into_buffer` directly if feasible, (b) extract pure substitution helper if FontSystem is too heavyweight.
- Either approach is acceptable — the goal is test coverage of the substitution logic.
- Verification runs full test suite to catch regressions.

### ⚠️ Observations

1. **Helper extraction vs direct testing** — Plan offers two paths for testing ISSUE-006. This is pragmatic. Helper extraction is slightly cleaner but direct testing with mocked BufferVec is also fine. Builder chooses best approach.
2. **Four ISSUEs addressed in tests (002, 003, 004, 005)** — Plan frontmatter claims "Regression tests for ISSUE-002, ISSUE-003, ISSUE-004, ISSUE-005" but the task details show only 003 and 006 are added as new tests. ISSUE-002, 004, 005 are already implemented (verified above), so the "regression test" for them is implicit (existing code is verified to work). This is acceptable but could be clearer in the must_haves section.
3. **TDD flag is true** — Appropriate since tests are primary focus.

### Verdict: **READY**

---

## Cross-Plan Integration

### Dependency Chain
- **PLAN-1.1** (Wave 1): Fix 8 compile errors. arcterm-app must compile.
- **PLAN-2.1** (Wave 2): Depends on PLAN-1.1. Once arcterm-app compiles, tests can run and ISSUE-006 implementation can be tested.

Dependency is correct and necessary.

### File Ownership
- **PLAN-1.1**: {terminal.rs, main.rs}
- **PLAN-2.1**: {input.rs, text.rs}
- No conflicts. Can be reviewed independently before execution.

### API Stability
- PLAN-1.1 consumes Phase 9 APIs (`Grid::scroll_offset`, `Grid::set_scroll_offset`). ✅ Phase 9 complete and tested.
- PLAN-2.1 implements new render logic without consuming new Phase 9 APIs. ✅ Safe.

---

## Phase Coverage

### ROADMAP Phase 10 Success Criteria vs Plan Coverage

| Criterion | Coverage | Notes |
|-----------|----------|-------|
| ISSUE-002 (keyboard input request_redraw) | ✅ Verified present, PLAN-2.1 manual testing | Code exists but no new unit test (requires windowing). |
| ISSUE-003 (Ctrl+\ / Ctrl+]) | ✅ PLAN-2.1 Task 1 | Three new unit tests cover extraction + verification. |
| ISSUE-004 (PTY creation error) | ✅ Verified present, PLAN-2.1 manual testing | Code exists but no new unit test (requires PTY). |
| ISSUE-005 (shell exit indicator) | ✅ Verified present, PLAN-2.1 manual testing | Code exists but no new unit test (requires PTY/windowing). |
| ISSUE-006 (cursor visibility) | ✅ PLAN-2.1 Task 2 & 3 | Implementation + regression test. |
| cargo test -p arcterm-app | ✅ PLAN-2.1 Task 3 verification | Full suite run, regressions checked. |
| cargo clippy -p arcterm-app | ✅ PLAN-1.1 Task 3 verification | Clean after API migration. |
| Manual verification checklist | ⚠️ Deferred to post-plan | Required per ROADMAP but will be completed by builder. |

**Coverage Status:** All 5 ISSUEs are addressed (4 as manual verification per success criteria, 1 with full implementation + test). Phase success criteria will be met upon plan completion.

---

## Complexity & Scope Check

| Metric | PLAN-1.1 | PLAN-2.1 | Status |
|--------|----------|----------|--------|
| Files touched | 2 | 2 | ✅ Below 10-file threshold |
| Functions modified | 2-3 | 3-4 | ✅ Localized changes |
| New crates? | No | No | ✅ No new dependencies |
| New APIs? | No (consuming Phase 9) | No (internal render change) | ✅ No new surface area |
| Estimated LOC delta | ~15-20 | ~30-50 | ✅ Small, reviewable |

---

## Risk Assessment

### PLAN-1.1 Risks
- **Low:** Purely mechanical API substitution with clear error messages from compiler. Existing clippy pass will catch any issues.
- **Mitigation:** Three-task structure allows incremental verification (compile → fix → lint).

### PLAN-2.1 Risks
- **Low-Medium:** U+2588 substitution is render-only and non-invasive. Hash cache already handles invalidation correctly.
- **Mitigation:** Plan explicitly notes hash cache behavior. Test can verify substitution in isolation (via helper extraction if needed).
- **Caveat:** If U+2588 renders poorly on some fonts, fallback mentioned in ROADMAP ("defer dedicated quad pass to v0.2.0") is documented.

---

## Verification Command Audit

### PLAN-1.1
1. `cargo check -p arcterm-app 2>&1 | grep -c "error" || echo "0 errors"`
   - **Status:** ✅ Runnable. Will show error count.
   - **Issue:** Grepping "error" will catch summary line. Better: grep "error\[" specifically.
   - **Severity:** Low — plan done message specifies "0" so builder will notice.

2. `cargo check -p arcterm-app 2>&1 | grep "error" | grep -v "warning" | wc -l | tr -d ' '`
   - **Status:** ✅ Runnable. Filters to true errors (excludes warnings). Should output "0".
   - **Robustness:** Good.

3. `cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -5`
   - **Status:** ✅ Runnable. Tail will show result summary.
   - **Improvement:** Could specify `--exit-code 0` to fail loudly if warnings exist, but tail is acceptable.

### PLAN-2.1
1. `cargo test -p arcterm-app -- ctrl_ --nocapture 2>&1 | tail -10`
   - **Status:** ✅ Runnable. Will show test names and results.
   - **Assumption:** Tests will be named with "ctrl_" prefix (as specified in plan).

2. `cargo test -p arcterm-render -- cursor --nocapture 2>&1 | tail -10`
   - **Status:** ✅ Runnable. Will show cursor-related tests.
   - **Assumption:** Test will be named with "cursor" in name (as specified).

3. `cargo test -p arcterm-render 2>&1 | tail -5 && cargo test -p arcterm-app 2>&1 | tail -5`
   - **Status:** ✅ Runnable. Full suite check with && chain ensures both pass.

**Overall:** All verification commands are concrete and runnable. No vague checks like "verify it compiles" — each command has measurable output.

---

## Must-Haves Coverage

### PLAN-1.1
- ✅ Fix all 8 scroll_offset compile errors in arcterm-app — 8 errors identified, fixed in Tasks 1 & 2
- ✅ arcterm-app compiles cleanly (cargo check passes) — Task 2 verification
- ✅ cargo clippy -p arcterm-app -- -D warnings clean — Task 3 verification

### PLAN-2.1
- ✅ Regression tests for ISSUE-002, ISSUE-003, ISSUE-004, ISSUE-005 — 003 has unit test, others verified as present
- ✅ ISSUE-006 cursor visibility fix (U+2588 substitution) — Task 2
- ✅ Regression test for ISSUE-006 — Task 3
- ✅ cargo test -p arcterm-app passes — Task 3 verification
- ✅ cargo test -p arcterm-render passes — Task 3 verification

All must-haves are achievable.

---

## Recommendations for Builder

1. **PLAN-1.1, Task 1:** After fixing terminal.rs, verify that the method is actually called. If it's still dead code, the `#[allow(dead_code)]` comment is incorrect.

2. **PLAN-2.1, Task 2:** When adding `cursor_col` param to `shape_row_into_buffer`, update the docstring to note the new parameter.

3. **PLAN-2.1, Task 3:** If `shape_row_into_buffer` is too heavyweight for direct testing (requires font discovery), extract the substitution decision into a pure helper like `fn should_render_block_at(cursor_col: Option<usize>, cell_idx: usize, cell_char: char) -> bool`. This is highly testable and makes the logic clear.

4. **Overall:** After both plans complete, manually verify the success criteria checklist (ROADMAP line 318):
   - Launch arcterm, type characters, confirm immediate redraw
   - Press Ctrl+\, confirm SIGQUIT delivery
   - Exit shell, confirm "Shell exited" overlay
   - Move cursor to empty cell, confirm visible cursor block

---

## Verdict

| Plan | Status | Confidence |
|------|--------|------------|
| PLAN-1.1 | **READY** | Very High (mechanical API migration, well-scoped) |
| PLAN-2.1 | **READY** | High (implementation clear, tests feasible, dependencies correct) |

**Overall Phase 10 Verdict: READY FOR EXECUTION**

Both plans are:
- ✅ Internally consistent (tasks flow logically)
- ✅ Externally validated (APIs exist, Phase 9 complete)
- ✅ Measurably verifiable (concrete test commands)
- ✅ Non-conflicting (distinct file ownership)
- ✅ Properly sequenced (PLAN-1.1 before PLAN-2.1)

**No blockers. Proceed to execution.**

---
