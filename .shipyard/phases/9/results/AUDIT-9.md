# Security Audit Report — Phase 9

## Executive Summary

**Verdict:** PASS (with two conditions before plugin tool dispatch is exposed to untrusted input)
**Risk Level:** Medium

Phase 9 successfully closes all five previously identified security and correctness issues (H-1, H-2, M-1, M-2, M-6). The most dangerous concern — that epoch interruption was configured but never ticked, leaving WASM plugins able to loop forever — is now correctly remediated with an OS-level ticker thread. Path traversal guards on the `wasm` manifest field are now applied in two layers (manifest validation + canonicalize check). Symlink attacks during plugin file copy are now blocked. No hardcoded secrets, new unsafe blocks, or new dependencies were introduced. Two conditions remain before Phase 9 can be considered fully shippable: the `call_tool` error path emits unescaped JSON (ISSUE-014, already flagged by the per-task reviewer), and the epoch ticker thread leaks per `PluginRuntime::new()` call (ISSUE-016). Neither is exploitable by a remote attacker in the current codebase, but ISSUE-014 becomes a JSON injection sink the moment plugin tool dispatch is reachable from an AI model.

### What to Do

| Priority | Finding | Location | Effort | Action |
|----------|---------|----------|--------|--------|
| 1 | JSON injection in tool-not-found error path | `manager.rs:379-382` | Trivial | Replace format string with `serde_json::json!` or a sanitizing helper |
| 2 | Epoch ticker thread leaks on every `PluginRuntime::new()` | `runtime.rs:29-32` | Small | Add `Arc<AtomicBool>` shutdown flag; check in loop; set on `Drop` |
| 3 | `validate_wasm_rejects_backslash` test covers `..` guard, not backslash guard | `manifest.rs:399-403` | Trivial | Change test input to `"sub\\file.wasm"` — no `..` in path |
| 4 | `canonicalize` fallback silently uses raw path when wasm file absent | `manager.rs:250-252` | Small | Propagate `canonicalize` error as "plugin wasm not found" rather than silently bypassing |
| 5 | Double-lock TOCTOU window in `call_tool` | `manager.rs:368-376` | Small | Acquire mutable lock once and check ownership within same critical section |

### Themes
- Remediation quality is high: all five target issues are correctly closed with defence-in-depth (manifest validation + filesystem canonicalize + test coverage).
- The remaining issues are all concentrated in the plugin tool-dispatch path that was newly activated in this phase — a natural consequence of replacing a stub with real code.
- No new attack surfaces were introduced in the VT processor, grid, or PTY session changes.

---

## Detailed Findings

### Critical

*None.*

---

### Important

**[I1] JSON injection in tool-not-found error message (ISSUE-014)**
- **Location:** `arcterm-plugin/src/manager.rs:379-382`
- **CWE:** CWE-116 (Improper Encoding or Escaping of Output)
- **Description:** When no plugin owns the requested tool name, `call_tool` returns an error JSON string constructed by bare format interpolation: `format!("{{\"error\":\"tool not found\",\"tool\":\"{}\"}}", name)`. The `name` parameter is not escaped. A caller that passes a tool name containing `"` or `\` (e.g., `foo"}}{"injected":"val`) will receive malformed or structurally manipulated JSON. The same pattern appeared in the pre-Phase-9 stub and was flagged by the per-task reviewer (ISSUE-014) but was not fixed in this phase.
- **Impact:** In the current codebase the return value is consumed by the OSC 7770 protocol handler. If an AI model or MCP client constructs a `call_tool` request with a crafted name, the malformed JSON response could cause the consuming parser to misbehave, silently drop the error, or — if the JSON is further embedded in a larger response object without re-encoding — corrupt the outer structure. This is not a remote code execution vector, but it is a correctness failure at a trust boundary that handles AI-generated input.
- **Remediation:** Replace the format interpolation with `serde_json::json!({"error": "tool not found", "tool": name}).to_string()`. This is a one-line change; `serde_json` is already in the dependency tree for this crate.
- **Evidence:**
  ```rust
  Ok(format!(
      "{{\"error\":\"tool not found\",\"tool\":\"{}\"}}",
      name
  ))
  ```

**[I2] Epoch ticker thread is never terminated; leaks per `PluginRuntime::new()` call (ISSUE-016)**
- **Location:** `arcterm-plugin/src/runtime.rs:28-32`
- **CWE:** CWE-404 (Improper Resource Shutdown or Release)
- **Description:** `std::thread::spawn` creates a detached background thread that loops forever, incrementing the epoch. The thread holds an `Arc` clone of the `Engine`. Because the thread never exits, the `Arc` refcount never reaches zero and the `Engine` is never dropped for the lifetime of the process. Each call to `PluginRuntime::new()` — including each test that constructs a `PluginManager` — spawns one additional permanent OS thread. The per-task reviewer counted 7+ test functions each invoking `PluginManager::new_with_dir`; in a long test session this accumulates to dozens of leaked threads.
- **Impact:** In production there is typically one `PluginRuntime` instance, so the steady-state impact is one leaked thread with one `Engine` clone. In tests the accumulation can exhaust the OS thread limit or cause interference between test runs if thread counts are monitored. More significantly, the absence of a `Drop` implementation means the pattern is fragile: if future code creates multiple `PluginRuntime` instances (e.g., to support per-workspace plugin isolation), the leak multiplies.
- **Remediation:** Add a `_ticker_stop: Arc<AtomicBool>` field on `PluginRuntime`. Check the flag inside the thread loop (`if stop.load(Ordering::Relaxed) { break; }`). Implement `Drop for PluginRuntime` to set the flag to `true`. The `Arc<AtomicBool>` can be cloned cheaply and requires no `Mutex`.
- **Evidence:**
  ```rust
  let engine_clone = engine.clone();
  std::thread::spawn(move || loop {
      std::thread::sleep(std::time::Duration::from_millis(10));
      engine_clone.increment_epoch();
  });
  ```

**[I3] `validate_wasm_rejects_backslash` test exercises the `..` guard, not the backslash guard (ISSUE-015)**
- **Location:** `arcterm-plugin/src/manifest.rs:399-403`
- **Description:** The test input `"..\\evil.wasm"` contains `..`, so validation fails at the `contains("..")` check (line 134) before ever reaching the `contains('\\')` check (line 139). The assertion `err.contains("..") || err.contains("backslash")` passes via the `..` branch. The backslash-only code path — the case where an attacker supplies a Windows-style path without `..` — is therefore untested.
- **Impact:** If the `contains('\\')` guard were accidentally removed or incorrectly changed, no test would catch the regression. In a defence-in-depth model where the backslash check is a distinct security control, it deserves its own test.
- **Remediation:** Change the test input to `"sub\\file.wasm"` (contains `\` but no `..`). This directly exercises line 139 and produces the "backslash" error message. A separate test can cover the `..\\` combination if desired.
- **Evidence:**
  ```rust
  fn validate_wasm_rejects_backslash() {
      let err = make_manifest_wasm("..\\evil.wasm")  // triggers ".." guard, not backslash guard
          .validate().expect_err("should reject backslash");
      assert!(err.contains("..") || err.contains("backslash"), "{err}");
  }
  ```

---

### Advisory

- `manager.rs:250-252` — `canonicalize().unwrap_or(raw_path)` silently falls back to the unresolved path when the wasm file does not yet exist; the `starts_with` check will likely fail legitimately (different prefix formats), producing a misleading "resolves outside the plugin directory" error rather than "file not found". Propagate the `canonicalize` error directly instead. (ISSUE-018)

- `manager.rs:368-376` — Double-lock pattern in `call_tool` releases the lock after the ownership check and re-acquires it for dispatch, creating a TOCTOU window. Currently benign (tools only register during `call_load`, before the plugin is inserted into `self.plugins`), but fragile if tool registration is ever made dynamic. (ISSUE-017)

- `arcterm-plugin/src/manifest.rs:139` — The `contains('\\')` check fires only after the `starts_with('\\')` check. The ordering is correct (absolute Windows paths like `\evil.wasm` are caught at line 136 before line 139 would test them), but the comment on line 139 could make this read as a complete check when it is already partially covered. No bug; documentation clarity only.

---

## Verification of Phase 9 Security Fixes

This section confirms each targeted fix from CONCERNS.md was correctly implemented.

### H-1 — Epoch interruption ticker: FIXED

The fix at `runtime.rs:28-32` correctly activates the previously inert `epoch_interruption(true)` setting by spawning a background OS thread that calls `engine_clone.increment_epoch()` every 10ms. All four WASM call sites (`call_load` in both `load_plugin` and `load_plugin_with_wasi`, `call_update`, `call_render`) and the new `call_tool_export` now call `store.set_epoch_deadline(3000)` immediately before the WASM call, establishing a 30-second wall-clock bound. An infinite-looping WASM plugin will now be interrupted.

The choice of OS thread over `tokio::spawn` is correct and documented: sync tests call `PluginRuntime::new()` without an active Tokio runtime; `tokio::spawn` would panic in that context. The thread leak concern (I2 above) does not affect the correctness of the interruption mechanism — it is an operational hygiene issue.

**One gap identified:** The epoch deadline is set to 3000 (30 seconds) before `call_load`. This means a malicious plugin could delay its initialization for up to 30 seconds before being interrupted. This is acceptable for the current threat model (plugins are trusted at install time), but worth noting if untrusted third-party plugins are a future concern.

### H-2 — Full WASM tool dispatch: FIXED

The hard-coded stub at `manager.rs:353-371` has been replaced with real WASM dispatch. The new `call_tool_export` at `runtime.rs:131-137` delegates to `call_call_tool` on the generated WIT bindings, with epoch deadline protection. The WIT interface at `arcterm.wit:77` exports `call-tool: func(name: string, args-json: string) -> string`. The `args_json` parameter is passed through to the WASM component without host-side transformation — the component receives and parses it, so injection risk is at the WASM boundary, not the host.

The JSON injection issue in the tool-not-found fallback (I1) is distinct from the dispatch path and does not affect successful tool calls.

### M-1 — KeyInput event kind mapping: FIXED

`arcterm-plugin/wit/arcterm.wit:26` now includes `key-input` in the `event-kind` enum. `manager.rs:87` correctly maps `PluginEvent::KeyInput { .. } => WitEventKind::KeyInput`. The silent mis-mapping to `PaneOpened` is gone. Test coverage at `manager.rs:582-397` directly asserts the correct mapping.

### M-2 — WASM path traversal validation: FIXED (with advisory gap)

Two independent layers now guard against path traversal in the `wasm` field:

1. **Manifest validation** (`manifest.rs:133-141`): rejects `..` sequences, absolute paths (Unix and Windows), and backslash separators.
2. **Filesystem canonicalize check** (`manager.rs:250-257`): resolves the path and confirms it starts with the canonicalized plugin directory.

The test gap for the backslash guard (I3) leaves one code path without a direct test, but the logic itself is correct.

The advisory concern about `canonicalize().unwrap_or` (advisory section, ISSUE-018) means the second layer has a silent failure mode when the wasm file is absent. The first layer (manifest validation) still fires, so the defence-in-depth is only partially weakened, not eliminated.

### M-6 — Symlink rejection in file copy: FIXED

`manager.rs:215-228` now calls `entry.path().symlink_metadata()` before any copy operation. The use of `symlink_metadata()` (not `metadata()`) is correct: it inspects the link itself, not its target, so a symlink is detected before it can be followed. Non-file entries (directories, sockets) are skipped rather than erroring, which is a reasonable policy. The Unix-only test at `manager.rs:595-612` validates the rejection with a real symlink.

**One gap:** The destination path `dest_file = dest.join(&file_name)` is not checked for containment within `dest`. A `file_name` component of `../escape.txt` would resolve outside the plugin directory. In practice, `file_name()` on a `DirEntry` returns only the final path component (no separators), making this safe by the OS API contract — but the defense relies on that guarantee rather than an explicit check. This is a low-risk advisory item rather than a finding.

---

## Cross-Component Analysis

### Authentication and trust boundary coherence

The plugin system has one trust boundary: the `plugin.toml` manifest, which is read from the filesystem. Phase 9 hardened the manifest validation for the `wasm` field (M-2) and the file copy step (M-6). These fixes operate in concert: `copy_plugin_files` (called during install) now rejects symlinks at the source, and `load_from_dir` (called at load time) now validates the manifest path and confirms filesystem containment. An attacker would need to compromise the plugin install directory between install and load — a local privilege concern documented in CONCERNS.md M-6 — to bypass both layers.

### Data flow: `args_json` passthrough in `call_tool`

The `args_json` string received by `call_tool` flows through `manager.rs → runtime.rs → call_call_tool (WASM bindgen)` without host-side parsing or validation. This is the correct design: the WASM component is responsible for parsing its own arguments. The host does not need to understand the schema. The risk is that a malformed `args_json` (not valid JSON) is passed directly to the component; the component's behavior on invalid input is outside the scope of this audit. The finding (I1) concerns the *error response* path only, not the success path.

### Error information leakage

Phase 9 error messages are appropriately informative without being exploitable. `anyhow::bail!` messages in `manager.rs` name the symlink filename and the wasm path but do not expose host filesystem structure beyond what the plugin installer already knows. Lock-poisoned errors (`"lock poisoned: {e}"`) expose the Mutex error but no sensitive data. No stack traces or internal state are returned to callers.

### Thread safety of epoch ticker

The epoch ticker is safe. `Engine::clone()` is designed to be shared across threads (it is internally `Arc`-wrapped). `increment_epoch()` is documented as thread-safe in wasmtime. The ticker thread never holds any application-level locks. The only concern is the missing shutdown mechanism (I2), which is a resource leak, not a data race.

### Lock ordering in `call_tool` double-lock

The double-lock pattern in `call_tool` (advisory ISSUE-017) acquires `lp.instance.lock()` twice in sequence, never simultaneously. There is no lock ordering hazard because the second lock acquisition does not nest inside the first — the first lock guard is explicitly dropped before the second is acquired (via the block scope on lines 368-374). A deadlock would require a thread to hold the `instance` lock and then attempt to re-acquire it; that cannot happen with this pattern. The concern is TOCTOU correctness, not deadlock.

### Grid arithmetic safety

All four scroll/insert/delete operations refactored in `grid.rs` use in-place cell copy loops instead of `Vec::remove` + `Vec::insert`. The arithmetic for `bottom + 1 - n` cannot underflow because `n` is clamped to `region_height = bottom + 1 - top` before use, and `bottom >= top >= 0` is enforced by `set_scroll_region`. The `checked_sub` guard at lines 182 and 447 correctly handles the case where `n == region_height` and `bottom - n` would wrap. No new integer overflow or underflow risks were introduced.

---

## Analysis Coverage

| Area | Checked | Notes |
|------|---------|-------|
| Code Security (OWASP) | Yes | All 7 changed files reviewed; injection, auth, access control, data exposure checked |
| Secrets & Credentials | Yes | Full scan of all changed files; no hardcoded secrets, API keys, or credentials found |
| Dependencies | Yes | No new dependencies added in Phase 9 diff; lock files not changed |
| Infrastructure as Code | N/A | No IaC files changed |
| Docker/Container | N/A | No Dockerfiles or container configs changed |
| Configuration | Yes | No config files changed; WIT interface additions reviewed |

---

## Dependency Status

No new dependencies were introduced in Phase 9. The diff contains no changes to `Cargo.toml` or `Cargo.lock` files. Previously audited dependencies carry forward unchanged.

| Package | Version | Known CVEs | Status |
|---------|---------|-----------|--------|
| wasmtime | (existing) | None identified at audit time | OK |
| wasmtime-wasi | (existing) | None identified at audit time | OK |
| serde_json | (existing, recommended for I1 fix) | None | OK |

---

## IaC Findings

Not applicable. No infrastructure-as-code files were modified in Phase 9.
