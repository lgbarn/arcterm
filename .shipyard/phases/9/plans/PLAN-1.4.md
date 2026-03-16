---
phase: foundation-fixes
plan: "1.4"
wave: 1
dependencies: []
must_haves:
  - H-1 epoch interruption ticking with 10ms interval and 3000-tick (30s) deadline
  - H-2 full WASM tool dispatch via WIT call-tool export
  - M-1 KeyInput event kind maps to dedicated WIT variant
  - M-2 wasm path validated against traversal and absolute paths
  - M-6 copy_plugin_files rejects symlinks
files_touched:
  - arcterm-plugin/src/runtime.rs
  - arcterm-plugin/src/manager.rs
  - arcterm-plugin/src/manifest.rs
  - arcterm-plugin/wit/arcterm.wit
tdd: false
---

# PLAN-1.4 — Plugin Fixes (arcterm-plugin)

## Context

Five issues in `arcterm-plugin` span security, correctness, and functionality:

- **H-1**: `epoch_interruption(true)` is configured in the wasmtime `Engine` but `increment_epoch()` is never called. Without ticking, epoch deadlines never fire and runaway WASM plugins execute indefinitely.

- **H-2**: `call_tool()` in `manager.rs` is a stub that returns a hardcoded error JSON. The WIT file has no `call-tool` export. Full dispatch requires adding the WIT export, updating the manager to call through wasmtime, and setting epoch deadlines.

- **M-1**: `PluginEvent::KeyInput` maps to `WitEventKind::PaneOpened` in the `kind()` method — a silent mis-mapping. A dedicated `key-input` variant must be added to the WIT `event-kind` enum.

- **M-2**: The `wasm` field in `plugin.toml` is only checked for emptiness. Path traversal (`../../evil.wasm`), absolute paths (`/etc/passwd`), and backslashes are not rejected.

- **M-6**: `copy_plugin_files` iterates directory entries and calls `std::fs::copy` without checking for symlinks. A symlink in the plugin source directory could exfiltrate files from outside the plugin directory.

**WIT file coordination:** M-1 and H-2 both modify `arcterm.wit`. M-1 adds `key-input` to the `event-kind` enum. H-2 adds `export call-tool: func(...)`. These changes are compatible and are applied in the same task to avoid merge conflicts.

## Dependencies

None. This plan touches only `arcterm-plugin/` files, which are independent of the other three crates.

## Tasks

<task id="1" files="arcterm-plugin/wit/arcterm.wit, arcterm-plugin/src/manager.rs, arcterm-plugin/src/manifest.rs" tdd="false">
  <action>
  Fix M-1 (KeyInput event kind), M-2 (wasm path traversal), and M-6 (symlink rejection). These are three independent correctness/security fixes that can be applied atomically.

  **M-1 — WIT + manager.rs:**

  1. In `arcterm.wit` (line 21-26), add `key-input` to the `event-kind` enum:
  ```wit
  enum event-kind {
      pane-opened,
      pane-closed,
      command-executed,
      workspace-switched,
      key-input,
  }
  ```

  2. In `manager.rs` line 89, change:
  ```rust
  PluginEvent::KeyInput { .. } => WitEventKind::PaneOpened,
  ```
  to:
  ```rust
  PluginEvent::KeyInput { .. } => WitEventKind::KeyInput,
  ```

  3. Add test in `manager.rs` test module:
  ```rust
  #[test]
  fn key_input_event_kind_is_key_input() {
      let ev = PluginEvent::KeyInput {
          key_char: Some("a".to_string()),
          key_name: "a".to_string(),
          modifiers: KeyInputModifiers::default(),
      };
      assert!(matches!(ev.kind(), WitEventKind::KeyInput));
  }
  ```

  **M-2 — manifest.rs `validate()`:**

  Extend the `wasm` validation block (after line 133) to reject path traversal, absolute paths, and backslashes:
  ```rust
  if self.wasm.contains("..") {
      return Err(format!("plugin wasm path '{}' must not contain '..'", self.wasm));
  }
  if self.wasm.starts_with('/') || self.wasm.starts_with('\\') {
      return Err(format!("plugin wasm path '{}' must not be an absolute path", self.wasm));
  }
  if self.wasm.contains('\\') {
      return Err(format!("plugin wasm path '{}' must not contain backslashes", self.wasm));
  }
  ```

  Add defence-in-depth canonicalize check in `manager.rs` `load_from_dir()` after `dir.join(&manifest.wasm)` (before `std::fs::read`):
  ```rust
  let wasm_canonical = wasm_path.canonicalize().unwrap_or(wasm_path.clone());
  let dir_canonical = dir.canonicalize().unwrap_or(dir.to_path_buf());
  if !wasm_canonical.starts_with(&dir_canonical) {
      anyhow::bail!("plugin wasm path '{}' resolves outside the plugin directory", manifest.wasm);
  }
  ```

  Add tests in `manifest.rs` test module:
  - `validate_wasm_rejects_path_traversal`: `wasm = "../../evil.wasm"` must fail
  - `validate_wasm_rejects_absolute_unix`: `wasm = "/etc/evil.wasm"` must fail
  - `validate_wasm_rejects_backslash`: `wasm = "..\\evil.wasm"` must fail

  **M-6 — manager.rs `copy_plugin_files()`:**

  Replace the `for entry in std::fs::read_dir(source_path)?` body (lines 215-220) to add symlink and directory checks:
  ```rust
  for entry in std::fs::read_dir(source_path)? {
      let entry = entry?;
      let metadata = entry.path().symlink_metadata()?;
      if metadata.file_type().is_symlink() {
          anyhow::bail!(
              "plugin source directory contains a symlink '{}'; symlinks are not permitted",
              entry.file_name().to_string_lossy()
          );
      }
      if !metadata.is_file() {
          continue;
      }
      let file_name = entry.file_name();
      let dest_file = dest.join(&file_name);
      std::fs::copy(entry.path(), &dest_file)?;
  }
  ```

  Add test (unix-only) in `manager.rs` test module:
  ```rust
  #[cfg(unix)]
  #[test]
  fn copy_plugin_files_rejects_symlinks() {
      // Create temp dir with a symlink, assert copy fails
  }
  ```
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-plugin -- key_input_event_kind && cargo test -p arcterm-plugin -- validate_wasm_rejects && cargo test -p arcterm-plugin -- copy_plugin_files_rejects_symlinks</verify>
  <done>M-1: `KeyInput` maps to `WitEventKind::KeyInput`. M-2: path traversal, absolute paths, and backslashes are rejected. M-6: symlinks in plugin source directories cause an error. All tests pass.</done>
</task>

<task id="2" files="arcterm-plugin/src/runtime.rs" tdd="false">
  <action>
  Fix H-1 — activate epoch interruption by spawning a background ticker and setting deadlines before WASM calls.

  **1. Epoch ticker in `PluginRuntime::new()`:**

  After the engine is created (line 23), spawn a background tokio task:
  ```rust
  let engine_clone = engine.clone();
  tokio::spawn(async move {
      let mut interval = tokio::time::interval(std::time::Duration::from_millis(10));
      loop {
          interval.tick().await;
          engine_clone.increment_epoch();
      }
  });
  ```

  **2. Set epoch deadline before every WASM call:**

  In `PluginInstance::call_update()`, add before the WASM call:
  ```rust
  self.store.set_epoch_deadline(3000); // 3000 × 10ms = 30 seconds
  ```

  In `PluginInstance::call_render()`, add before the WASM call:
  ```rust
  self.store.set_epoch_deadline(3000);
  ```

  In `load_plugin` / `load_plugin_with_wasi` — add `store.set_epoch_deadline(3000)` before calling `instance.call_load(&mut store)`.

  **3. Doc comments:**

  Add `/// 3000 epochs at 10ms tick interval = 30-second deadline.` above the constant or inline.

  **Test note:** A full unit test requires a WAT module with an infinite loop. This is validated in Task 3 alongside H-2's test, which already requires WAT compilation infrastructure.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-plugin && cargo clippy -p arcterm-plugin -- -D warnings</verify>
  <done>Epoch ticker spawned. All WASM entry points (`call_update`, `call_render`, `call_load`) set `epoch_deadline(3000)` before invocation. Existing tests pass. Clippy clean.</done>
</task>

<task id="3" files="arcterm-plugin/src/runtime.rs, arcterm-plugin/src/manager.rs, arcterm-plugin/wit/arcterm.wit" tdd="false">
  <action>
  Fix H-2 — implement full WASM tool dispatch replacing the stub.

  **1. Add WIT export** in `arcterm.wit` world block (after the existing `export render`):
  ```wit
  export call-tool: func(name: string, args-json: string) -> string;
  ```

  The `bindgen!` macro in `host.rs` regenerates automatically. `ArctermPlugin` gains a `call_call_tool(&mut store, &str, &str) -> Result<String>` method.

  **2. Add `call_tool_export` to `PluginInstance`** in `runtime.rs`:
  ```rust
  /// Dispatch a tool call to the WASM plugin's `call-tool` export.
  pub fn call_tool_export(&mut self, name: &str, args_json: &str) -> anyhow::Result<String> {
      self.store.set_epoch_deadline(3000);
      let result = self.instance.call_call_tool(&mut self.store, name, args_json)?;
      Ok(result)
  }
  ```

  **3. Replace the stub in `PluginManager::call_tool()`** (manager.rs lines 352-371):
  ```rust
  pub fn call_tool(&self, name: &str, args_json: &str) -> anyhow::Result<String> {
      for lp in self.plugins.values() {
          let owned = {
              let inst = lp.instance.lock()
                  .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
              inst.host_data().registered_tools.iter().any(|t| t.name == name)
          };
          if owned {
              let mut inst = lp.instance.lock()
                  .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
              return inst.call_tool_export(name, args_json);
          }
      }
      Ok(format!(
          "{{\"error\":\"tool not found\",\"tool\":\"{}\"}}",
          name
      ))
  }
  ```

  Key changes from the stub:
  - `_args_json` renamed to `args_json` (remove underscore prefix)
  - Lock acquired twice: first read-only for ownership check, then mutable for the call (avoids holding lock during scan)
  - Real WASM dispatch via `call_tool_export` instead of hardcoded error string

  **4. Remove the "Phase 8 deliverable" comment** from the old stub code.

  **Test:** If WAT infrastructure is straightforward to set up, add an integration test that compiles a minimal WAT component implementing the `call-tool` export and verifies round-trip dispatch. If WAT test setup is too complex for this plan, document the gap and verify manually that `cargo build -p arcterm-plugin` succeeds with the new WIT export.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-plugin && cargo test -p arcterm-plugin && cargo clippy -p arcterm-plugin -- -D warnings</verify>
  <done>`call_tool()` dispatches to the real WASM `call-tool` export. The stub is gone. `arcterm.wit` has the `call-tool` export and `key-input` event-kind variant. Build, tests, and clippy pass.</done>
</task>

## Final Verification

```bash
cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-plugin && cargo test -p arcterm-plugin && cargo clippy -p arcterm-plugin -- -D warnings
```

All plugin fixes applied. Build succeeds with updated WIT. All tests pass. Clippy is clean.

## Notes

- **WIT file is modified by both Task 1 and Task 3.** Tasks within a plan are sequential, so Task 1 adds `key-input` to the enum and Task 3 adds `export call-tool`. No conflict.
- **Existing guest WASM binaries** compiled against the old WIT will not be compatible. This is acceptable — v0.1.1 is pre-release with no published plugins.
- **H-1 epoch ticker** runs indefinitely in a background tokio task. The `Engine` is reference-counted internally, so the clone is cheap. The task will be cleaned up when the tokio runtime shuts down.
