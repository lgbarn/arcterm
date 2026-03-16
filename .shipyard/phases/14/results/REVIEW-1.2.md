---
plan: "1.2"
phase: phase-14
verdict: APPROVE
stage1: PASS
stage2_critical: 0
stage2_important: 1
stage2_suggestions: 1
---

# REVIEW-1.2 — Plugin Fixes

## Stage 1: Spec Compliance
**Verdict:** PASS

### Task 1: ISSUE-015 — Backslash validation test exercises the correct guard
- Status: PASS
- Evidence: `arcterm-plugin/src/manifest.rs:400-408` — test `validate_wasm_rejects_backslash` uses input `"sub\\file.wasm"`. Guard ordering at lines 133–141 confirms: `contains("..")` at line 133 does not match (no `..` in `"sub\\file.wasm"`); `starts_with('\\')` at line 136 does not match (input starts with `s`); `contains('\\')` at line 139 matches and returns `"must not contain backslashes"`. Test asserts `err.contains("backslash")`, which matches the error string at line 140.
- Notes: The only guard that fires is the backslash check at line 139, as required. The `..` guard at line 133 is not reached for this input, making the test genuinely isolating. Fully compliant with the spec.

### Task 2: ISSUE-016 — Epoch ticker thread has shutdown mechanism
- Status: PASS
- Evidence:
  - `arcterm-plugin/src/runtime.rs:1-3` — imports `Arc`, `AtomicBool`, `Ordering`.
  - `arcterm-plugin/src/runtime.rs:17` — `shutdown: Arc<AtomicBool>` field on `PluginRuntime`.
  - `arcterm-plugin/src/runtime.rs:32-40` — flag created before spawn; `shutdown_clone` passed into thread closure; loop changed from `loop { ... }` to `while !shutdown_clone.load(Ordering::Relaxed) { ... }`.
  - `arcterm-plugin/src/runtime.rs:50` — `shutdown` stored in the struct.
  - `arcterm-plugin/src/runtime.rs:113-119` — `impl Drop for PluginRuntime` sets the flag with `Ordering::Relaxed`.
  - `arcterm-plugin/tests/runtime_test.rs:89-99` — `epoch_ticker_stops_on_drop` smoke test creates a runtime, drops it, sleeps 50ms, and asserts no panic.
- Notes: Implementation matches the spec exactly including field name, ordering, and Drop pattern. The smoke test documents intent; the summary's acknowledgment that it "passed trivially before" (no-panic before Drop was a pre-existing invariant) is accurate and does not undermine its value as a regression guard.

### Task 3: ISSUE-017/014/018 — call_tool single lock, JSON escaping, canonicalize error
- Status: PASS
- Evidence (ISSUE-017 + ISSUE-014):
  - `arcterm-plugin/src/manager.rs:376-386` — `call_tool` acquires `lp.instance.lock()` once, checks ownership via `inst.host_data().registered_tools`, and dispatches within the same critical section. No second lock acquisition.
  - Line 385 — tool-not-found response uses `serde_json::json!({"error": "tool not found", "tool": name}).to_string()`.
  - `arcterm-plugin/src/manager.rs:765-790` — `call_tool_not_found_returns_valid_json` test uses a name containing `"` and `\`, parses with `serde_json::from_str`, asserts round-trip fidelity of the `"tool"` field.
  - `arcterm-plugin/src/manager.rs:798-811` — `call_tool_single_lock_no_toctou` structural test.
- Evidence (ISSUE-018):
  - `arcterm-plugin/src/manager.rs:250-255` — `wasm_path.canonicalize()` now uses `.map_err(|e| anyhow::anyhow!("plugin wasm file '{}' not found: {e}", ...))?` with explicit propagation.
  - `arcterm-plugin/src/manager.rs:821-846` — `load_from_dir_rejects_missing_wasm` test creates a dir with `plugin.toml` but no `.wasm`, asserts error contains `"not found"`.
- Notes: All three regression tests from the plan spec are present and correctly structured.

---

## Stage 2: Code Quality

### Important

- **Residual `unwrap_or` fallback on `dir_canonical` at `arcterm-plugin/src/manager.rs:256`**
  - `let dir_canonical = dir.canonicalize().unwrap_or(dir.to_path_buf());` silently falls back to the raw path when the plugin directory does not exist or is inaccessible. In practice this fires only after `wasm_canonical` is already resolved (so the directory must exist), but the contract is not obvious from reading the code alone. If the canonicalize of the dir fails for a permission reason (e.g., the directory exists but is unreadable), the subsequent `!wasm_canonical.starts_with(&dir_canonical)` check compares a canonical path against a raw path, producing a false positive "resolves outside the plugin directory" error that is harder to diagnose than "permission denied."
  - Remediation: Apply the same explicit error pattern used for `wasm_canonical`: `let dir_canonical = dir.canonicalize().map_err(|e| anyhow::anyhow!("cannot canonicalize plugin directory '{}': {e}", dir.display()))?;`. This was not required by PLAN-1.2 (the spec only targeted `wasm_canonical`), but given ISSUE-018's original motivation it is the natural completion of the fix. Log as a new issue for a follow-up plan.

### Suggestions

- **`epoch_ticker_stops_on_drop` does not verify the thread actually stopped, only that no panic occurred**
  - `arcterm-plugin/tests/runtime_test.rs:89-99` — The test drops `PluginRuntime`, sleeps 50ms, and passes. It cannot distinguish between "thread exited cleanly" and "thread is still running but hasn't panicked yet." The summary acknowledges this is a smoke test. For a stronger guarantee, the `PluginRuntime` could expose a `thread_handle: Option<JoinHandle<()>>` and the test could call `join()` after the 50ms sleep to verify completion. This is not required by the spec but would make the test falsifiable.
  - Remediation: Optionally store the `JoinHandle` from `std::thread::spawn` in `PluginRuntime`. In the test, access it via a `#[cfg(test)]` accessor and call `handle.join().expect("ticker thread should have exited")` after the sleep. This is a test-quality improvement, not a correctness fix.

---

## Summary
**Verdict:** APPROVE

All five issues from PLAN-1.2 (ISSUE-015, ISSUE-016, ISSUE-017, ISSUE-014, ISSUE-018) are correctly implemented with evidence in the source files. Each fix is accompanied by at least one regression test as required. The backslash guard ordering is correctly isolated, the epoch ticker Drop impl matches the spec, `call_tool` uses a single lock scope with `serde_json::json!` for the not-found response, and `canonicalize` errors propagate with explicit context. One non-blocking finding (the residual `unwrap_or` on `dir_canonical`) is logged as a follow-up item.

Critical: 0 | Important: 1 | Suggestions: 1
