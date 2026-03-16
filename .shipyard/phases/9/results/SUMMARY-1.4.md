# SUMMARY-1.4 — Plugin Fixes (arcterm-plugin)

**Phase:** 9
**Plan:** 1.4
**Status:** Complete
**Commits:**
- `7ff766c` — shipyard(phase-9): M-1 KeyInput kind fix, M-2 wasm path traversal guard, M-6 symlink rejection
- `c35f559` — shipyard(phase-9): H-1 activate epoch interruption with OS-thread ticker and 30s deadlines
- `356e203` — shipyard(phase-9): H-2 full WASM tool dispatch via call-tool WIT export

---

## Task 1: M-1 + M-2 + M-6 — Security and Correctness Fixes

### M-1: KeyInput event kind fix

**Problem:** `PluginEvent::KeyInput` mapped to `WitEventKind::PaneOpened` in the `kind()` method — a silent mis-mapping that would corrupt subscription filtering.

**Fix:**
- Added `key-input` variant to the `event-kind` enum in `arcterm-plugin/wit/arcterm.wit`.
- Changed `PluginEvent::KeyInput { .. } => WitEventKind::PaneOpened` to `WitEventKind::KeyInput` in `manager.rs`.
- Added test `key_input_event_kind_is_key_input` verifying the mapping.

### M-2: WASM path traversal guard

**Problem:** The `wasm` field in `plugin.toml` was only checked for emptiness. Path traversal (`../../evil.wasm`), absolute paths, and backslashes were not rejected.

**Fix:**
- Extended `PluginManifest::validate()` in `manifest.rs` to reject `..`, absolute Unix paths (`/`), absolute Windows paths (`\`), and backslashes.
- Added canonicalize-based defense-in-depth check in `manager.rs` `load_from_dir()` that bails if the resolved wasm path escapes the plugin directory.
- Added three tests: `validate_wasm_rejects_path_traversal`, `validate_wasm_rejects_absolute_unix`, `validate_wasm_rejects_backslash`.

### M-6: Symlink rejection in copy_plugin_files

**Problem:** `copy_plugin_files` iterated directory entries and called `std::fs::copy` without checking for symlinks, allowing a symlink in the plugin source to exfiltrate files.

**Fix:**
- Replaced the copy loop body to call `symlink_metadata()` first, bail on symlinks, skip non-file entries (directories, sockets, etc.), then copy.
- Added unix-only test `copy_plugin_files_rejects_symlinks` that creates a symlink in a temp source dir and asserts `copy_plugin_files` returns an error.

---

## Task 2: H-1 — Epoch Interruption Ticker

**Problem:** `epoch_interruption(true)` was set on the Engine but `increment_epoch()` was never called. Epoch deadlines never fired; runaway plugins could execute indefinitely.

**Fix:**
- Spawned a background OS thread in `PluginRuntime::new()` (after engine creation) that calls `engine.increment_epoch()` every 10ms in a loop.
- Used `std::thread::spawn` instead of `tokio::spawn` — the ticker only needs `thread::sleep`, not async I/O, and this avoids a panic in synchronous test contexts where no tokio reactor is running.
- Added `store.set_epoch_deadline(3000)` (3000 × 10ms = 30 seconds) before every WASM entry point: `call_load` (in both `load_plugin` and `load_plugin_with_wasi`), `call_update`, and `call_render`.

**Deviation:** Used OS thread instead of tokio task. The plan suggested `tokio::spawn` but sync unit tests (e.g., `install_copies_files_to_plugin_dir`) call `PluginRuntime::new()` without a tokio runtime, which panicked. OS thread is equivalent for this use case and simpler.

---

## Task 3: H-2 — Full WASM Tool Dispatch

**Problem:** `call_tool()` in `manager.rs` was a stub that returned a hardcoded error JSON. The WIT file had no `call-tool` export.

**Fix:**
- Added `export call-tool: func(name: string, args-json: string) -> string;` to the world block in `arcterm.wit`. The `bindgen!` macro regenerated `call_call_tool` automatically on `ArctermPlugin`.
- Added `call_tool_export(&mut self, name: &str, args_json: &str) -> anyhow::Result<String>` on `PluginInstance` in `runtime.rs` — sets 30s epoch deadline, then calls `call_call_tool`.
- Replaced the stub `call_tool()` in `PluginManager` with real dispatch: iterates plugins, checks ownership via `registered_tools` (lock held read-only), then acquires the lock again mutably to call `call_tool_export`. Returns a "tool not found" JSON if no plugin owns the tool.
- Removed the "Phase 8 deliverable" stub comment.

**WAT integration test:** Not added — WAT component compilation for WIT components requires significant infrastructure (wasm-tools chain, a full component adapter). Documented as a gap; verified by successful `cargo build` with the new WIT export.

---

## Final Verification

```
cargo build -p arcterm-plugin  → ok
cargo test -p arcterm-plugin   → 25 tests: 22 lib + 3 integration, 0 failed
cargo clippy -p arcterm-plugin -- -D warnings → clean
```

## Files Modified

| File | Changes |
|------|---------|
| `arcterm-plugin/wit/arcterm.wit` | Added `key-input` to `event-kind` enum; added `export call-tool` |
| `arcterm-plugin/src/manager.rs` | Fixed `kind()` mapping; added canonicalize guard; fixed copy loop for symlinks; replaced `call_tool` stub; added 3 tests |
| `arcterm-plugin/src/manifest.rs` | Added wasm path validation; added 3 tests |
| `arcterm-plugin/src/runtime.rs` | Added OS-thread epoch ticker; added `epoch_deadline(3000)` to all WASM entry points; added `call_tool_export` |
