# Phase 11 Plan Critique

**Date:** 2026-03-16
**Plans Reviewed:** PLAN-1.1.md, PLAN-1.2.md, PLAN-2.1.md
**Type:** Feasibility Stress Test

---

## Executive Summary

All three Phase 11 plans are **READY for execution**. The plans collectively cover all three phase success criteria (M-3, M-4, M-5), have correct dependencies and wave ordering, contain no file conflicts within waves, and feature testable acceptance criteria. No architectural issues or hidden dependencies were identified.

---

## Detailed Analysis

### 1. File Paths Verification

| File | Status | Notes |
|------|--------|-------|
| `arcterm-app/src/config.rs` | ✓ Exists | 706 lines; test suite present at line 419+ |
| `arcterm-app/src/terminal.rs` | ✓ Exists | 232 lines; `PendingImage` struct has `#[allow(dead_code)]` at line 13 |
| `arcterm-app/src/main.rs` | ✓ Exists | 3,415 lines; Renderer::new at line 1003; take_pending_images at line 1452 |
| `arcterm-render/src/gpu.rs` | ✓ Exists | 109 lines; three `.expect()` calls at lines 29, 38, 50 |
| `arcterm-render/src/renderer.rs` | ✓ Exists | 508 lines; Renderer::new at line 84 |
| `arcterm-core/src/grid.rs` | ✓ Exists | 1,286 lines; not directly touched by Phase 11 plans (scrollback interaction deferred) |

**Finding:** All referenced files exist at expected line numbers with stated content.

---

### 2. API Surface Verification

**Grep for key functions:**

| Function | Signature | Status |
|----------|-----------|--------|
| `GpuState::new` | `pub fn new(window: Arc<Window>) -> Self` | ✓ Found (gpu.rs:17) |
| `GpuState::new_async` | `async fn new_async(window: Arc<Window>) -> Self` | ✓ Found (gpu.rs:21) |
| `Renderer::new` | `pub fn new(window: Arc<Window>, font_size: f32) -> Self` | ✓ Found (renderer.rs:84) |
| `Terminal::new` | `pub fn new(...) -> Result<(Self, mpsc::Receiver<Vec<u8>>), PtyError>` | ✓ Found (terminal.rs:54); currently 2-tuple, to become 3-tuple |
| `Terminal::process_pty_output` | `pub fn process_pty_output(&mut self, bytes: &[u8])` | ✓ Found (terminal.rs:79); has inline `image::load_from_memory` at line 92 |
| `Terminal::take_pending_images` | `pub fn take_pending_images(&mut self) -> Vec<PendingImage>` | ✓ Found (terminal.rs:117); to be removed in PLAN-2.1 |
| `ArctermConfig::load` | `pub fn load() -> Self` | ✓ Found (config.rs:175) |
| `ArctermConfig::load_with_overlays` | `pub fn load_with_overlays() -> (Self, toml::Value)` | ✓ Found (config.rs:258) |

**Finding:** All referenced APIs exist and match expected signatures.

---

### 3. Compilation Verification

```bash
$ cargo check -p arcterm-app -p arcterm-render
Finished `dev` profile [unoptimum [unoptimized + debuginfo] target(s) in 8.42s
```

**Finding:** Current codebase compiles without errors or warnings.

---

### 4. Dependency and Wave Analysis

**Wave 1 (Parallel Execution):**
- PLAN-1.1: touches only `arcterm-app/src/config.rs`
- PLAN-1.2: touches `arcterm-render/src/gpu.rs`, `arcterm-render/src/renderer.rs`, `arcterm-app/src/main.rs`
- **File Overlap:** None. Plans can execute in parallel.
- **Dependencies Declared:** Both have `dependencies: []`. Correct.

**Wave 2 (Serialized):**
- PLAN-2.1: touches `arcterm-app/src/terminal.rs`, `arcterm-app/src/main.rs`
- **Dependencies Declared:** `dependencies: ["1.2"]`. Correct — PLAN-1.2 modifies main.rs at line 1003 (Renderer::new error handling); PLAN-2.1 modifies main.rs at line 1452 (image drain loop). No merge conflict if applied in order.

**Finding:** Wave ordering is correct and dependency declaration is sound.

---

### 5. File Conflict Analysis

| File | Plans Touching | Conflict Risk | Status |
|------|---|---|---|
| `arcterm-app/src/config.rs` | PLAN-1.1 | None | ✓ Safe |
| `arcterm-render/src/gpu.rs` | PLAN-1.2 | None | ✓ Safe |
| `arcterm-render/src/renderer.rs` | PLAN-1.2 | None | ✓ Safe |
| `arcterm-app/src/main.rs` | PLAN-1.2, PLAN-2.1 | Potential | ✓ Mitigated by dependencies |
| `arcterm-app/src/terminal.rs` | PLAN-2.1 | None | ✓ Safe |

**Analysis:** main.rs is touched by both Wave 1 (PLAN-1.2, line 1003) and Wave 2 (PLAN-2.1, line 1452). These edits are 450+ lines apart; no merge conflict if PLAN-1.2 executes first. The dependency declaration ensures this ordering.

**Finding:** No blocking conflicts; dependency on PLAN-1.2 by PLAN-2.1 ensures safe serialization.

---

### 6. Complexity Assessment

| Plan | File Count | Lines Modified | Complexity |
|------|---------|---|---|
| PLAN-1.1 | 1 | ~50 (1 method + 2 tests) | Low — isolated validation logic |
| PLAN-1.2 | 3 | ~30 (3 signature changes + error handling) | Medium — propagation through call stack |
| PLAN-2.1 | 2 | ~150 (struct change + async wiring + test) | High — async/channel refactoring |
| **Total** | **4 files** | **~230 lines** | **Medium-High** |

**Finding:** All plans stay well under the complexity cap. PLAN-2.1 is the most invasive but its changes are well-scoped (Terminal struct and process_pty_output method). No plan exceeds 3 tasks (PLAN-1.1: 1 task, PLAN-1.2: 2 tasks, PLAN-2.1: 3 tasks).

---

### 7. Success Criteria Coverage

**Phase 11 has three core requirements (M-3, M-4, M-5):**

| Criterion | Coverage | Plan | Evidence |
|-----------|----------|------|----------|
| **M-3:** Kitty image decode async via spawn_blocking | ✓ Fully Covered | PLAN-2.1 Tasks 1-3 | Terminal.rs struct change + main.rs channel integration + regression test |
| **M-4:** scrollback_lines capped at 1,000,000 | ✓ Fully Covered | PLAN-1.1 Task 1 | Config.rs validate() method + 2 regression tests |
| **M-5:** GpuState::new() returns Result | ✓ Fully Covered | PLAN-1.2 Tasks 1-2 | gpu.rs signature change + renderer.rs + main.rs error handling + compile-time verification |

**Additional Requirements:**

| Requirement | Status |
|---|---|
| Each fix includes regression test | ✓ PLAN-1.1 includes 2 tests; PLAN-2.1 includes 1 test; PLAN-1.2 uses compile-time verification (type system + clippy) |
| `cargo test --workspace` succeeds | ✓ Verifiable post-execution |
| `cargo clippy --workspace -- -D warnings` clean | ✓ All plans verify clippy passes; current baseline already clean |
| No new `.expect()` on fallible ops (app/render) | ✓ PLAN-1.2 Task 1 explicitly removes 3 `.expect()` calls from gpu.rs; Task 2 replaces main.rs call site with match |

**Finding:** All success criteria are explicitly mapped to plan tasks. Coverage is complete.

---

### 8. Acceptance Criteria Testability

| Plan | Task | Criterion | Testable | Method |
|---|---|---|---|---|
| PLAN-1.1 | 1 | scrollback_lines capped | ✓ Yes | `cargo test config::tests::scrollback_lines_capped_at_maximum` |
| PLAN-1.1 | 1 | scrollback_lines below cap unchanged | ✓ Yes | `cargo test config::tests::scrollback_lines_below_cap_unchanged` |
| PLAN-1.1 | 1 | existing tests pass | ✓ Yes | `cargo test --package arcterm-app` |
| PLAN-1.2 | 1 | GpuState::new returns Result<Self, String> | ✓ Yes | `cargo check --package arcterm-render` (type checking enforces this) |
| PLAN-1.2 | 2 | Renderer::new returns Result | ✓ Yes | `cargo check --package arcterm-render` |
| PLAN-1.2 | 2 | main.rs error handling | ✓ Yes | `cargo xc` (unused Result warning promoted to error) |
| PLAN-2.1 | 1 | Terminal struct has image_tx field | ✓ Yes | `cargo check --package arcterm-app` |
| PLAN-2.1 | 2 | All Terminal::new() call sites updated | ✓ Yes | `cargo check` (type mismatch on 2-tuple vs 3-tuple) |
| PLAN-2.1 | 2 | try_recv drain pattern | ✓ Yes | `cargo xc` (unused Result warning if pattern wrong) |
| PLAN-2.1 | 3 | async_image_decode_via_channel test | ✓ Yes | `cargo test terminal::tests::async_image_decode_via_channel` |

**Finding:** All acceptance criteria are verifiable via automated tooling (cargo check, cargo test, cargo xc). No subjective or manual-only criteria.

---

### 9. Dependency Verification

**Required Crates:**

| Crate | Required For | Status |
|---|---|---|
| `tokio` | PLAN-2.1 (spawn_blocking) | ✓ Available with "full" features in Cargo.toml |
| `tokio::sync::mpsc` | PLAN-2.1 (channels) | ✓ Already imported in terminal.rs:7 |
| `image` | PLAN-2.1 (image decode) | ✓ Available (image = "0.25" in Cargo.toml) |
| `log` | PLAN-1.1 (log::warn) | ✓ Available (log = "0.4" in Cargo.toml); already used in config.rs |

**Finding:** All required dependencies are available. No new dependency additions needed.

---

### 10. Edge Cases and Risks

| Risk | Severity | Mitigation | Status |
|---|---|---|---|
| **M-5 propagation:** Renderer::new() returns Result; all call sites must handle it | Medium | PLAN-1.2 Task 2 explicitly updates main.rs line 1003 with match expression | ✓ Mitigated |
| **M-3 channel semantics:** One-frame image latency acceptable? | Low | PLAN-2.1 context notes this; try_recv in about_to_wait drains before frame render | ✓ Acknowledged |
| **Bounded channel (M-3):** Channel full drops images | Low | PLAN-2.1 Task 1 logs warning on send failure; capacity 32 is generous for most use cases | ✓ Mitigated |
| **M-4 validation scope:** Is validate() called in all load paths? | Low | PLAN-1.1 Task 1 calls validate() at line 194 (load) and 302 (load_with_overlays) | ✓ Complete |
| **Compile-time M-5 test:** No runtime test for GPU adapter failure | Low | ROADMAP acknowledges "if testable, otherwise integration-level assertion"; type system enforces Result handling | ✓ Acceptable |

**Finding:** All identified risks are acknowledged in plan text and mitigated. No unaddressed hazards.

---

### 11. Task Granularity

| Plan | Task | Lines of Code | Scope | Status |
|---|---|---|---|---|
| PLAN-1.1 | 1 | ~50 | Write test + implement validate() | ✓ Single responsibility |
| PLAN-1.2 | 1 | ~10 | GpuState signature + 3 `.expect()` → Result | ✓ Cohesive |
| PLAN-1.2 | 2 | ~20 | Renderer::new signature + main.rs error handling | ✓ Cohesive |
| PLAN-2.1 | 1 | ~30 | Terminal struct refactor: Vec → channel | ✓ Cohesive |
| PLAN-2.1 | 2 | ~100 | Terminal::new() return type + all 4 call sites in main.rs | ✓ Well-defined |
| PLAN-2.1 | 3 | ~50 | Async test with spawn_blocking pattern | ✓ Isolated |

**Finding:** All tasks are appropriately scoped. No task tries to do too much; each has a clear acceptance criterion.

---

### 12. Instruction Clarity

| Plan | Issue | Severity | Resolution |
|---|---|---|---|
| PLAN-1.2, Task 1, Line 45 | "Wrap the `pollster::block_on` call" is incomplete | Low | Intent is clear from context: `new()` should call `pollster::block_on(Self::new_async(window))` and return its Result. Instruction assumes reader understands return-type propagation. Acceptable. |

**Finding:** Minor clarity issue in PLAN-1.2 Task 1, but intent is unambiguous and recoverable from context. No show-stopper.

---

## Verification of Specific Concerns

### Concern: "Can 4 Terminal::new() call sites all be updated?"

**Analysis:** Grep confirms exactly 4 call sites in main.rs (lines 347, 830, 910, 1061). PLAN-2.1 Task 2 explicitly states "four call sites" and instructs destructuring the 3-tuple at each. Compile-time error (type mismatch on 2-tuple) will catch any missed sites.

**Verdict:** ✓ Safe; compiler will enforce correctness.

---

### Concern: "Will main.rs edits from PLAN-1.2 and PLAN-2.1 conflict?"

**Analysis:**
- PLAN-1.2 modifies line 1003 (Renderer::new call)
- PLAN-2.1 modifies line 1452 (image drain loop)
- 449-line separation minimizes merge conflict risk
- Dependency declaration (PLAN-2.1 depends on PLAN-1.2) ensures serial application

**Verdict:** ✓ Safe; no conflict if applied in dependency order.

---

### Concern: "Will PLAN-2.1 compile without PLAN-1.2 completed?"

**Analysis:** PLAN-2.1 changes Terminal::new return type and main.rs image drain. If main.rs still has old Renderer::new call site, PLAN-2.1 will fail at cargo check. However, the declared dependency ensures PLAN-1.2 executes first. If builder ignores the dependency, compilation will fail loudly (as intended to catch the ordering violation).

**Verdict:** ✓ Safe; dependency enforces correct order; failure mode is obvious.

---

## Verdict

### **Status: READY**

All three Phase 11 plans are ready for execution. No blockers, no hidden dependencies, no architectural issues. Plans are well-scoped, testable, and correctly sequenced.

### Recommended Execution Order

1. **Wave 1 (Parallel):** Execute PLAN-1.1 and PLAN-1.2 in parallel.
2. **Wave 2 (Serialized):** Execute PLAN-2.1 after Wave 1 completes.

### Success Metrics (Post-Execution)

- All three plans individually report "DONE" status (test/check commands pass)
- `cargo test --workspace` passes (new test count > 558)
- `cargo clippy --workspace -- -D warnings` clean
- Full Phase 11 regression test suite passes
- All M-3, M-4, M-5 criteria met with evidence in VERIFICATION.md

---

## Notes for Builder

1. **PLAN-1.1:** Straightforward TDD pattern. Write tests first, then implement validate(). Existing config tests will validate no regression.

2. **PLAN-1.2:** The type system will enforce Result handling. If any call site is missed, compiler will raise "unused Result" warning (under `-D warnings`). Fix all clippy warnings before declaring the plan done.

3. **PLAN-2.1:** The most invasive plan. Pay attention to the 3-tuple destructuring at all 4 Terminal::new() call sites. The regression test for async decode is crucial to verify the channel pattern works correctly. Use `#[tokio::test]` for the async test.

4. **Dependencies:** PLAN-1.2 must complete before PLAN-2.1 can be applied safely. The builder tool should enforce this or at least warn if violated.

---

**Prepared by:** Verification Agent
**Date:** 2026-03-16
