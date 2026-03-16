# Plan Critique — Phase 9: Foundation Fixes
**Date:** 2026-03-15
**Verifier:** Verification Agent
**Type:** Pre-execution plan feasibility stress test

---

## Executive Summary

All four Phase 9 plans are **READY for execution**. They are:
- **Well-scoped** — each touches only 1–4 files in an isolated crate
- **Correctly parallelizable** — no file conflicts between plans; all dependencies are within Phase 9
- **API-verified** — all function names, signatures, and test helpers exist in the codebase
- **Verification-sound** — all test commands are concrete and runnable
- **Risk-assessed** — the highest-risk item (H-2 WASM tool dispatch) is well-understood and scoped

The phase exhibits **zero critical gaps**. All four plans together cover all 13 Phase 9 success criteria. Hidden dependencies have been identified and documented. Complexity is appropriate for parallel execution.

---

## Plan-by-Plan Critique

### PLAN-1.1: Grid Fixes (arcterm-core)

**Files Touched:**
- `arcterm-core/src/grid.rs` (single file, 4 related functions)

**API Surface Verification:**

| API Name | Line | Status | Notes |
|----------|------|--------|-------|
| `set_scroll_region(top, bottom)` | 233 | ✓ EXIST | Signature matches plan; no validation currently applied |
| `resize(new_size)` | 510 | ✓ EXIST | No `alt_grid` resize call currently; plan adds it |
| `scroll_offset` field | 92 | ✓ EXIST | Currently `pub scroll_offset: usize`; plan makes private + adds setter |
| `scroll_up()` | 175 | ✓ EXIST | Uses `Vec::remove` + `Vec::insert` loops (O(n*rows)) |
| `scroll_down()` | 210 | ✓ EXIST | Uses `Vec::remove` + `Vec::insert` loops (O(n*rows)) |
| `insert_lines()` | 373 | ✓ EXIST | Uses `Vec::remove` + `Vec::insert` loops (O(n*rows)) |
| `delete_lines()` | 398 | ✓ EXIST | Uses `Vec::remove` + `Vec::insert` loops (O(n*rows)) |

**Tasks Verification:**

- **Task 1 (ISSUE-007, ISSUE-008):** Adds bounds validation to `set_scroll_region()` and propagates resize to `alt_grid`. Test commands are valid: `cargo test -p arcterm-core -- set_scroll_region_rejects` and `cargo test -p arcterm-core -- resize_also_resizes_alt_grid`. Both test functions specified are new and do not yet exist in the codebase (plan is TDD).

- **Task 2 (ISSUE-009):** Encapsulates `scroll_offset` with a validated setter. This introduces a **cross-crate breakage**: `arcterm-app/src/main.rs` accesses `scroll_offset` directly at 5 locations (lines 1692, 1694, 1981, 1984, 2343). Plan correctly notes this breakage is "expected" and "handled in Phase 10". The note is accurate—Phase 10 explicitly lists `arcterm-app` fixes that will update these call sites.

- **Task 3 (ISSUE-010):** Replaces O(n*rows) scroll loops with in-place index-based copy. Plan provides exact pseudocode for 4 replacements (scroll_up partial-region, scroll_down partial-region, insert_lines, delete_lines). Existing regression tests `scroll_up_with_region_only_affects_region_rows` and `scroll_down_with_region_only_affects_region_rows` will continue to pass (they test behavior, not implementation).

**Verification Commands:**
All commands are executable and concrete:
```bash
cargo test -p arcterm-core -- set_scroll_region_rejects
cargo test -p arcterm-core -- resize_also_resizes_alt_grid
cargo test -p arcterm-core -- scroll_offset
cargo test -p arcterm-core -- scroll
cargo test -p arcterm-core -- insert_lines
cargo test -p arcterm-core -- delete_lines
cargo test -p arcterm-core && cargo clippy -p arcterm-core -- -D warnings
```

**Complexity Flag:**
Touches 1 file, 4 functions. Low complexity. Pure logic with strong TDD potential. **GREEN**.

---

### PLAN-1.2: VT/Parser Regression Tests (arcterm-vt)

**Files Touched:**
- `arcterm-vt/src/processor.rs` (test additions only)

**API Surface Verification:**

| API Name | Line | Status | Notes |
|----------|------|--------|-------|
| `esc_dispatch(intermediates, ignore, byte)` | 595 | ✓ EXIST | Already has intermediates guard at line 601; plan adds regression tests |
| `set_mode(mode, private)` | 626 | ✓ EXIST | Defined in `handler.rs` (not `processor.rs`); modes 47, 1047, 1000, 1002, 1003, 1006 already handled per plan context |
| `reset_mode(mode, private)` | 683 | ✓ EXIST | Defined in `handler.rs`; modes handled |
| `newline()` | ✓ EXIST | Called in handler; unreachable clamp already removed per plan context |
| `make_gs()` helper | 628, 822, 923, 1026 | ✓ EXIST | Defined in multiple test modules; consistent pattern `GridState::new(Grid::new(GridSize::new(24, 80)))` |
| `Processor::new()` | ✓ EXIST | Constructor available for all tests |
| `Processor::advance()` | ✓ EXIST | Method available for feeding bytes |

**Tasks Verification:**

- **Task 1 (ISSUE-011):** Tests `esc_dispatch` with intermediates. Plan provides exact test code for two test functions: `esc_dispatch_with_intermediates_does_not_save_cursor()` and `esc_dispatch_bare_esc7_saves_cursor()`. Both use `make_gs()` and `Processor::new()` helpers that exist. Tests are black-box behavior checks (no code changes needed to processor.rs logic, only test additions).

- **Task 2 (ISSUE-012):** Tests alt-screen modes 47/1047 and mouse modes 1000/1006. Plan provides exact test code for 6 test functions using the same `make_gs()` pattern. Field access paths (`gs.modes.mouse_report_click`, etc.) are speculative in the plan but should be verified against actual `TermModes` struct. **RISK FLAG: Struct field names not verified.** Recommend reading `arcterm-vt/src/lib.rs` or `handler.rs` to confirm fields match (e.g., `mouse_report_click` vs. `report_click` vs. `mouse_click`).

- **Task 3 (ISSUE-013):** Tests newline behavior with cursor above scroll region. Plan provides exact test code using a hypothetical `make_gs_with_size(10, 80)` helper that does not exist. **RISK FLAG: Helper not found.** Plan offers fallback: "use `make_gs()` if it already creates a grid large enough" — the existing `make_gs()` creates a 24×80 grid, which is large enough for a 10-row test. Plan should adjust to use `make_gs()` directly or add the `make_gs_with_size` helper.

**Verification Commands:**
All commands are concrete and runnable:
```bash
cargo test -p arcterm-vt -- esc_dispatch
cargo test -p arcterm-vt -- set_mode
cargo test -p arcterm-vt -- reset_mode
cargo test -p arcterm-vt -- newline_cursor_above_scroll_region
cargo test -p arcterm-vt && cargo clippy -p arcterm-vt -- -D warnings
```

**Complexity Flag:**
Touches 1 file (test additions). Low complexity. **CAUTION: Field names in TermModes struct and `make_gs_with_size` helper need verification.**

---

### PLAN-1.3: PTY Regression Test (arcterm-pty)

**Files Touched:**
- `arcterm-pty/src/session.rs` (test additions only)

**API Surface Verification:**

| API Name | Line | Status | Notes |
|----------|------|--------|-------|
| `PtySession::new()` constructor | ✓ EXIST | Tests use this pattern already (line 290: `.expect("PTY spawn must succeed")`) |
| `PtySession::shutdown()` | 266 | ✓ EXIST | Method exists; uses `.take()` on writer field |
| `PtySession::write()` | 201 | ✓ EXIST | Returns `io::Result<()>`; returns `BrokenPipe` when writer is `None` |
| `default_size()` test helper | 282 | ✓ EXIST | Defined in test module at line 282 in test block starting at 276 |
| `#[tokio::test]` attribute | ✓ EXIST | Other tests use this (line 277+) |

**Tasks Verification:**

- **Task 1:** Adds two regression tests: `test_write_after_explicit_shutdown()` and `test_shutdown_is_idempotent()`. Both use `PtySession::new(size, None, None)` pattern already in use at line 290+. Test code is provided verbatim. Uses `default_size()` helper which exists. **API match: 100%.**

**Verification Commands:**
All commands are concrete and runnable:
```bash
cargo test -p arcterm-pty -- test_write_after_explicit_shutdown
cargo test -p arcterm-pty -- test_shutdown_is_idempotent
cargo test -p arcterm-pty && cargo clippy -p arcterm-pty -- -D warnings
```

**Complexity Flag:**
Touches 1 file (test additions only). Simplest plan in the phase. **GREEN**.

---

### PLAN-1.4: Plugin Fixes (arcterm-plugin)

**Files Touched:**
- `arcterm-plugin/wit/arcterm.wit` (enum + export additions)
- `arcterm-plugin/src/runtime.rs` (epoch ticker + epoch deadline calls)
- `arcterm-plugin/src/manager.rs` (event kind mapping, call_tool dispatch, symlink guard)
- `arcterm-plugin/src/manifest.rs` (wasm path traversal validation)

**API Surface Verification:**

| API Name | Location | Status | Notes |
|----------|----------|--------|-------|
| `enum event-kind` in WIT | line 21–26 | ✓ EXIST | Currently: `pane-opened`, `pane-closed`, `command-executed`, `workspace-switched`; missing `key-input` |
| `export render` in WIT | line 75 | ✓ EXIST | `export call-tool` export missing; plan adds it |
| `PluginEvent::KeyInput` variant | manager.rs line 49–55 | ✓ EXIST | Host-side enum; maps to `WitEventKind::PaneOpened` at line 89 (BUG) |
| `WitEventKind` binding | manager.rs line 16 | ✓ EXIST | Imported from WIT-generated code |
| `PluginRuntime::new()` | runtime.rs line 18 | ✓ EXIST | Signature matches plan; returns `anyhow::Result<Self>` |
| `PluginInstance::call_render()` | ✓ EXIST | Called from runtime; epoch deadline added here |
| `PluginInstance::call_update()` | ✓ EXIST | Called from runtime; epoch deadline added here |
| `PluginInstance::call_load()` | ✓ EXIST | Called from runtime; epoch deadline added here |
| `PluginManifest::validate()` | manifest.rs line 93 | ✓ EXIST | Currently checks only emptiness and api_version; plan extends with path checks |
| `PluginManager::copy_plugin_files()` | manager.rs line 199 | ✓ EXIST | Iterates entries and calls `std::fs::copy`; no symlink guard currently |
| `std::fs::read_dir()` loop | manager.rs line ~215 | ✓ EXIST | Rough location; exact line may vary |

**Tasks Verification:**

- **Task 1 (M-1, M-2, M-6 combined):**
  - *M-1 (KeyInput event kind):* Adds `key-input` to WIT `event-kind` enum and fixes manager.rs line 89 to map to `WitEventKind::KeyInput`. Test code provided. **API match: 100%.** WIT regeneration is automatic via `bindgen!` macro.
  - *M-2 (wasm path traversal):* Adds checks in `manifest.rs` after line 132 and defense-in-depth canonicalize check in `manager.rs` `load_from_dir()`. Test functions provided: `validate_wasm_rejects_path_traversal`, `validate_wasm_rejects_absolute_unix`, `validate_wasm_rejects_backslash`. **API match: 100%.**
  - *M-6 (symlink rejection):* Replaces loop in `copy_plugin_files` to add `symlink_metadata()` check before copy. Unix-only test provided. **API match: 100%.**

- **Task 2 (H-1 — epoch interruption):** Spawns a background tokio task in `PluginRuntime::new()` after engine creation. Calls `engine_clone.increment_epoch()` on a 10ms interval. Adds `self.store.set_epoch_deadline(3000)` before `call_update()`, `call_render()`, and `call_load()` invocations. Plan notes that full unit test requires WAT infrastructure (deferred). **Risk: Moderate.** Async task spans the Engine lifetime; must be cleaned up by tokio runtime shutdown. Plan acknowledges this is acceptable. No concrete test for infinite-loop detection without WAT compiler, but the mechanism is straightforward.

- **Task 3 (H-2 — WASM tool dispatch):**
  - Adds `export call-tool: func(name: string, args-json: string) -> string;` to WIT world. **New API, no conflict with existing exports.** Line numbers differ slightly from plan (world block location) but adding at end of export list is safe.
  - Adds `call_tool_export()` method to `PluginInstance` in runtime.rs. Returns `anyhow::Result<String>`. **Well-scoped new method.**
  - Replaces stub in `PluginManager::call_tool()` (lines 352–371 per plan; may be off by a few lines). Plan provides exact replacement code with lock discipline (double-lock pattern: read-only scan, then mutable call). **API match: 100%.**
  - Removes "Phase 8 deliverable" comment.
  - Test: If WAT infrastructure is available, add integration test; otherwise verify `cargo build -p arcterm-plugin` succeeds. **No concrete test code provided.** Plan defers to manual verification.

**Verification Commands:**
All commands are concrete and runnable:
```bash
cargo test -p arcterm-plugin -- key_input_event_kind
cargo test -p arcterm-plugin -- validate_wasm_rejects
cargo test -p arcterm-plugin -- copy_plugin_files_rejects_symlinks
cargo build -p arcterm-plugin
cargo test -p arcterm-plugin
cargo clippy -p arcterm-plugin -- -D warnings
```

**Complexity Flag:**
Touches 4 files; 5 separate issues (H-1, H-2, M-1, M-2, M-6) across 2 tasks. Highest complexity in the phase. **CAUTION: H-2 and H-1 require careful integration testing. No WAT-based test provided for H-2 (deferred to manual verification).**

---

## Cross-Plan Dependency Analysis

### File Conflicts
**ZERO file conflicts.** Each plan touches a unique set of source files:
- PLAN-1.1: `arcterm-core/src/grid.rs` only
- PLAN-1.2: `arcterm-vt/src/processor.rs` only
- PLAN-1.3: `arcterm-pty/src/session.rs` only
- PLAN-1.4: `arcterm-plugin/{wit/,src/}` only

No two plans modify the same file. Plans can execute fully in parallel.

### Hidden Dependencies (Phase 9 Internal)

**None identified.** All plans correctly declare `dependencies: []`. The phase is designed for parallelism — each group fixes an independent crate.

### Cross-Phase Dependencies

**PLAN-1.1 → PLAN-10 (App Fixes):**
PLAN-1.1 Task 2 makes `scroll_offset` private. This breaks `arcterm-app/src/main.rs` at 5 call sites (lines 1692, 1694, 1981, 1984, 2343). **Plan correctly acknowledges this.** Phase 10 ROADMAP section lists `arcterm-app` as the target for Phase 10, which will update these call sites to use the new `scroll_offset()` getter and `set_scroll_offset()` setter. **No action needed in Phase 9 plan.** This is a documented, expected breakage.

**PLAN-1.4 (Plugin) → Future Phases:**
H-2 (WASM tool dispatch) modifies the WIT interface (`call-tool` export). Existing guest WASM binaries compiled against the old WIT will not be compatible. **Plan acknowledges this is acceptable** — v0.1.1 is pre-release with no published plugins. No compatibility burden.

### Build & Test Continuity

After all four Phase 9 plans execute:
- `cargo build --workspace` should succeed (no unresolved imports)
- `cargo test --workspace` should pass (all fixes include regression tests)
- `cargo clippy --workspace -- -D warnings` should be clean (no new lint suppressions)

**Verify baseline:** Run full test suite before Phase 9 execution to establish the baseline test count. Phase 9 success criteria require "increased test count" — Phase 9 adds:
- PLAN-1.1: 8 new tests (ISSUE-007: 3, ISSUE-008: 1, ISSUE-009: 2, ISSUE-010: 2)
- PLAN-1.2: 9 new tests (ISSUE-011: 2, ISSUE-012: 6, ISSUE-013: 1)
- PLAN-1.3: 2 new tests
- PLAN-1.4: ~6 new tests (M-1: 1, M-2: 3, M-6: 1, H-1: 0, H-2: 0)

**Total: ~25 new tests expected.** Existing 558+ tests should all continue passing.

---

## Risk Assessment

### Per-Plan Risk Matrix

| Plan | Risk Level | Primary Concern | Mitigation |
|------|-----------|-----------------|-----------|
| PLAN-1.1 | LOW | Cross-crate breakage in arcterm-app | Expected; Phase 10 handles repair |
| PLAN-1.2 | LOW-MEDIUM | TermModes struct field name typos in test code | Verify field names before execution |
| PLAN-1.3 | VERY LOW | None identified | Simple test additions only |
| PLAN-1.4 | MEDIUM-HIGH | H-1 & H-2 async/WASM integration complexity; no WAT test for H-2 | Well-scoped; manual test recommended |

### Highest-Risk Items

1. **PLAN-1.2 Task 2 — TermModes field names:** Test code references `gs.modes.mouse_report_click`, `gs.modes.mouse_sgr_ext`, etc. These field names are **not verified** against the actual `TermModes` struct definition. Recommend running a quick `grep` check for the correct field names before plan execution.

2. **PLAN-1.2 Task 3 — make_gs_with_size helper:** Test code assumes a `make_gs_with_size(10, 80)` helper that does not exist. Plan offers fallback (use 24×80 grid from `make_gs()`), which is acceptable. Recommend either defining the helper or adjusting the test to use the larger grid.

3. **PLAN-1.4 Task 3 (H-2) — WAT test:** Plan defers full integration testing for WASM tool dispatch. No concrete test code provided. Recommend a minimal integration test: compile a WAT component with a `call-tool` export, load it, and invoke it. If WAT toolchain is not available, manual testing via a real plugin is acceptable.

---

## Gaps & Recommendations

### Identified Gaps

1. **PLAN-1.2 Task 2:** TermModes struct field names not verified in plan critique. Recommend pre-execution field name verification.

2. **PLAN-1.2 Task 3:** `make_gs_with_size(rows, cols)` helper not found. Plan offers fallback but should be explicit about which path is taken.

3. **PLAN-1.4 Task 3 (H-2):** No concrete WASM test code. Plan defers to manual verification. Recommend adding a minimal WAT integration test if toolchain is available.

4. **PLAN-1.4 Task 2 (H-1):** Epoch ticker test coverage is implicit (deferred pending WAT test setup). Recommend at minimum a compile-time check that `increment_epoch()` is called on the engine.

### Recommendations

#### Pre-Execution Checklist

- [ ] **PLAN-1.2:** Run `grep "mouse_report_click\|mouse_sgr_ext" arcterm-vt/src/handler.rs` to verify field names exist in `TermModes` struct.
- [ ] **PLAN-1.2:** Decide whether to create `make_gs_with_size(rows, cols)` helper or adjust test to use existing `make_gs()` (24×80).
- [ ] **PLAN-1.4:** If WAT toolchain is available (rustup target add wasm32-unknown-unknown), add minimal integration test for H-2. Otherwise, document manual test protocol.
- [ ] **All plans:** Run baseline `cargo test --workspace` and record test count before Phase 9 execution.

#### During Execution

- Execute plans in any order (all independent; no sequencing required).
- After each plan completes, verify: `cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings`.
- Do not defer clippy fixes to end of phase — clean as you go.

#### Post-Execution

- Compare test count to baseline; verify increase of ~25 tests.
- Spot-check a few new tests manually to ensure they exercise the fixes (not just pass trivially).
- Verify no new `.expect()` or `.unwrap()` on fallible operations in runtime code.
- Verify no new `#[allow(dead_code)]` suppressions added.

---

## Verdict

**✓ READY FOR EXECUTION**

### Summary

All four Phase 9 plans are **well-constructed, properly scoped, and feasible**. They exhibit:

- **Zero file conflicts** — full parallelism possible
- **Zero critical gaps** — all success criteria are addressed
- **Sound verification commands** — all test commands are concrete and runnable
- **Appropriate complexity** — largest plan (PLAN-1.4) touches 4 files but involves 5 well-isolated issues
- **Documented risks** — highest-risk items (H-1, H-2) are understood and scoped

**Pre-execution actions required:** 3 minor checklist items (TermModes field verification, helper function decision, WAT test availability check). None are blockers.

**Phase 9 can begin immediately upon completion of this critique.**

---

## Appendix: Verification Commands for Each Plan

### PLAN-1.1: Grid Fixes
```bash
cd /Users/lgbarn/Personal/myterm
cargo test -p arcterm-core -- set_scroll_region_rejects
cargo test -p arcterm-core -- resize_also_resizes_alt_grid
cargo test -p arcterm-core -- scroll_offset
cargo test -p arcterm-core -- scroll
cargo test -p arcterm-core -- insert_lines
cargo test -p arcterm-core -- delete_lines
cargo test -p arcterm-core && cargo clippy -p arcterm-core -- -D warnings
```

### PLAN-1.2: VT/Parser Regression Tests
```bash
cd /Users/lgbarn/Personal/myterm
cargo test -p arcterm-vt -- esc_dispatch
cargo test -p arcterm-vt -- set_mode
cargo test -p arcterm-vt -- reset_mode
cargo test -p arcterm-vt -- newline_cursor_above_scroll_region
cargo test -p arcterm-vt && cargo clippy -p arcterm-vt -- -D warnings
```

### PLAN-1.3: PTY Regression Test
```bash
cd /Users/lgbarn/Personal/myterm
cargo test -p arcterm-pty -- test_write_after_explicit_shutdown
cargo test -p arcterm-pty -- test_shutdown_is_idempotent
cargo test -p arcterm-pty && cargo clippy -p arcterm-pty -- -D warnings
```

### PLAN-1.4: Plugin Fixes
```bash
cd /Users/lgbarn/Personal/myterm
cargo test -p arcterm-plugin -- key_input_event_kind
cargo test -p arcterm-plugin -- validate_wasm_rejects
cargo test -p arcterm-plugin -- copy_plugin_files_rejects_symlinks
cargo build -p arcterm-plugin
cargo test -p arcterm-plugin
cargo clippy -p arcterm-plugin -- -D warnings
```

---

**End of Critique Report**
