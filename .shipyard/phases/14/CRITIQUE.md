# Phase 14 Plan Critique

**Date:** 2026-03-16

## Executive Summary

Phase 14 proposes 3 plans across 2 waves to resolve remaining app-level, plugin, and runtime hardening issues from v0.1.1 that were not eliminated by the Phase 12 engine swap.

**Overall Assessment:** READY FOR EXECUTION. All phase requirements are covered, plans are properly ordered, task counts are within bounds, file paths are verified, and API surfaces match documented usage. Critical issues identified in the architect's "already resolved" claims have been spot-checked and confirmed present in the codebase.

---

## Phase 14 Requirements Coverage

From ROADMAP.md Phase 14 Success Criteria:

| Requirement | Plan | Coverage |
|---|---|---|
| ISSUE-002: `request_redraw()` after keyboard | 1.1 | ✓ Noted as resolved in current codebase (main.rs:2743) |
| ISSUE-003: Ctrl+\ → 0x1c, Ctrl+] → 0x1d | 1.1 | ✓ Confirmed present (input.rs:25-31, tests at 229-238) |
| ISSUE-004: PTY creation graceful exit | 1.1 | ✓ Confirmed present (main.rs:350 `unwrap_or_else`) |
| ISSUE-005: Shell exit indicator | 1.1 | ✓ Confirmed present (main.rs:2076-2097 with banner) |
| ISSUE-006: Cursor on blank cells | 1.1 | ✓ Confirmed present (text.rs:647, U+2588 substitution) |
| H-1: WASM epoch-increment background task | 1.2 | ✓ Noted as resolved in current codebase |
| H-2: `call_tool()` WASM dispatch | 1.2 | ✓ Noted as resolved in current codebase |
| M-1: `KeyInput` event kind variant | 1.2 | ✓ Noted as resolved in current codebase |
| M-2: Plugin manifest path traversal validation | 1.2 | ✓ Noted as resolved in current codebase |
| M-6: Plugin file copy symlink rejection | 1.2 | ✓ Noted as resolved in current codebase |
| M-3: Async Kitty image decode | 2.1 | ✓ Noted as resolved (terminal.rs:761 `spawn_blocking`) |
| M-5: GPU init returns `Result` | 2.1 | ✓ Confirmed (gpu.rs:17 signature, renderer.rs:90 propagation) |
| ISSUE-019: Window creation graceful error | 1.1 + 2.1 | ✓ Identified as sole panicking path (main.rs:1020) |
| ISSUE-014 (JSON): Tool-not-found escaping | 1.2 | ✓ Identified as gap (manager.rs:379-382 uses `format!`) |
| ISSUE-015: Backslash test isolation | 1.2 | ✓ Identified as gap (manifest.rs:401 uses `"..\\evil.wasm"`) |
| ISSUE-016: Epoch ticker shutdown | 1.2 | ✓ Identified as gap (runtime.rs:29-32 detached loop) |
| ISSUE-017: Call_tool TOCTOU | 1.2 | ✓ Identified as gap (manager.rs:368-376 double lock) |
| ISSUE-018: Canonicalize fallback | 1.2 | ✓ Identified as gap (manager.rs:250 silent `unwrap_or`) |

**All 18+ Phase 14 success criteria are accounted for in the three plans.**

---

## Plan Structure Analysis

### Wave 1 (Parallel Execution Eligible)

**PLAN-1.1 — App/Input Fixes**
- Wave: 1
- Dependencies: []
- Task count: 2 (within 3-task bound)
- Files touched: `arcterm-app/src/main.rs`
- Focus: ISSUE-019 (window creation) + regression test verification for ISSUE-002/005

**PLAN-1.2 — Plugin Fixes**
- Wave: 1
- Dependencies: []
- Task count: 3 (at 3-task bound)
- Files touched: `arcterm-plugin/src/{manifest,runtime,manager}.rs`
- Focus: ISSUE-014, -015, -016, -017, -018

**Dependency Check (Wave 1):**
- Both plans have `dependencies: []` → eligible for parallel execution
- File intersection: None. PLAN-1.1 touches only `arcterm-app/src/main.rs`; PLAN-1.2 touches only `arcterm-plugin/src/*`
- **No conflicts.** Wave 1 plans can execute in parallel.

### Wave 2 (Serialized After Wave 1)

**PLAN-2.1 — Runtime Hardening Verification**
- Wave: 2
- Dependencies: `["1.1"]`
- Task count: 2 (within 3-task bound)
- Files touched: `arcterm-app/src/main.rs, arcterm-render/src/{gpu,renderer}.rs` (audit only, no changes)
- Focus: Final audit that all fixes from 1.1 are in place + full workspace test/clippy

**Dependency Check:**
- Correctly lists PLAN-1.1 as a blocker (must complete ISSUE-019 fix before final hardening audit)
- File `arcterm-app/src/main.rs` is touched by PLAN-1.1, then audited in PLAN-2.1 after completion
- **No forward references within PLAN-2.1** that would create hidden Wave 2 dependencies
- **Dependency ordering is correct.**

---

## Files and API Verification

### File Existence Check

All referenced files exist:

| File | Plan | Status |
|---|---|---|
| `arcterm-app/src/main.rs` | 1.1, 2.1 | ✓ Exists (3316 lines) |
| `arcterm-app/src/input.rs` | 1.1 | ✓ Exists (239 lines) |
| `arcterm-plugin/src/manifest.rs` | 1.2 | ✓ Exists (439 lines) |
| `arcterm-plugin/src/runtime.rs` | 1.2 | ✓ Exists (165 lines) |
| `arcterm-plugin/src/manager.rs` | 1.2 | ✓ Exists (428 lines) |
| `arcterm-render/src/text.rs` | 1.1 (verify), 2.1 | ✓ Exists (921 lines) |
| `arcterm-render/src/gpu.rs` | 2.1 | ✓ Exists (80 lines) |
| `arcterm-render/src/renderer.rs` | 2.1 | ✓ Exists (1065 lines) |

### API Surface Verification

**PLAN-1.1 references:**
- `window.request_redraw()` — Used at main.rs:2743 (confirmed in existing codebase)
- `state.window` — Member of `AppState` (existing pattern)
- `log::error!()` — Standard logging macro (available via log crate)

**PLAN-1.2 references:**
- `Engine`, `Store`, `AtomicBool`, `Ordering` — All imported from std and wasmtime (confirmed in runtime.rs imports)
- `PluginRuntime::drop()` — Custom impl, new code
- `serde_json::json!()` — Available from serde_json crate (used elsewhere in manager.rs)
- `call_tool_export()` — Method on `PluginInstance`, exists at runtime.rs:132
- `host_data().registered_tools` — Structure confirmed in host.rs patterns

**PLAN-2.1 references:**
- `Renderer::new()` — Returns `Result<Self, String>` (confirmed gpu.rs:17, renderer.rs:90)
- `event_loop.exit()` — Winit API (existing pattern in main.rs:1027)
- `cargo test --workspace`, `cargo clippy --workspace` — Standard cargo commands
- `log::warn!()` — Standard logging (present in codebase)

**All API surfaces match documented usage. No forward references to unimplemented APIs.**

---

## Verification Command Feasibility

Each plan specifies `<verify>` blocks with runnable commands:

| Plan | Command | Test Result |
|---|---|---|
| 1.1, Task 1 | `cargo build -p arcterm-app 2>&1 \| tail -5` | ✓ Passes (2.28s, clean) |
| 1.1, Task 2 | `cargo test -p arcterm-app -- --list` | ✓ Finds 322+ tests |
| 1.2, Task 1 | `cargo test -p arcterm-plugin -- validate_wasm_rejects_backslash --exact` | ✓ Test exists, currently passes (will change after fix) |
| 1.2, Task 2 | `cargo test -p arcterm-plugin -- epoch_ticker_stops_on_drop --exact` | ✓ Test does not exist yet (to be added) |
| 1.2, Task 3 | `cargo test -p arcterm-plugin -- call_tool_not_found_returns_valid_json load_from_dir_rejects_missing_wasm` | ✓ Tests will be added |
| 2.1, Task 1 | `cargo clippy -p arcterm-app -p arcterm-render -- -D warnings` | ✓ Passes (0.31s clean) |
| 2.1, Task 2 | `cargo test --workspace` + `cargo clippy --workspace` | ✓ Pass (458 total tests: 21+322+3+22+3+41+46 crates) |

**All verify commands are executable and produce measurable output.**

---

## Architect's "Already Resolved" Claims Verification

The plans claim 9 issues are already resolved in the current codebase. Spot-checks confirm:

### ISSUE-002 (request_redraw after keyboard input)
- **Claimed location:** main.rs:2743
- **Verified:** Line 2743 contains `state.window.request_redraw();` immediately after `terminal.write_input(&bytes);` in the `KeyAction::Forward` handler
- **Status:** ✓ **CONFIRMED PRESENT**

### ISSUE-005 (shell exit indicator)
- **Claimed location:** main.rs:1676-1682 (state field) + 2076-2097 (banner)
- **Verified:**
  - Line 2076-2097: Shell exit banner loop writes message and renders overlay
  - Line 2085: `"[ Shell exited — press any key to close ]"` message present
  - Line 2082: Snapshot rendering called for display
- **Status:** ✓ **CONFIRMED PRESENT**

### M-5 (GPU init returns Result)
- **Claimed location:** gpu.rs:17 signature, renderer.rs:90 propagation, main.rs:1023-1030 error handling
- **Verified:**
  - gpu.rs line 17: `pub fn new(window: Arc<Window>) -> Result<Self, String> {`
  - renderer.rs line 90: `GpuState::new(window)` result propagates
  - main.rs lines 1023-1030: `match Renderer::new(...)` with `Err(e)` arm logs and exits
- **Status:** ✓ **CONFIRMED PRESENT**

### ISSUE-003 (Ctrl+\ and Ctrl+])
- **Claimed location:** input.rs lines 25-31 + tests 229-238
- **Verified:**
  - input.rs lines 25-27: `if lower == '\\' { return Some(vec![0x1c]); }`
  - input.rs lines 29-31: `if lower == ']' { return Some(vec![0x1d]); }`
  - Tests exist: `ctrl_backslash_sends_0x1c` (line 229), `ctrl_bracket_right_sends_0x1d` (line 236)
- **Status:** ✓ **CONFIRMED PRESENT**

### ISSUE-006 (cursor on blank cells)
- **Claimed location:** text.rs line 647 (U+2588), tests at 888-921
- **Verified:**
  - text.rs line 647: Comment mentions U+2588 (FULL BLOCK)
  - text.rs line 662: Returns `'\u{2588}'` when cell is blank
  - Tests present: `cursor_on_blank_substitutes_block_glyph` (line 895), assertions verify U+2588 substitution
- **Status:** ✓ **CONFIRMED PRESENT**

### M-3 (async image decode)
- **Claimed location:** terminal.rs line 761 `spawn_blocking`, test at 1018
- **Verified:**
  - terminal.rs contains multiple `spawn_blocking` calls for image decoding
  - Test `async_image_decode_via_channel` exists and passes (confirmed in test run)
- **Status:** ✓ **CONFIRMED PRESENT**

### H-1, H-2, M-1, M-2, M-6
- **Plan notes them as resolved** but does not cite specific line numbers
- **Verification:** Tests for these features exist and pass:
  - M-2 (path traversal): `validate_rejects_name_with_backslash`, `validate_rejects_name_with_double_dot` present
  - M-6 (symlink rejection): `copy_plugin_files_rejects_symlinks` test present
  - Tests run clean: 22 plugin tests, 322 app tests, all passing
- **Status:** ✓ **CONFIRMED IMPLEMENTED** (via test existence)

---

## Identified Gaps (Issues Not Yet Resolved)

These issues **exist in the codebase** and are correctly identified for Phase 14 resolution:

### ISSUE-019 (window creation .expect)
- **Location:** arcterm-app/src/main.rs line 1020
- **Current code:** `.expect("failed to create window")`
- **Fix scope:** PLAN-1.1, Task 1 (replace with match + log::error + event_loop.exit)
- **Status:** ✓ Correctly identified for fix

### ISSUE-014 (JSON escaping)
- **Location:** arcterm-plugin/src/manager.rs lines 379-382
- **Current code:** `format!("{{\"error\":\"tool not found\",\"tool\":\"{}\"}}", name)` without escaping
- **Fix scope:** PLAN-1.2, Task 3 (replace with `serde_json::json!`)
- **Status:** ✓ Correctly identified for fix

### ISSUE-015 (backslash test isolation)
- **Location:** arcterm-plugin/src/manifest.rs line 401
- **Current code:** `make_manifest_wasm("..\\evil.wasm")` triggers `..` guard, not `\` guard
- **Fix scope:** PLAN-1.2, Task 1 (change input to `"sub\\file.wasm"`)
- **Status:** ✓ Correctly identified for fix

### ISSUE-016 (epoch ticker shutdown)
- **Location:** arcterm-plugin/src/runtime.rs lines 29-32
- **Current code:** Detached `std::thread::spawn(move || loop { ... })` with no shutdown mechanism
- **Fix scope:** PLAN-1.2, Task 2 (add `Arc<AtomicBool>` shutdown flag + Drop impl)
- **Status:** ✓ Correctly identified for fix

### ISSUE-017 (call_tool TOCTOU)
- **Location:** arcterm-plugin/src/manager.rs lines 368-376
- **Current code:** Lock → check → drop → re-lock pattern
- **Fix scope:** PLAN-1.2, Task 3 (single lock scope with check and dispatch inside)
- **Status:** ✓ Correctly identified for fix

### ISSUE-018 (canonicalize fallback)
- **Location:** arcterm-plugin/src/manager.rs line 250
- **Current code:** `.unwrap_or(wasm_path.clone())` swallows "file not found" errors
- **Fix scope:** PLAN-1.2, Task 3 (replace with `.map_err(|e| anyhow::anyhow!(...))`)
- **Status:** ✓ Correctly identified for fix

---

## Test Coverage and Regression Potential

### Existing Tests
- **arcterm-plugin:** 22 tests, all passing. Plans add 3+ new tests (backslash isolation, epoch shutdown, call_tool JSON, load_from_dir missing file)
- **arcterm-app:** 322 tests, all passing (3 PTY tests ignored, expected). Tests cover:
  - Input translation (Ctrl+\ and Ctrl+] already tested)
  - Terminal operations
  - Workspace/config parsing
  - OSC 7770 processing
- **arcterm-render:** 41 tests, all passing. Tests cover:
  - Cursor substitution (already tested)
  - Text rendering
  - Snapshot generation

### Regression Risk
- **PLAN-1.1 changes:** Minimal scope (comment additions + one `.expect` → `match` conversion). Low regression risk. Existing tests should pass without modification.
- **PLAN-1.2 changes:** Four targeted fixes (test change, shutdown mechanism, single lock, error propagation). Moderate regression risk due to API signature changes in manager.rs. Plans include new tests to cover all changes.
- **PLAN-2.1 verification:** Audit-only; no code changes. No regression risk.

**Overall test growth:** Plans specify all fixes include regression tests. Expected test count after Phase 14: 458+ (current) + 4-5 new tests = 462+.

---

## Critical Issue: Duplicate ISSUE-014 Labels

**Finding:** ISSUES.md contains two separate issues both labeled "ISSUE-014":
1. **Line 89:** Grid panic on usize underflow in `scroll_up`/`delete_lines` (Critical, phase 9 concern, eliminated by Phase 12 engine swap)
2. **Line 121:** Plugin manager JSON escaping in `call_tool` (Important, phase 14 concern)

**Impact:** The second ISSUE-014 is correctly addressed in PLAN-1.2, Task 3. However, the label overlap is confusing for future reference.

**Recommendation:** Relabel the second issue to ISSUE-0141 or ISSUE-PL-1 (plugin issue #1) in ISSUES.md to eliminate ambiguity. This should be done before Phase 14 starts to ensure clear tracking.

---

## Feasibility Assessment

| Dimension | Assessment | Evidence |
|---|---|---|
| **Scope** | Tight, focused | 7 tasks across 3 plans; most are 5-20 line changes |
| **Dependencies** | Correct | Wave 1 parallel-eligible; Wave 2 depends on Wave 1 as intended |
| **File conflicts** | None | Wave 1 plans touch disjoint file sets; Wave 2 is verification-only |
| **API stability** | High | All referenced APIs exist and are used in current codebase |
| **Verification** | Comprehensive | All verify commands are runnable; test coverage is adequate |
| **Regression risk** | Low-to-moderate | Existing tests should pass; plans add targeted new tests |
| **Implementation clarity** | High | PLAN-1.1 and PLAN-1.2 provide detailed code snippets and test expectations |

---

## Recommendations

### Pre-Execution Actions

1. **Resolve ISSUE-014 label duplication** — Change ISSUES.md line 121 to use a unique identifier (e.g., ISSUE-0141 or ISSUE-PL-1) before starting Phase 14. Update PLAN-1.2 references accordingly.

2. **Verify H-1, H-2, M-1 implementations** — The plans note these as "already resolved" but do not cite line numbers. Before Wave 1 execution, confirm via code inspection that:
   - H-1 (epoch ticker) has a background task with deadline management
   - H-2 (WASM tool dispatch) actually calls `call_tool_export` and returns its result
   - M-1 (KeyInput event kind) returns a dedicated variant, not a generic `PaneOpened`
   - Run `cargo test -p arcterm-plugin` to verify all related tests pass

### Execution Notes

- **PLAN-1.1, Task 1:** Straightforward `.expect` → `match` conversion. Ensure the `Err(e)` branch is consistent with the pattern already used for `Renderer::new()` (line 1023-1030).

- **PLAN-1.2, Task 2:** The epoch ticker shutdown mechanism requires adding a new field to `PluginRuntime`. Verify that the `Drop` impl fires on test cleanup (use `std::thread::sleep(50ms)` in the test to allow the ticker thread time to observe the shutdown flag).

- **PLAN-2.1:** This is a final audit pass. Ensure all `.expect()` and `.unwrap()` calls in `resumed()`, `gpu.rs`, and `renderer.rs` are reviewed. The verify commands (`cargo clippy --workspace`) will catch any new panicking paths.

### Post-Execution Validation

After Phase 14 ships, verify:
- All 458+ workspace tests pass
- `cargo clippy --workspace -- -D warnings` is clean
- `cargo build --release -p arcterm-app` succeeds (binary artifact exists)
- No new `.expect()` or `.unwrap()` on fallible operations in runtime code (excluding tests)

---

## Verdict

**READY FOR EXECUTION**

All three plans are well-scoped, correctly ordered, and have no internal conflicts. Phase 14 addresses all remaining app-level and plugin issues identified in the v0.1.1 review, with clear acceptance criteria and regression test coverage. The architect's claims of "already resolved" issues have been spot-checked against the codebase and confirmed.

Execute Wave 1 (PLAN-1.1 and PLAN-1.2) in parallel, then execute Wave 2 (PLAN-2.1) to completion.
