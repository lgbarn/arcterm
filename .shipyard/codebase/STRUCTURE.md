# STRUCTURE.md

## Overview

ArcTerm is organized as a single Cargo workspace with approximately 45 crates at the repository root. The crates span four concerns: binaries (user-facing CLI and GUI), core terminal logic, multiplexer infrastructure, and utility/support libraries. ArcTerm now includes two dedicated crates — `arcterm-ai` and `arcterm-wasm-plugin` — added as the first two entries in the workspace `members` array.

---

## Findings

### Workspace Organization

The workspace is declared in `Cargo.toml` (root). The `members` array lists the following (full list as declared):

| Crate (directory) | Package name | Role |
|---|---|---|
| `arcterm-ai/` | `arcterm-ai` | AI integration: LLM backends (Ollama, Claude), pane context, system prompts, destructive detection, inline suggestions, agent sessions. |
| `arcterm-wasm-plugin/` | `arcterm-wasm-plugin` | WASM plugin system: wasmtime v36 Component Model, capability sandbox, host API linker, event routing. |
| `wezterm-gui/` | `wezterm-gui` | Main GUI binary. Window creation, rendering, key/mouse handling, AI overlay modules. |
| `wezterm/` | `wezterm` | CLI binary. Delegates GUI subcommands to `wezterm-gui` via exec. |
| `wezterm-mux-server/` | `wezterm-mux-server` | Standalone headless mux server binary. |
| `wezterm-blob-leases/` | `wezterm-blob-leases` | Temporary blob storage for image data transfers. |
| `wezterm-cell/` | `wezterm-cell` | Cell attribute types (color, text style, decoration). |
| `wezterm-escape-parser/` | `wezterm-escape-parser` | Escape sequence parser wrapper crate. |
| `wezterm-dynamic/` | `wezterm-dynamic` | Dynamic value type for Lua ↔ Rust config bridging. |
| `wezterm-open-url/` | `wezterm-open-url` | Platform URL opener. |
| `wezterm-ssh/` | `wezterm-ssh` | SSH session management (libssh2 or libssh-rs backends). |
| `wezterm-surface/` | `wezterm-surface` | Surface/line model shared between `term` and `termwiz`. |
| `wezterm-uds/` | `wezterm-uds` | Unix domain socket listener. |
| `bidi/` | `wezterm-bidi` | Bidirectional text algorithm. |
| `bidi/generate` | *(codegen)* | Code generator for bidi tables. |
| `strip-ansi-escapes/` | `strip-ansi-escapes` | Strip ANSI escapes from strings. |
| `sync-color-schemes/` | `sync-color-schemes` | Color scheme synchronization tool. |
| `deps/cairo` | *(vendored)* | Vendored Cairo graphics library. |

**Note**: The root `Cargo.toml` `members` array lists only 18 top-level entries plus `bidi/generate` and `deps/cairo`. Many crates present in the repository (`mux/`, `term/`, `config/`, `pty/`, `window/`, `termwiz/`, `codec/`, `promise/`, `wezterm-font/`, `lua-api-crates/`, etc.) are **not** listed as explicit workspace members — they are referenced only as path dependencies via `[workspace.dependencies]`. They are still part of the workspace graph.

- Evidence: `Cargo.toml` lines 1-26

**Crates excluded from the workspace** (in `exclude`):
- `termwiz/codegen`
- `wezterm-char-props/codegen`

**Removed**: `arcterm-structured-output` is not present anywhere in the repository (no directory, no workspace entry, no import).

### ArcTerm-Specific Crates

#### `arcterm-ai/`

Provides the LLM abstraction layer for all AI features.

| Module | File | Purpose |
|---|---|---|
| `backend` | `src/backend/mod.rs`, `ollama.rs`, `claude.rs` | `LlmBackend` trait + `OllamaBackend` / `ClaudeBackend` implementations. `create_backend()` factory. |
| `config` | `src/config.rs` | `AiConfig` struct; defaults to Ollama at `http://localhost:11434`, model `qwen2.5-coder:7b`. |
| `context` | `src/context.rs` | `PaneContext`: scrollback, CWD, foreground process, dimensions. `format_for_llm()` formats for LLM messages. |
| `prompts` | `src/prompts.rs` | `AI_PANE_SYSTEM_PROMPT`, `COMMAND_OVERLAY_SYSTEM_PROMPT`, `format_context_message()`. |
| `agent` | `src/agent.rs` | `AgentSession` state machine (Planning → Reviewing → Executing → Completed/Aborted/StepFailed). `build_agent_query()`, `parse_steps()`. |
| `suggestions` | `src/suggestions.rs` | `SuggestionConfig` (debounce_ms=300, accept_key="Tab"). `is_at_shell_prompt()`, `build_suggestion_query()`, `clean_suggestion()`. |
| `destructive` | `src/destructive.rs` | `is_destructive()` pattern list, `maybe_warn()`. |

Evidence: `arcterm-ai/src/lib.rs`; individual source files.

#### `arcterm-wasm-plugin/`

Provides the WASM plugin system using wasmtime Component Model.

| Module | File | Purpose |
|---|---|---|
| `capability` | `src/capability.rs` | `Capability::parse()`, `CapabilitySet::new/check()`. Path-prefix enforcement + `..` traversal rejection. `terminal:read` always granted. |
| `config` | `src/config.rs` | `WasmPluginConfig` (path, capabilities, memory_limit_mb=64, fuel_per_callback=1_000_000). `REGISTERED_PLUGINS` global Mutex. `register_plugin()`, `take_registered_plugins()`. |
| `loader` | `src/loader.rs` | `create_engine()` (fuel + Component Model). `load_plugin()`: reads `.wasm` → compiles `Component` → creates `Store<PluginStoreData>`. `refuel_store()`. |
| `lifecycle` | `src/lifecycle.rs` | `PluginState` enum, `Plugin` struct, `PluginManager::load_all()` / `shutdown_all()`. |
| `host_api` | `src/host_api.rs` | Registers 4 host interface groups into `wasmtime::component::Linker`: `arcterm:plugin/log`, `filesystem`, `network`, `terminal`. `create_default_linker()`. |
| `event_router` | `src/event_router.rs` | `EventRouter`: pub/sub for `PluginEvent` (OutputChanged, Bell, FocusChanged, KeyBindingTriggered). |

WIT contract: `arcterm-wasm-plugin/wit/plugin-host-api.wit` — defines world `arcterm-plugin@1.0.0` with host imports (terminal-read, terminal-write, filesystem, network, keybindings, log) and guest exports (lifecycle, events).

Evidence: `arcterm-wasm-plugin/src/lib.rs`; individual source files; `wit/plugin-host-api.wit`.

### Lua API Extension Crates (`lua-api-crates/`)

These crates expose Rust functionality to the Lua config/scripting layer. Each registers itself via `config::lua::add_context_setup_func`.

| Subcrate | Exposed as |
|---|---|
| `battery/` | `wezterm.battery` |
| `color-funcs/` | Color functions |
| `filesystem/` | `wezterm.filesystem` |
| `logging/` | `wezterm.log` |
| `mux/` | `wezterm.mux` |
| `plugin/` | `wezterm.plugin` — downloads/loads Lua plugins from GitHub repos into `DATA_DIR/plugins/` |
| `procinfo-funcs/` | `wezterm.procinfo` |
| `serde-funcs/` | JSON/YAML helpers |
| `share-data/` | Shared mutable Lua state |
| `spawn-funcs/` | Spawn helpers |
| `ssh-funcs/` | SSH config inspection |
| `termwiz-funcs/` | Termwiz surface helpers |
| `time-funcs/` | Time helpers |
| `url-funcs/` | URL parsing |
| `window-funcs/` | Window scripting API |

Evidence: `lua-api-crates/` directory listing; `lua-api-crates/plugin/src/lib.rs` lines 95-96, 158-160.

### Binary Entry Points

| Binary | Source | Purpose |
|---|---|---|
| `wezterm-gui` | `wezterm-gui/src/main.rs` | Primary GUI process. Parses CLI, initializes mux, starts event loop. Includes `mod ai_pane` and `pub mod ai_command_overlay` for AI features. |
| `wezterm` | `wezterm/src/main.rs` | Thin CLI wrapper. GUI subcommands (`start`, `ssh`, etc.) are delegated via `exec` to `wezterm-gui`. |
| `wezterm-mux-server` | `wezterm-mux-server/` | Headless multiplexer server (no GUI). |

- The `wezterm` binary's `delegate_to_gui` function resolves `wezterm-gui` from the same directory as the running executable and `exec`s it.
  - Evidence: `wezterm/src/main.rs` lines 768-813
- The `wezterm-gui` binary's `main()` calls `run()` → `run_terminal_gui()` → `build_initial_mux()` → `GuiFrontEnd::try_new()` → `gui.run_forever()`.
  - Evidence: `wezterm-gui/src/main.rs` lines 829-841, 720-791

### Module Dependency Graph (Simplified)

```
wezterm-gui ──► arcterm-ai ──► mux ──► term (wezterm-term)
            │               │       └──► pty (portable-pty)
            │               │       └──► wezterm-ssh
            │               │       └──► config
            │               │       └──► termwiz
            │               └──► config
            ├──► arcterm-wasm-plugin ──► mux
            │                        └──► config
            │                        └──► wasmtime (v36)
            ├──► window ──► wezterm-input-types
            ├──► wezterm-font
            ├──► wezterm-client ──► codec ──► mux
            ├──► wezterm-mux-server-impl ──► codec
            ├──► config ──► wezterm-dynamic
            │           └──► luahelper ──► mlua
            ├──► promise (no GUI deps; pure async scheduler)
            └──► lua-api-crates/* ──► config, mux, window
```

Key observations:
- `arcterm-ai` depends on `mux` and `config` but not on `wezterm-gui` or `window` — it is a pure logic crate.
- `arcterm-wasm-plugin` depends on `mux` and `config` but not on `wezterm-gui` or `window` — it is a pure infrastructure crate.
- Both ArcTerm crates use `ureq` / `wasmtime` respectively, which are not workspace-level shared dependencies and are declared directly in their own `Cargo.toml`.
- `promise` has no dependency on `window` or GUI; it only provides the scheduling abstraction.
- `codec` depends on `mux` (for type definitions) but not on `wezterm-gui`.
- `term` has no dependency on `mux`, `window`, or `wezterm-gui` — it is a pure state machine.
- `config` depends on `mlua` (Lua runtime) and `wezterm-dynamic` but not on `mux` or `window`.

Evidence: `arcterm-ai/Cargo.toml`; `arcterm-wasm-plugin/Cargo.toml`; `wezterm-gui/Cargo.toml` lines 38, 99; cross-checked against `wezterm-gui/Cargo.toml`, `mux/Cargo.toml`, `term/Cargo.toml`.

### Configuration Loading Path

1. **CLI parsing**: `wezterm-gui/src/main.rs` → `Opt::parse()` extracts `--config-file`, `--config name=value`, `--skip-config` flags.
2. **`config::common_init`** (`config/src/lib.rs` line 344): stores the override path and any `--config` overrides in global statics, then triggers a config reload.
3. **`Config::load_with_overrides`** (`config/src/config.rs` line 1003): searches for the config file in priority order:
   - (Windows only) `<exe_dir>/arcterm.lua` (then `wezterm.lua` as fallback)
   - `$ARCTERM_CONFIG_FILE` env var (takes precedence; `$WEZTERM_CONFIG_FILE` also accepted with deprecation warning)
   - `--config-file` CLI override
   - `~/.arcterm.lua`
   - `$XDG_CONFIG_HOME/arcterm/arcterm.lua` (probed as sibling of the wezterm XDG dirs)
   - `~/.wezterm.lua` (deprecated fallback — emits deprecation notice in logs)
   - `$XDG_CONFIG_HOME/wezterm/wezterm.lua` (deprecated fallback)
   - Falls back to built-in defaults if none found.
   - Evidence: `config/src/config.rs` lines 1009-1078
4. **Lua execution**: `config::lua::make_lua_context` (`config/src/lua.rs` line 211) creates an `mlua::Lua` instance, sets up `package.path` to include `CONFIG_DIRS`, and runs all registered `add_context_setup_func` callbacks.
   - Evidence: `wezterm-gui/src/main.rs` lines 1205-1207
5. **Live reload**: `config::subscribe_to_config_reload` registers callbacks; `notify` crate watches the config file for changes. [Inferred] Reloads are triggered from a background thread and scheduled onto the main thread via `promise::spawn_into_main_thread`.

**Update from previous analysis**: `arcterm.lua` is now fully implemented in the config resolution code. The search order probes `arcterm.lua` paths first, with all `wezterm.lua` paths as deprecated fallbacks that emit log warnings.
- Evidence: `config/src/config.rs` lines 1009-1017, 1044-1052, 1076-1078

### Plugin Loading Path

The existing Lua plugin system (not the WASM system) works as follows:

1. User calls `wezterm.plugin.require("https://github.com/owner/repo")` in their config.
2. `lua-api-crates/plugin/src/lib.rs::require_plugin` (line 187) downloads the repo as a tarball.
3. Extracted into `$DATA_DIR/plugins/<component>/` directory.
4. Plugin's `plugin.lua` is loaded via the Lua `require` mechanism.
   - Evidence: `lua-api-crates/plugin/src/lib.rs` lines 62-100, 158-187

The WASM plugin system path (`arcterm-wasm-plugin`) is implemented in the crate but not yet wired into `wezterm-gui` startup. Registration via a `wezterm.plugin.register_wasm()` Lua function is referenced in `arcterm-wasm-plugin/src/config.rs` documentation but the Lua-side registration was not confirmed in `lua-api-crates/`.

### Test Organization

| Location | Type | Coverage |
|---|---|---|
| `arcterm-ai/src/` | Inline `#[test]` in every module | Comprehensive: backend, config, context, agent, suggestions, destructive, prompts |
| `arcterm-wasm-plugin/src/` | Inline `#[test]` in every module + integration tests in `tests/` | capability, loader, lifecycle, host_api, event_router |
| `arcterm-wasm-plugin/tests/` | `backend_tests.rs`, `integration_tests.rs` | Integration tests; fixture `.wasm` files in `tests/fixtures/` |
| `term/src/test/` | Unit tests — VT state machine | Extensive: `c0.rs`, `c1.rs`, `csi.rs`, `keyboard.rs`, `kitty.rs`, `iterm.rs`, `sixel.rs`, `mouse.rs`, `image.rs`, `selection.rs` |
| `wezterm-dynamic/tests/` | Unit tests — dynamic value serde | `fromdynamic.rs`, `todynamic.rs` |
| `bidi/tests/` | Conformance tests — bidi algorithm | `conformance.rs` |
| `wezterm-ssh/tests/` | Integration / e2e tests — SSH | Requires a running sshd; `e2e/sftp/`, `e2e/agent_forward.rs` |
| Inline `#[test]` | Scattered throughout crates | Common in `codec/`, `config/`, `rangeset/` |

- The main command to run all tests is `cargo test --all` (from `CLAUDE.md`).
- No CI configuration file was found in the repository at time of analysis, so CI test execution cannot be verified.

### Shared / Common Code Locations

| Purpose | Location |
|---|---|
| Config types and Lua bridge | `config/src/` |
| Mux notification bus | `mux/src/lib.rs` (`MuxNotification` enum, `Mux::subscribe`) |
| Terminal cell model | `wezterm-cell/src/` + `wezterm-surface/src/` |
| Input event types | `wezterm-input-types/src/` |
| Dynamic Lua ↔ Rust values | `wezterm-dynamic/src/` |
| Async spawn primitives | `promise/src/spawn.rs` |
| Cross-platform file descriptors | `filedescriptor/src/` |
| LLM backend abstraction | `arcterm-ai/src/backend/` |
| WASM capability enforcement | `arcterm-wasm-plugin/src/capability.rs` |
| AI system prompts | `arcterm-ai/src/prompts.rs` |

---

## Summary Table

| Item | Detail | Confidence |
|------|--------|------------|
| Total top-level crate directories | ~45 | Observed |
| Workspace members (explicitly declared) | 20 (incl. `arcterm-ai`, `arcterm-wasm-plugin`) | Observed |
| ArcTerm-specific crates | `arcterm-ai`, `arcterm-wasm-plugin` | Observed |
| `arcterm-structured-output` crate | Removed; absent from workspace and filesystem | Observed |
| GUI binary entry point | `wezterm-gui/src/main.rs::main` | Observed |
| CLI binary entry point | `wezterm/src/main.rs::main` (delegates to GUI) | Observed |
| Config file priority | `arcterm.lua` first; `wezterm.lua` as deprecated fallback (with log warning) | Observed |
| `arcterm.lua` config name | Fully implemented in search path | Observed |
| Plugin directory (Lua) | `$DATA_DIR/plugins/` | Observed |
| WASM plugin wired into GUI | Not yet; crate is a dep but no call site found | Observed |
| Test framework | Rust built-in `#[test]` + `rstest` (workspace dep) | Observed |
| SSH test requires external sshd | Yes | Observed |

## Open Questions

- The workspace `members` array in `Cargo.toml` does not include all of the `lua-api-crates/*` subcrates individually — they may be referenced only as path dependencies from other crates. Their exact workspace membership status needs verification via `cargo metadata`.
- No `.github/workflows/` or `ci/` directory was found containing CI pipeline definitions, making it impossible to confirm how tests are run in CI.
- `wezterm-char-props/` is present at the root but its `codegen` subcrate is excluded from the workspace; the relationship and build order are not fully traced.
- The `wezterm.plugin.register_wasm()` Lua function is documented as the WASM plugin registration entry point in `arcterm-wasm-plugin/src/config.rs`, but the actual Lua-side binding in `lua-api-crates/` was not confirmed to exist.
- `arcterm-wasm-plugin` is declared as a dependency of `wezterm-gui` (`wezterm-gui/Cargo.toml` line 99) but no call to `PluginManager::load_all` or `take_registered_plugins` was found in `wezterm-gui/src/`. The startup integration call site is missing.
