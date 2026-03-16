# CONCERNS.md

## Overview

Arcterm is an early-stage (v0.1.0) project with well-structured foundations and active security mitigations (WASM sandbox, path-traversal validation). The primary concerns are: a non-functional plugin tool-invocation stub, a silent logic bug in event routing, epoch-interruption configured but never ticked (infinite-loop protection is inert), and pervasive use of `#[allow(dead_code)]` indicating significant unfinished integration work. Performance concerns are real but explicitly documented as future-phase work. No hardcoded secrets or credential leakage was detected.

---

## Findings

### Critical

*(No Critical findings. The path-traversal fix in the recent commit `2f4964b` addressed what would have been the most critical security item.)*

---

### High

#### H-1 — Plugin epoch-interruption is configured but never ticked (infinite-loop protection is inert)

- **Severity**: High
- **Description**: `config.epoch_interruption(true)` is set during engine construction, which configures wasmtime to check an epoch counter for cancellation. However, nothing in the codebase ever calls `engine.increment_epoch()` or sets a per-store epoch deadline (`store.set_epoch_deadline()`). The result is that a malicious or buggy WASM plugin that enters an infinite loop will **never be interrupted** — it will deadlock the tokio blocking thread it runs on, consuming a thread from the runtime's thread pool indefinitely.
  - Evidence: `arcterm-plugin/src/runtime.rs:21` — `config.epoch_interruption(true)` set
  - Evidence: `arcterm-plugin/src/runtime.rs` — no `increment_epoch` call anywhere in the file
  - Evidence: Grep across all `*.rs` — zero occurrences of `increment_epoch` or `set_epoch_deadline`
  - **Remediation**: Spawn a background tokio task that calls `engine.increment_epoch()` on a fixed interval (e.g., 10ms). In each `Store`, call `store.set_epoch_deadline(N)` before a WASM call to bound execution time. Without this, the safety mechanism does nothing.

#### H-2 — Plugin tool invocation is a hard-coded stub returning a JSON error

- **Severity**: High (functional gap, not a security issue)
- **Description**: `PluginManager::call_tool()` always returns a JSON error stub `{"error":"tool invocation not yet implemented","tool":"..."}` regardless of which tool is called. Any AI agent relying on the MCP tool-call protocol will receive silent failures. The comment explicitly defers this to "Phase 8".
  - Evidence: `arcterm-plugin/src/manager.rs:353-371`
  - ```rust
    // Full WASM invocation is a Phase 8 deliverable.
    return Ok(format!(
        "{{\"error\":\"tool invocation not yet implemented\",\"tool\":\"{}\"}}",
        name
    ));
    ```
  - **Remediation**: Implement actual WASM function dispatch: look up the tool's registered handler in the WASM component and call it with deserialized args. This is the core value-add of the plugin → AI bridge.

---

### Medium

#### M-1 — `PluginEvent::KeyInput` maps to wrong `EventKind` in `kind()`

- **Severity**: Medium (logic bug, affects event routing)
- **Description**: The `kind()` method on `PluginEvent` returns `WitEventKind::PaneOpened` for `KeyInput` events. This is acknowledged in a comment ("KeyInput does not map to a subscribable EventKind"), but the fallback to `PaneOpened` is silently incorrect: plugins subscribed to `PaneOpened` would theoretically receive key events that were accidentally routed through the broadcast bus. In the current code, `KeyInput` is delivered directly via `send_key_input`, so this may not cause observable bugs today — but it is a landmine for any future code that broadcasts `KeyInput` events through the event bus.
  - Evidence: `arcterm-plugin/src/manager.rs:87-91`
  - ```rust
    PluginEvent::KeyInput { .. } => WitEventKind::PaneOpened,
    ```
  - **Remediation**: Add a dedicated `KeyInput` variant to `WitEventKind` in the WIT file, or return a sentinel/unreachable! to make the incorrect usage obvious at the call site.

#### M-2 — `wasm` field in `plugin.toml` is not validated for path traversal

- **Severity**: Medium (security — mitigated by WASM sandbox, but defence-in-depth is incomplete)
- **Description**: `PluginManifest::validate()` carefully sanitises the `name` field against path-traversal characters. However, the `wasm` field is only checked for non-emptiness (`arcterm-plugin/src/manifest.rs:130`). A `plugin.toml` with `wasm = "../../etc/passwd"` would not fail validation; `load_from_dir` would then attempt to read that path as a WASM binary. The error would be caught at WASM compile time, but the file would be read from the host filesystem.
  - Evidence: `arcterm-plugin/src/manifest.rs:130-138` — only `trim().is_empty()` check on `wasm`
  - Evidence: `arcterm-plugin/src/manager.rs:241-243` — `wasm_path = dir.join(&manifest.wasm)` with no path containment check
  - **Remediation**: Apply the same traversal checks to `manifest.wasm` as to `manifest.name`: reject if it contains `..`, starts with `/`, or contains `\`. Verify the resolved path is still a child of `dir`.

#### M-3 — Synchronous image decode blocks PTY processing thread for large images

- **Severity**: Medium (performance)
- **Description**: Kitty image protocol data is decoded from PNG/JPEG to RGBA using `image::load_from_memory` synchronously in the PTY output processing loop. The code itself documents this limitation with a TODO.
  - Evidence: `arcterm-app/src/terminal.rs:37-39` — `TODO(phase-5): move PNG/JPEG decoding to a background thread for images larger than 1MB`
  - Evidence: `arcterm-app/src/terminal.rs:92` — `image::load_from_memory(&decoded_bytes)` called inline
  - **Remediation**: Move decode to `tokio::task::spawn_blocking` and store a `JoinHandle`; drain decoded results before the next render frame.

#### M-4 — `scrollback_lines` config has no upper-bound validation

- **Severity**: Medium (resource exhaustion)
- **Description**: `scrollback_lines` is a `usize` parsed directly from user TOML with no clamping. A misconfigured or malicious `config.toml` could set this to `usize::MAX`, causing unbounded memory allocation as the scrollback buffer grows without limit.
  - Evidence: `arcterm-app/src/config.rs:32` — `pub scrollback_lines: usize`
  - Evidence: `arcterm-core/src/grid.rs:199` — `while self.scrollback.len() > self.max_scrollback` — the cap is enforced, but only against the unclamped user value
  - **Remediation**: Apply a reasonable cap (e.g., 1,000,000 lines) during config loading. Log a warning when the value is clamped.

#### M-5 — GPU initialisation panics on systems without a suitable GPU adapter

- **Severity**: Medium (availability — poor UX on integrated-GPU or headless machines)
- **Description**: Three `.expect()` calls in `gpu.rs` will panic the process if wgpu cannot find a compatible adapter or device. There is no graceful degradation or user-facing error message.
  - Evidence: `arcterm-render/src/gpu.rs:29` — `expect("failed to create wgpu surface")`
  - Evidence: `arcterm-render/src/gpu.rs:38` — `expect("failed to find a suitable GPU adapter")`
  - Evidence: `arcterm-render/src/gpu.rs:50` — `expect("failed to create wgpu device")`
  - **Remediation**: Return `Result<GpuState, String>` from `GpuState::new`. Display a user-friendly dialog or log fatal message before exiting.

#### M-6 — Plugin file copy does not guard against symlink attacks

- **Severity**: Medium (security — local privilege issue if plugin directories are world-writable)
- **Description**: `copy_plugin_files` iterates `source_path` with `read_dir` and copies every entry to `dest`. If a plugin source directory contains symbolic links pointing outside the source tree, `std::fs::copy` will follow the symlink and copy the target file's contents into the plugin install directory. On a shared system, an attacker controlling the source directory could exfiltrate or plant files.
  - Evidence: `arcterm-plugin/src/manager.rs:215-219`
  - ```rust
    for entry in std::fs::read_dir(source_path)? {
        let entry = entry?;
        let dest_file = dest.join(&file_name);
        std::fs::copy(entry.path(), dest_file)?;
    }
    ```
  - **Remediation**: Check `entry.path().symlink_metadata()` and skip or reject symlinks. Also verify that `dest_file` is still a child of `dest` after resolution.

---

### Low

#### L-1 — Pervasive `#[allow(dead_code)]` indicating incomplete integration

- **Severity**: Low (technical debt, not a bug)
- **Description**: 30+ `#[allow(dead_code)]` suppressions appear across production code in `arcterm-app`. Several entire modules are suppressed at the file level. This is characteristic of phase-gated development where APIs are defined before they are wired up, but it means the compiler cannot warn about code that will never be called.
  - Evidence: `arcterm-app/src/detect.rs:1` — `#![allow(dead_code)] // Wired in PLAN-3.1 integration`
  - Evidence: `arcterm-app/src/workspace.rs:13` — `#![allow(dead_code)]`
  - Evidence: `arcterm-app/src/terminal.rs:13,116,196,213,229` — multiple struct fields and methods
  - Evidence: `arcterm-app/src/context.rs:21,29,85,101`, `arcterm-app/src/tab.rs:21,134`, etc.

#### L-2 — `block_in_place` in `send_key_input` requires multi-threaded Tokio runtime

- **Severity**: Low (runtime configuration dependency)
- **Description**: `tokio::task::block_in_place` is called inside `send_key_input` to run synchronous WASM on the calling async thread. This requires the `multi_thread` runtime; it will panic with `current_thread` runtime. No assertion or documentation enforces this constraint.
  - Evidence: `arcterm-plugin/src/manager.rs:416` — `tokio::task::block_in_place(|| { ... })`

#### L-3 — No upper limit on the number of loaded plugins

- **Severity**: Low (resource exhaustion)
- **Description**: `PluginManager` will load every subdirectory under `plugin_dir` that contains a `plugin.toml`. Each plugin instance allocates up to 10MB of WASM linear memory. There is no cap on the number of plugins that can be loaded simultaneously.
  - Evidence: `arcterm-plugin/src/manager.rs:304-328` — `load_all_installed` iterates all entries without limit
  - Evidence: `arcterm-plugin/src/host.rs:47-49` — 10MB memory limit per-store

#### L-4 — `dirs::config_dir()` fallback to `".config"` may surprise users

- **Severity**: Low (UX — incorrect install path on unusual systems)
- **Description**: Both `PluginManager::new()` and `ArctermConfig::config_path()` use `unwrap_or_else(|| PathBuf::from(".config"))` when `dirs::config_dir()` returns `None`. On unsupported platforms, this silently creates a `.config` directory in the current working directory rather than failing.
  - Evidence: `arcterm-plugin/src/manager.rs:154-157`
  - Evidence: `arcterm-app/src/config.rs:159-161`

#### L-5 — `PluginId` type duplication: `String` alias vs. newtype struct

- **Severity**: Low (technical debt — type confusion)
- **Description**: `PluginId` is defined twice: as a `String` type alias in `arcterm-plugin/src/manager.rs:102` and as a newtype struct `PluginId(pub String)` in `arcterm-plugin/src/types.rs:3`. These are inconsistent; the type alias is used internally, while the newtype is re-exported publicly.
  - Evidence: `arcterm-plugin/src/manager.rs:102` — `pub type PluginId = String;`
  - Evidence: `arcterm-plugin/src/types.rs:3` — `pub struct PluginId(pub String);`

#### L-6 — Incomplete image placement tracking (Kitty protocol)

- **Severity**: Low (feature incompleteness)
- **Description**: Image placements are cleared every frame with `retain(|_| false)`, preventing images from persisting on screen across frames. Proper Kitty image placement tracking (deletion by `image_id`, z-index support) is deferred.
  - Evidence: `arcterm-app/src/main.rs:2155-2156` — `// TODO(phase-5): implement proper placement tracking per image_id.`
  - Evidence: `arcterm-app/src/main.rs:2156` — `state.renderer.image_placements.retain(|_| false);`

#### L-7 — `unsafe` code in process inspection helpers

- **Severity**: Low (justified but warrants review)
- **Description**: `arcterm-app/src/proc.rs` and `arcterm-pty/src/session.rs` use `unsafe` blocks to call macOS `sysctl`/`proc_pidinfo` APIs. The safety invariants are documented inline and appear correct (zeroed structs, bounded slices). However, PID values come from the child process table with no sanitization — if a PID wraps to a negative `c_int`, the sysctl call will fail gracefully (returns ≤ 0) rather than panic.
  - Evidence: `arcterm-app/src/proc.rs:17,58,74`
  - Evidence: `arcterm-pty/src/session.rs:45,48,67,71`

---

## Summary Table

| ID | Item | Severity | Confidence |
|----|------|----------|------------|
| H-1 | Epoch interruption configured but never ticked — infinite-loop protection is inert | High | Observed |
| H-2 | Plugin tool invocation always returns error stub (Phase 8 deferred) | High | Observed |
| M-1 | `KeyInput` event maps to `PaneOpened` kind — silent logic bug in `kind()` | Medium | Observed |
| M-2 | `wasm` field in `plugin.toml` not validated for path traversal | Medium | Observed |
| M-3 | Synchronous Kitty image decode blocks PTY thread for large images | Medium | Observed |
| M-4 | `scrollback_lines` config has no upper-bound validation | Medium | Observed |
| M-5 | GPU init panics with `.expect()` on systems without compatible adapter | Medium | Observed |
| M-6 | Plugin file copy follows symlinks without guard | Medium | Observed |
| L-1 | Pervasive `#[allow(dead_code)]` — 30+ suppressions in production code | Low | Observed |
| L-2 | `block_in_place` in `send_key_input` requires multi-threaded Tokio runtime | Low | Observed |
| L-3 | No upper limit on number of loaded plugins (up to 10MB each) | Low | Observed |
| L-4 | `dirs::config_dir()` fallback to `.config` may create unexpected directory | Low | Observed |
| L-5 | `PluginId` defined as both `String` alias and newtype struct | Low | Observed |
| L-6 | Kitty image placements cleared every frame — no persistence across frames | Low | Observed |
| L-7 | `unsafe` in process inspection helpers (macOS sysctl / proc_pidinfo) | Low | Observed |

---

## Open Questions

1. **H-1 remediation scope**: Is epoch ticking intended to be per-plugin or shared across all stores? A shared ticker works but requires all stores to call `set_epoch_deadline` consistently.
2. **H-2 timeline**: Is Phase 8 (full WASM tool invocation) on the active roadmap, or has it been deferred indefinitely?
3. **M-6 threat model**: Is plugin installation expected to handle untrusted third-party plugins? If yes, symlink attack mitigations and a signature/checksum verification step should be prioritized.
4. **L-5 `PluginId` duplication**: Which type should be the canonical public API — the type alias or the newtype? The newtype provides stronger type safety but requires more boilerplate.
5. **Integration phases**: Multiple `#[allow(dead_code)]` comments reference "Wave 2", "Wave 3", "Phase 5" — is there a single authoritative roadmap document that tracks when these integrations land?
