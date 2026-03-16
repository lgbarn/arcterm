---
phase: phase-14
plan: "1.2"
wave: 1
dependencies: []
must_haves:
  - ISSUE-015 backslash-only validation test exercises the correct guard
  - ISSUE-016 epoch ticker thread has a shutdown mechanism
  - ISSUE-017 call_tool uses single lock (no TOCTOU)
  - ISSUE-018 canonicalize propagates error instead of silent fallback
  - ISSUE-014 (tool-not-found) JSON response properly escapes name parameter
  - Each fix includes at least one regression test
files_touched:
  - arcterm-plugin/src/manifest.rs
  - arcterm-plugin/src/runtime.rs
  - arcterm-plugin/src/manager.rs
tdd: true
---

# PLAN-1.2 — Plugin Fixes

## Context

The Phase 12 engine swap did not touch `arcterm-plugin`. The following issues from
the v0.1.1 review remain open. H-1 (epoch ticker exists), H-2 (WASM tool dispatch
wired), M-1 (KeyInput kind), M-2 (path traversal validation), and M-6 (symlink
rejection) were already resolved in the current codebase.

The surviving plugin issues are:

| ID | File | Summary |
|---|---|---|
| ISSUE-015 | manifest.rs:399-403 | Backslash test uses `"..\\evil.wasm"` which triggers `..` guard, not `\` guard |
| ISSUE-016 | runtime.rs:29-32 | Epoch ticker OS thread never terminated; leaks Engine Arc |
| ISSUE-017 | manager.rs:368-376 | Double-lock TOCTOU in `call_tool` |
| ISSUE-018 | manager.rs:250-252 | `canonicalize` fallback hides "file not found" errors |
| ISSUE-014 (tool JSON) | manager.rs:379-382 | `format!` does not JSON-escape `name` |

## Tasks

<task id="1" files="arcterm-plugin/src/manifest.rs" tdd="true">
  <action>Fix ISSUE-015: Change the `validate_wasm_rejects_backslash` test at line 399-403 to use input `"sub\\file.wasm"` (no `..` component) so the backslash check at line 139 is the one that triggers. Update the assertion to check that the error message contains `"backslash"` specifically (not `".."` as an alternative). The test should look like:

```rust
#[test]
fn validate_wasm_rejects_backslash() {
    let err = make_manifest_wasm("sub\\file.wasm")
        .validate()
        .expect_err("should reject backslash");
    assert!(
        err.contains("backslash"),
        "error should mention backslash, got: {err}"
    );
}
```

This ensures the backslash-only validation path at line 139 (`if self.wasm.contains('\\')`) is exercised independently of the `..` check at line 133.
  </action>
  <verify>cargo test -p arcterm-plugin -- validate_wasm_rejects_backslash --exact 2>&1 | tail -5</verify>
  <done>Test `validate_wasm_rejects_backslash` passes. The test input `"sub\\file.wasm"` does not contain `..`, so the only way it can pass is via the backslash guard at manifest.rs line 139.</done>
</task>

<task id="2" files="arcterm-plugin/src/runtime.rs" tdd="true">
  <action>Fix ISSUE-016: Add a shutdown mechanism to the epoch ticker thread.

1. Add `use std::sync::Arc; use std::sync::atomic::{AtomicBool, Ordering};` to runtime.rs imports (Arc is likely already imported via other paths).

2. Add a `shutdown: Arc<AtomicBool>` field to `PluginRuntime`.

3. In `PluginRuntime::new()`, create `let shutdown = Arc::new(AtomicBool::new(false));` before spawning the thread. Clone it into the thread closure. Change the thread loop from:
   ```rust
   std::thread::spawn(move || loop {
       std::thread::sleep(std::time::Duration::from_millis(10));
       engine_clone.increment_epoch();
   });
   ```
   to:
   ```rust
   let shutdown_clone = shutdown.clone();
   std::thread::spawn(move || {
       while !shutdown_clone.load(Ordering::Relaxed) {
           std::thread::sleep(std::time::Duration::from_millis(10));
           engine_clone.increment_epoch();
       }
   });
   ```

4. Store `shutdown` in the `PluginRuntime` struct.

5. Implement `Drop for PluginRuntime`:
   ```rust
   impl Drop for PluginRuntime {
       fn drop(&mut self) {
           self.shutdown.store(true, Ordering::Relaxed);
       }
   }
   ```

6. Add a test `epoch_ticker_stops_on_drop` that creates a `PluginRuntime`, drops it, sleeps 50ms, and verifies no panic (the thread should have exited cleanly). This is a basic smoke test — the real verification is that test processes no longer leak threads.
  </action>
  <verify>cargo test -p arcterm-plugin -- epoch_ticker_stops_on_drop --exact 2>&1 | tail -5</verify>
  <done>Test passes. `PluginRuntime` implements `Drop` that sets the shutdown flag. The epoch ticker thread checks the flag each iteration and exits when it is set. Thread count no longer grows unboundedly across test runs.</done>
</task>

<task id="3" files="arcterm-plugin/src/manager.rs" tdd="true">
  <action>Fix three issues in manager.rs:

**ISSUE-017 (TOCTOU double-lock):** Rewrite `call_tool` at lines 366-383 to use a single lock scope. Replace the current pattern (lock → check ownership → drop lock → re-lock → dispatch) with:
```rust
pub fn call_tool(&self, name: &str, args_json: &str) -> anyhow::Result<String> {
    for lp in self.plugins.values() {
        let mut inst = lp.instance.lock()
            .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let owned = inst.host_data().registered_tools.iter().any(|t| t.name == name);
        if owned {
            return inst.call_tool_export(name, args_json);
        }
    }
    Ok(serde_json::json!({"error": "tool not found", "tool": name}).to_string())
}
```

This also fixes **ISSUE-014 (tool-not-found JSON escaping)** by using `serde_json::json!` instead of `format!`, which properly escapes special characters in `name`.

**ISSUE-018 (canonicalize fallback):** In `load_from_dir` at lines 250-252, replace the `unwrap_or` fallback with an explicit error. Change:
```rust
let wasm_canonical = wasm_path.canonicalize().unwrap_or(wasm_path.clone());
```
to:
```rust
let wasm_canonical = wasm_path.canonicalize().map_err(|e| {
    anyhow::anyhow!(
        "plugin wasm file '{}' not found: {e}",
        wasm_path.display()
    )
})?;
```

Add three regression tests:
1. `call_tool_not_found_returns_valid_json` — call `call_tool` with a name containing `"` and `\`, parse the result with `serde_json::from_str`, assert it succeeds and the `"tool"` field matches the input.
2. `call_tool_single_lock_no_toctou` — (documented test name; actual verification is that the code compiles with a single lock scope — the TOCTOU was a code structure issue, not a runtime-observable bug in the single-threaded test context).
3. `load_from_dir_rejects_missing_wasm` — create a plugin directory with a valid `plugin.toml` but no `.wasm` file, call `load_from_dir`, assert the error message contains `"not found"`.
  </action>
  <verify>cargo test -p arcterm-plugin -- call_tool_not_found_returns_valid_json load_from_dir_rejects_missing_wasm 2>&1 | tail -10</verify>
  <done>Both new tests pass. `call_tool` uses a single lock scope (ISSUE-017). Tool-not-found response is valid JSON even with special characters in the tool name (ISSUE-014). `load_from_dir` returns a clear "not found" error when the wasm file does not exist (ISSUE-018).</done>
</task>
