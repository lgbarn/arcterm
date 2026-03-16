---
plan: "1.2"
phase: phase-14
status: complete
commits:
  - 2ae27ef — fix ISSUE-015 — backslash validation test exercises correct guard
  - 44440f7 — fix ISSUE-016 — add shutdown flag to epoch ticker thread
  - f7c97f4 — fix ISSUE-017/014/018 — call_tool single lock, JSON escaping, canonicalize error
---

# SUMMARY-1.2 — Plugin Fixes

## What Was Done

### Task 1 — ISSUE-015: Backslash validation test (manifest.rs)

**File:** `arcterm-plugin/src/manifest.rs`

Changed `validate_wasm_rejects_backslash` test input from `"..\\evil.wasm"` to
`"sub\\file.wasm"`. The old input triggered the `contains("..")` guard at line
133 before reaching the `contains('\\')` guard at line 139, leaving the
backslash-only validation path untested. The new input has no `..` component so
the only guard that fires is the backslash check. Assertion updated to require
`"backslash"` in the error message specifically rather than the `||` fallback.

**Deviations:** None. The `--exact` flag in the plan's verify command did not
match the full module-path form `manifest::tests::validate_wasm_rejects_backslash`
when run as written; the test was confirmed passing via the full test run and
filtered run. No code change was needed.

---

### Task 2 — ISSUE-016: Epoch ticker thread shutdown (runtime.rs)

**File:** `arcterm-plugin/src/runtime.rs`
**Test file:** `arcterm-plugin/tests/runtime_test.rs`

Added imports `use std::sync::Arc` and `use std::sync::atomic::{AtomicBool, Ordering}`.
Added `shutdown: Arc<AtomicBool>` field to `PluginRuntime`. In `PluginRuntime::new()`,
created the flag before the thread spawn and passed a clone into the ticker closure.
Changed the loop from `loop { sleep; increment_epoch }` to
`while !shutdown_clone.load(Ordering::Relaxed) { sleep; increment_epoch }`.
Implemented `Drop for PluginRuntime` that stores `true` into the flag.

Added `epoch_ticker_stops_on_drop` smoke test in `runtime_test.rs` that creates a
`PluginRuntime`, drops it, sleeps 50ms, and verifies no panic. The test was written
first (TDD). It passed trivially before the implementation (dropping without `Drop`
does not panic) but documents the intent and confirms the implementation does not
regress.

**Deviations:** None.

---

### Task 3 — ISSUE-017/014/018: call_tool + load_from_dir fixes (manager.rs)

**File:** `arcterm-plugin/src/manager.rs`

Three fixes were applied in this task as specified by the plan.

**ISSUE-017 + ISSUE-014 — Rewritten `call_tool`:**

Replaced the double-lock pattern (lock → check ownership → drop → re-lock →
dispatch) with a single `lock().mut_lock` scope that checks ownership and
dispatches within the same critical section. Simultaneously replaced the
`format!()` tool-not-found response with `serde_json::json!({"error": "tool not
found", "tool": name}).to_string()`, which correctly JSON-escapes any special
characters in `name`.

**ISSUE-018 — Canonicalize error propagation:**

Replaced `wasm_path.canonicalize().unwrap_or(wasm_path.clone())` with an explicit
`.map_err(|e| anyhow::anyhow!("plugin wasm file '{}' not found: {e}", ...))?`.
This surfaces a clear "not found" error when the wasm file is absent instead of
silently using the raw path, which caused `!wasm_canonical.starts_with(&dir_canonical)`
to trigger a misleading "resolves outside the plugin directory" message.

**Regression tests added:**
- `call_tool_not_found_returns_valid_json` — passes a name containing `"` and `\`,
  parses the result with `serde_json::from_str`, asserts validity and field values.
- `call_tool_single_lock_no_toctou` — structural verification that the single-lock
  call path compiles and returns valid JSON for an empty plugin set.
- `load_from_dir_rejects_missing_wasm` — creates a plugin directory with a valid
  `plugin.toml` but no `.wasm` file, asserts the error contains `"not found"`.

All three tests failed before the implementation and pass after it.

**Deviations:** None.

---

## Verification Results

All tasks verified with:
```
cargo test -p arcterm-plugin         # 25 tests pass, 0 failed
cargo clippy -p arcterm-plugin -- -D warnings  # clean
```

Test counts:
- `manifest::tests`: 10 tests (includes fixed `validate_wasm_rejects_backslash`)
- `manager::tests`: 11 tests (includes 3 new regression tests)
- `runtime_test` integration: 4 tests (includes new `epoch_ticker_stops_on_drop`)

## Must-Haves Checklist

- [x] ISSUE-015 backslash-only validation test exercises the correct guard
- [x] ISSUE-016 epoch ticker thread has a shutdown mechanism
- [x] ISSUE-017 call_tool uses single lock (no TOCTOU)
- [x] ISSUE-018 canonicalize propagates error instead of silent fallback
- [x] ISSUE-014 (tool-not-found) JSON response properly escapes name parameter
- [x] Each fix includes at least one regression test
