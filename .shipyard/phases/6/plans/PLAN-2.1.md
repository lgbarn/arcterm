---
phase: wasm-plugin-system
plan: "2.1"
wave: 2
dependencies: ["1.1"]
must_haves:
  - PluginManifest struct parsed from plugin.toml with fields name, version, api_version, wasm path, and permissions (filesystem paths, network bool, panes enum, ai bool)
  - WasiCtxBuilder configuration driven by manifest permissions (preopened_dir for each filesystem path, no socket if network=false)
  - PluginManager that owns HashMap<PluginId, PluginInstance> and a broadcast::Sender<PluginEvent>
  - Event emission from AppState (PaneOpened, PaneClosed, CommandExecuted, WorkspaceSwitched) routed to plugin instances
  - Plugin instances receive events via broadcast::Receiver and call update() only for subscribed event kinds
  - CLI subcommands: arcterm plugin install <path>, arcterm plugin list, arcterm plugin remove <name>, arcterm plugin dev <path>
  - Plugin storage at ~/.config/arcterm/plugins/<name>/ with plugin.toml + wasm file
files_touched:
  - arcterm-plugin/src/manifest.rs
  - arcterm-plugin/src/manager.rs
  - arcterm-plugin/src/lib.rs
  - arcterm-app/src/main.rs
  - arcterm-app/Cargo.toml
tdd: true
---

# PLAN-2.1 -- Plugin Manifest, Permission Sandbox, Event Bus, and CLI

## Goal

Build the plugin management layer: TOML manifest parsing with permission enforcement, the event bus connecting terminal lifecycle events to plugin instances, and the CLI commands for installing and developing plugins. After this plan, a developer can write a plugin, declare its permissions in `plugin.toml`, install it via CLI, and have it receive terminal events at runtime.

## Why Wave 2

This plan depends on PLAN-1.1 (Wave 1): it imports `PluginRuntime`, `PluginInstance`, `PluginHostData`, and the WIT-generated types. The `PluginManager` wraps `PluginRuntime` with lifecycle management (install/load/unload/event dispatch). The CLI commands call `PluginManager` methods.

## Design Notes

**Manifest parsing:** The `PluginManifest` struct uses `#[derive(Deserialize)]` with the `toml` crate (already a workspace dependency via `arcterm-app`). The manifest schema matches RESEARCH.md section "Plugin manifest TOML structure". The `api_version` field ("0.1") is checked against a compile-time constant; mismatched versions produce a clear error message.

**Permission enforcement:** The `PluginManifest.permissions` struct is consumed by a `build_wasi_ctx(permissions: &Permissions) -> WasiCtx` function that configures `WasiCtxBuilder`:
- Each `filesystem` path gets `preopened_dir`
- `network = false` means no socket preopening (WASI enforces this at the OS level via cap-std)
- `panes` and `ai` are enforced at the host function level: the `Host` trait implementation in `host.rs` checks `PluginHostData.permissions` before executing `render-text` (requires panes != "none") or `register-mcp-tool` (requires ai = true)

**Event bus:** A single `tokio::sync::broadcast::channel::<PluginEvent>(256)` is owned by `PluginManager`. The sender is cloned and passed to `AppState`. Each `PluginInstance` spawns a `tokio::task` holding a `broadcast::Receiver` that filters events against `subscribed_events` and calls the plugin's `update()` export. If `update()` returns true, the task calls `render()` and swaps the draw buffer.

**Plugin storage:** `dirs::config_dir().join("arcterm/plugins/<name>/")` contains `plugin.toml` and the `.wasm` file. `arcterm plugin install <path>` copies from the source path. `arcterm plugin dev <path>` loads directly from the given path without copying (for rapid iteration).

**CLI integration:** A new `CliCommand::Plugin { subcommand: PluginSubcommand }` variant is added to the existing clap enum in `main.rs`. The `install`, `list`, and `remove` subcommands execute before the event loop (like `List` and `Open`). The `dev` subcommand sets a flag that `AppState` reads during initialization to load the dev plugin.

## Tasks

<task id="1" files="arcterm-plugin/src/manifest.rs, arcterm-plugin/src/lib.rs" tdd="true">
  <action>Create `arcterm-plugin/src/manifest.rs`. Define `PluginManifest` struct with fields: `name: String`, `version: String`, `api_version: String`, `wasm: String` (relative path to .wasm file). Define `Permissions` struct with fields: `filesystem: Vec<String>` (default empty), `network: bool` (default false), `panes: PaneAccess` (default None), `ai: bool` (default false). Define `PaneAccess` enum: None, Read, Write (with serde Deserialize using lowercase strings). Add `PluginManifest::from_toml(content: &str) -> Result<Self, toml::de::Error>` and `PluginManifest::validate(&self) -> Result<(), String>` (checks api_version == "0.1", name is non-empty, wasm path is non-empty). Write `build_wasi_ctx(permissions: &Permissions) -> wasmtime_wasi::WasiCtx` that calls `WasiCtxBuilder` with preopened_dir for each filesystem entry and no socket if network is false. Write tests: (a) parse a valid plugin.toml with all permission fields, (b) parse a minimal plugin.toml with defaults (no permissions section), (c) validate rejects api_version "0.2", (d) validate rejects empty name, (e) build_wasi_ctx with empty filesystem produces a ctx with no preopened dirs. Export from lib.rs.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-plugin manifest -- --nocapture 2>&1 | tail -20</verify>
  <done>All manifest tests pass. `PluginManifest::from_toml` correctly parses all fields with defaults. `validate` rejects invalid manifests. `build_wasi_ctx` produces sandboxed contexts.</done>
</task>

<task id="2" files="arcterm-plugin/src/manager.rs, arcterm-plugin/src/host.rs, arcterm-plugin/src/lib.rs" tdd="true">
  <action>Create `arcterm-plugin/src/manager.rs`. Define `PluginManager` struct with fields: `runtime: PluginRuntime`, `instances: HashMap<PluginId, PluginInstance>`, `event_tx: broadcast::Sender<PluginEvent>`, `plugin_dir: PathBuf` (defaults to `dirs::config_dir()/arcterm/plugins`). Implement methods: `new() -> Self` (creates PluginRuntime, broadcast channel with capacity 256), `install(&self, source_path: &Path) -> Result<PluginId>` (reads plugin.toml from source, validates manifest, copies dir to plugin_dir/<name>/, loads wasm, returns id), `load_from_dir(&mut self, dir: &Path) -> Result<PluginId>` (reads plugin.toml + wasm, builds WasiCtx from permissions via build_wasi_ctx, calls runtime.load_plugin, spawns event listener task, returns id), `unload(&mut self, id: PluginId)` (drops PluginInstance, removes from map), `load_all_installed(&mut self) -> Vec<Result<PluginId>>` (scans plugin_dir for subdirs with plugin.toml, loads each), `list_installed(&self) -> Vec<(PluginId, String, String)>` (returns id, name, version for each loaded plugin), `event_sender(&self) -> broadcast::Sender<PluginEvent>` (clone of event_tx). The event listener task per plugin: receives from broadcast::Receiver, filters against subscribed_events, calls update(), if true calls render(), swaps draw buffer via Arc<Mutex<Vec<StyledLine>>>. Update `host.rs` to gate `render_text` on `permissions.panes != PaneAccess::None` and `register_mcp_tool` on `permissions.ai == true` -- return a WIT error variant if denied. Write tests: (a) install from a temp dir containing valid plugin.toml + dummy wasm, verify files copied to plugin_dir, (b) load_from_dir with a WAT test component, send a PaneOpened event, verify plugin's update() is called (observable via draw buffer change after render). Export from lib.rs.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-plugin manager -- --nocapture 2>&1 | tail -20</verify>
  <done>All manager tests pass. Install copies files correctly. Event dispatch reaches plugin instances. Permission gating blocks unauthorized host calls.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs, arcterm-app/Cargo.toml" tdd="false">
  <action>Add `arcterm-plugin = { path = "../arcterm-plugin" }` to `arcterm-app/Cargo.toml` dependencies. In `main.rs`, add `CliCommand::Plugin` variant with nested `PluginSubcommand` enum: `Install { path: PathBuf }`, `List`, `Remove { name: String }`, `Dev { path: PathBuf }`. Handle `CliCommand::Plugin` in the pre-event-loop match: `Install` calls `PluginManager::new().install(&path)` and prints success/error, `List` calls `PluginManager::new().list_installed()` and prints a table (name, version, status), `Remove` deletes `plugin_dir/<name>/` and prints confirmation, `Dev` stores the path in a variable consumed during AppState initialization. Add `plugin_manager: Option<PluginManager>` field to `AppState`. During `AppState` initialization (in the `Resumed` handler), create `PluginManager`, call `load_all_installed()`, and if a dev plugin path was specified call `load_from_dir(&dev_path)`. Clone `plugin_manager.event_sender()` and store it on AppState. In `spawn_pane`, after pane creation, call `event_tx.send(PluginEvent::PaneOpened { pane_id })`. In the pane close path (channel disconnect in about_to_wait), call `event_tx.send(PluginEvent::PaneClosed { pane_id })`.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app 2>&1 | tail -10 && ./target/debug/arcterm-app plugin list 2>&1</verify>
  <done>`cargo build` succeeds. `arcterm-app plugin list` runs without error (prints empty list or installed plugins). `arcterm-app plugin install --help` shows the path argument. Event emission code compiles in spawn_pane and pane close paths.</done>
</task>
