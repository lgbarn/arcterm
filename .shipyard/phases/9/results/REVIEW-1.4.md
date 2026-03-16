# REVIEW-1.4 — Plugin Fixes (arcterm-plugin)

**Reviewer:** shipyard:reviewer
**Plan:** `.shipyard/phases/9/plans/PLAN-1.4.md`
**Summary:** `.shipyard/phases/9/results/SUMMARY-1.4.md`
**Commits reviewed:** `7ff766c`, `c35f559`, `356e203`

---

## Stage 1: Spec Compliance

**Verdict:** PASS

### Task 1: M-1 + M-2 + M-6 — Security and Correctness Fixes

**M-1 — KeyInput kind:**
- Status: PASS
- Evidence: `arcterm-plugin/wit/arcterm.wit:26` — `key-input,` present in `event-kind` enum. `arcterm-plugin/src/manager.rs:87` — `PluginEvent::KeyInput { .. } => WitEventKind::KeyInput`. Test `key_input_event_kind_is_key_input` at `manager.rs:582` asserts `matches!(ev.kind(), WitEventKind::KeyInput)`.
- Notes: Bug was a silent mis-mapping; the fix is minimal and correct.

**M-2 — WASM path traversal:**
- Status: PASS
- Evidence: `manifest.rs:133-141` — three consecutive guards: `contains("..")`, `starts_with('/')` or `starts_with('\\')`, `contains('\\')`. `manager.rs:250-257` — canonicalize-based defense-in-depth rejects wasm paths that resolve outside the plugin directory. Tests `validate_wasm_rejects_path_traversal`, `validate_wasm_rejects_absolute_unix`, `validate_wasm_rejects_backslash` at `manifest.rs:388-403`.
- Notes: One test coverage gap flagged in Stage 2 (ISSUE-015).

**M-6 — Symlink rejection:**
- Status: PASS
- Evidence: `manager.rs:215-228` — `entry.path().symlink_metadata()?` called before any copy; symlinks bail with error message; non-file entries (directories, sockets) are skipped. Unix-only test `copy_plugin_files_rejects_symlinks` at `manager.rs:595-612` creates a real symlink and asserts the error message contains "symlink".
- Notes: Fix correctly uses `symlink_metadata()` (not `metadata()`) so symlinks cannot be followed before detection.

### Task 2: H-1 — Epoch Interruption Ticker

- Status: PASS
- Evidence: `runtime.rs:28-32` — OS thread spawned after engine creation; 10ms `thread::sleep` loop calling `engine_clone.increment_epoch()`. `store.set_epoch_deadline(3000)` added before `call_load` in both `load_plugin` (`runtime.rs:64`) and `load_plugin_with_wasi` (`runtime.rs:93`). `self.store.set_epoch_deadline(3000)` added before `call_update` (`runtime.rs:115`) and `call_render` (`runtime.rs:125`). Doc comment "3000 epochs at 10ms tick interval = 30-second deadline" present at each call site.
- Notes: **Documented deviation** — OS thread used instead of `tokio::spawn`. Rationale is sound: sync tests call `PluginRuntime::new()` without a tokio runtime, causing `tokio::spawn` to panic. OS thread is functionally equivalent for a CPU-interval ticker. The thread-leak concern is flagged in Stage 2 (ISSUE-016).

### Task 3: H-2 — Full WASM Tool Dispatch

- Status: PASS
- Evidence: `arcterm.wit:77` — `export call-tool: func(name: string, args-json: string) -> string;` present in world block. `runtime.rs:132-137` — `call_tool_export` sets `epoch_deadline(3000)` then calls `self.instance.call_call_tool`. `manager.rs:366-383` — stub replaced with real dispatch: double-lock pattern checks ownership read-only, then acquires mutable lock to call `call_tool_export`. "Phase 8 deliverable" comment removed (confirmed in diff).
- Notes: WAT integration test gap was documented in the summary and is acceptable per the plan's own note about setup complexity. The tool-not-found JSON format issue is flagged in Stage 2 (ISSUE-014).

---

## Stage 2: Code Quality

### Critical

*None.*

### Important

**ISSUE-014 — Tool-not-found JSON does not escape `name` parameter**
- Location: `arcterm-plugin/src/manager.rs:379-382`
- Code: `Ok(format!("{{\"error\":\"tool not found\",\"tool\":\"{}\"}}", name))`
- If `name` contains `"` or `\`, the output is malformed JSON. Example: `call_tool("foo\"}", "")` produces `{"error":"tool not found","tool":"foo"}"}`. Any caller that parses this response (including AI model JSON parsers) will reject it. The return value goes through the MCP layer, so the surface is reachable from external input.
- Remediation: JSON-escape `name` before interpolating. Either use `serde_json::json!({"error": "tool not found", "tool": name}).to_string()` or at minimum call a small helper that replaces `\` → `\\` and `"` → `\"` in the name string before format interpolation.

**ISSUE-015 — `validate_wasm_rejects_backslash` test exercises the `..` guard, not the backslash guard**
- Location: `arcterm-plugin/src/manifest.rs:399-403`
- Code: `make_manifest_wasm("..\\evil.wasm").validate()`. The input `..\\evil.wasm` contains `..`, so `validate()` returns an error at `line 134` ("must not contain '..'") and never reaches the `contains('\\')` check at `line 139`. The assertion `err.contains("..") || err.contains("backslash")` passes via the `..` branch, leaving the backslash-only check untested.
- Remediation: Change the test input to a path with a backslash but no `..`, e.g. `"sub\\file.wasm"`. This exercises line 139 and returns the "backslash" error. Keep a separate test for the `..\\` combination if desired.

**ISSUE-016 — Epoch ticker thread is never terminated; leaks Engine references**
- Location: `arcterm-plugin/src/runtime.rs:28-32`
- The `std::thread::spawn` creates a detached thread that loops forever. The thread holds `engine_clone` (a cheap `Arc` clone of the internal wasmtime Engine). Because the thread never exits, the Arc refcount never reaches zero, so the `Engine` is never dropped for the lifetime of the process. Each call to `PluginRuntime::new()` (including in every test that constructs a `PluginManager`) spawns one additional permanent thread. With 7+ test functions calling `PluginManager::new_with_dir`, this is 7 leaked threads plus 7 leaked engines per test binary execution. In production there is one `PluginRuntime`, so the impact is limited, but the pattern is fragile.
- Remediation: Use a cancellation mechanism. The simplest option: add a `_ticker_stop: Arc<AtomicBool>` field on `PluginRuntime`; set it to `true` in `Drop`, check it in the loop. A lighter alternative: use `std::thread::Builder::new().name("epoch-ticker")` with a channel and break when the receiver is closed. A wasmtime-idiomatic approach is to use `Engine::increment_epoch` from a thread that receives a shutdown signal on `PluginRuntime::drop`.

### Suggestions

**ISSUE-017 — Double-lock in `call_tool` has a TOCTOU window**
- Location: `arcterm-plugin/src/manager.rs:368-376`
- The lock is released after the ownership check, then re-acquired for the call. In the current design, tool registration only occurs during `call_load` (i.e., before the plugin is inserted into `self.plugins`), so the window is benign. However, if tool registration is ever made dynamic, a plugin could unregister a tool between the two lock acquisitions, causing `call_tool_export` to dispatch a call to a plugin that no longer owns it.
- Remediation: Lock mutably from the start and check ownership within the same critical section: `let mut inst = lp.instance.lock()?; if inst.host_data().registered_tools.iter().any(|t| t.name == name) { return inst.call_tool_export(name, args_json); }`.

**ISSUE-018 — Canonicalize fallback in `load_from_dir` weakens defense-in-depth**
- Location: `arcterm-plugin/src/manager.rs:250-252`
- Code: `wasm_path.canonicalize().unwrap_or(wasm_path.clone())` / `dir.canonicalize().unwrap_or(dir.to_path_buf())`. If the wasm file does not yet exist (e.g., in a partially-installed plugin), `wasm_path.canonicalize()` returns `Err` and falls back to the raw path. The raw path and the canonicalized directory path have different prefix formats (e.g., `../plugins/foo/plugin.wasm` vs `/home/user/.config/arcterm/plugins/foo`), so `wasm_canonical.starts_with(&dir_canonical)` will fail even for a legitimate path — returning a misleading "resolves outside the plugin directory" error instead of "file not found".
- Remediation: If `canonicalize` fails, propagate the underlying error (with context) rather than silently falling back to the raw path. The wasm file must exist to be loaded anyway; missing-file canonicalize failure should produce `Err("plugin wasm not found")` not a silent path bypass.

---

## Summary

**Verdict:** APPROVE

All five must-have fixes (H-1, H-2, M-1, M-2, M-6) are correctly implemented and match the plan's action and done criteria. The documented deviation (OS thread for epoch ticker instead of tokio task) is justified and technically sound. Three Important and two Suggestion findings are logged below; none block this merge in a pre-release codebase, but ISSUE-014 (JSON injection) and ISSUE-015 (test coverage gap) should be addressed before plugin tool dispatch is exposed to untrusted inputs.

Critical: 0 | Important: 3 | Suggestions: 2
