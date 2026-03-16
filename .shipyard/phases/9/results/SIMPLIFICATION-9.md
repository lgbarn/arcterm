# Simplification Report

**Phase:** 9 — Foundation Fixes (v0.1.1 stabilization)
**Date:** 2026-03-16T03:20:22Z
**Files analyzed:** 7
**Findings:** 2 medium, 3 low

---

## High Priority

None. No high-priority findings in this phase.

---

## Medium Priority

### Duplicate `make_gs()` / `feed()` helpers across five test modules

- **Type:** Consolidate
- **Locations:**
  - `arcterm-vt/src/processor.rs:628` — `phase4_task2_tests::make_gs()` + `feed()`
  - `arcterm-vt/src/processor.rs:822` — `osc7770_tools_tests::make_gs()` + `feed()`
  - `arcterm-vt/src/processor.rs:923` — `phase9_regression_tests::make_gs()` (no `feed()`)
  - `arcterm-vt/src/processor.rs:1085` — `osc133_tests::make_gs()` + `feed()`
  - `arcterm-vt/src/processor.rs:1188` — `osc7770_context_tests::make_gs()` + `feed()`
- **Description:** `make_gs()` is defined identically in five separate test modules — each returning `GridState::new(Grid::new(GridSize::new(24, 80)))` with the exact same body. The `feed()` helper (`Processor::new()` + `proc.advance()`) is duplicated in four of those five modules. The new `phase9_regression_tests` module added by PLAN-1.2 also adds `make_gs_with_size(rows, cols)` which is a generalization of `make_gs()`. This is a textbook Rule of Three violation: five identical copies (make_gs) and four identical copies (feed).
- **Suggestion:** Create a `mod test_helpers` (or a `#[cfg(test)]` free block above the test modules) inside `processor.rs` that declares `pub(super) fn make_gs()`, `pub(super) fn make_gs_with_size()`, and `pub(super) fn feed()`. All five test modules import from it. This removes ~30 lines and creates one place to change if `GridState::new` or the default grid size changes.
- **Impact:** ~30 lines removed, 5 → 1 definition of make_gs, 4 → 1 definition of feed, single canonical default grid size (24×80) for tests.

---

### Repeated epoch-deadline setup — magic constant scattered across five call sites

- **Type:** Refactor
- **Locations:**
  - `arcterm-plugin/src/runtime.rs:63-64` — `load_plugin`
  - `arcterm-plugin/src/runtime.rs:92-93` — `load_plugin_with_wasi`
  - `arcterm-plugin/src/runtime.rs:114-115` — `call_update`
  - `arcterm-plugin/src/runtime.rs:124-125` — `call_render`
  - `arcterm-plugin/src/runtime.rs:133-134` — `call_tool_export`
- **Description:** The call `store.set_epoch_deadline(3000)` appears five times, each preceded by an identical comment (`// 3000 epochs at 10ms tick interval = 30-second deadline.`). The constant `3000` and its relationship to the 10ms tick interval (defined once in the thread spawn at line 30) are implicit at every call site. If the tick interval changes, or the desired timeout changes, five spots must be updated in lock-step.
- **Suggestion:** Declare a single named constant at module scope:
  ```rust
  /// Epoch deadline: 3000 ticks × 10ms/tick = 30-second maximum plugin runtime per call.
  const EPOCH_DEADLINE: u64 = 3000;
  ```
  Replace all five `store.set_epoch_deadline(3000)` calls with `store.set_epoch_deadline(EPOCH_DEADLINE)` and remove the five repetitive inline comments. Optionally pair it with a `EPOCH_TICK_MS: u64 = 10` constant used at the thread spawn site.
- **Impact:** 5 comment+call pairs → 1 constant declaration + 5 bare constant references. Semantic intent documented once, change propagates automatically.

---

## Low Priority

- **Double lock acquisition in `call_tool`** — `arcterm-plugin/src/manager.rs:368-376`: The lock on `lp.instance` is acquired twice: once (read-only) to check `registered_tools`, dropped, then re-acquired mutably to call `call_tool_export`. In a single-threaded context this is harmless, but it leaves a TOCTOU window where another thread could load/unload a plugin between the two acquisitions. The pattern is slightly wasteful in the common case where a plugin does own the tool. A minor refactor: hold the lock for both the ownership check and the dispatch in one acquisition (only releasing if not owned). This is a nit, not urgent, as `Mutex` poisoning would surface the error.

- **`make_manifest` and `make_manifest_wasm` are near-duplicates in tests** — `arcterm-plugin/src/manifest.rs:335` and `376`: Both helpers construct a `PluginManifest` with identical boilerplate; they differ only in which field is parameterized (`name` vs `wasm`). These two don't yet cross the Rule of Three threshold for extraction, but if a third variant emerges (e.g. parameterizing `api_version`), a single `make_manifest_full(name, wasm)` helper should replace both.

- **Redundant comment on `set_scroll_region` guard** — `arcterm-core/src/grid.rs:248`: The doc comment says "Silently rejects invalid bounds: top >= rows, bottom >= rows, or top >= bottom." The guard condition on line 250 reads `if top >= self.size.rows || bottom >= self.size.rows || top >= bottom`. The comment is accurate and useful; the only nit is that the word "silently" slightly downplays the behavior — the existing four tests (`set_scroll_region_rejects_*`) document the rejection clearly. No code change needed; mention only for awareness.

---

## Summary

- **Duplication found:** 2 instances across 5 locations (make_gs/feed across 5 test modules; epoch deadline comment+constant across 5 call sites)
- **Dead code found:** 0 unused definitions
- **Complexity hotspots:** 0 functions exceeding thresholds (all new functions are short and well-scoped)
- **AI bloat patterns:** 1 instance (repeated magic constant + comment at every call site rather than a named constant)
- **Estimated cleanup impact:** ~30 lines removable from test helpers consolidation; constant extraction is a rename-only change with no line impact

---

## Recommendation

Simplification is **deferrable** — no findings block shipping. The two medium findings are cleanup items that should be addressed in a near-term maintenance pass rather than held against this phase. Phase 9 is a stabilization/bugfix phase and the code changes are well-scoped: every fix is localized, every new function is short, and no over-engineering was introduced. The grid scroll refactor (ISSUE-010) is particularly clean — the four O(n·rows) operations were replaced with consistent in-place copy patterns. The test module helper duplication is a pre-existing structural pattern in `processor.rs` that predates Phase 9; PLAN-1.2 followed it for consistency. Both medium findings are low-risk mechanical improvements best bundled with a future `arcterm-vt` test infrastructure pass.
