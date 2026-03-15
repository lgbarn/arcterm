---
phase: wasm-plugin-system
plan: "1.1"
wave: 1
dependencies: []
must_haves:
  - New workspace crate arcterm-plugin with Cargo.toml depending on wasmtime 42 (component-model, async, cranelift) and wasmtime-wasi 42
  - WIT world definition at arcterm-plugin/wit/arcterm.wit defining the plugin world with host imports (render-text, subscribe-event, get-config, register-mcp-tool, log) and guest exports (load, update, render)
  - WIT types for styled-line (text + fg + bg + bold + italic), event-kind enum, plugin-event variant, tool-schema record
  - wasmtime::component::bindgen! macro invocation generating host-side Rust types and traits
  - PluginHostData struct implementing WasiView (holds WasiCtx + ResourceTable + host state)
  - PluginRuntime struct owning Engine + component Linker, with methods load_plugin(manifest, wasm_bytes) and unload_plugin(id)
  - PluginInstance struct holding Store<PluginHostData> + generated plugin exports handle
  - StoreLimits configured with max_memory_size 10MB per plugin instance
  - Host trait implementation that captures render-text calls into a Vec<StyledLine> draw buffer
  - Unit test loading a minimal WAT component that exports load/update/render and verifying host dispatch round-trips
files_touched:
  - Cargo.toml (workspace members, workspace.dependencies)
  - arcterm-plugin/Cargo.toml
  - arcterm-plugin/src/lib.rs
  - arcterm-plugin/src/runtime.rs
  - arcterm-plugin/src/host.rs
  - arcterm-plugin/src/types.rs
  - arcterm-plugin/wit/arcterm.wit
tdd: true
---

# PLAN-1.1 -- WIT Interface Definition + Wasmtime Host Runtime

## Goal

Create the `arcterm-plugin` crate containing the WIT world definition, wasmtime Component Model host runtime, and WASI capability sandbox infrastructure. This is the foundation every other Phase 6 plan depends on -- without a working host runtime that can load and call a WASM component, nothing else can proceed.

## Why This Must Come First

The WIT file is the contract between arcterm (host) and every plugin (guest). The `PluginRuntime` and `PluginHostData` types are imported by the manifest system (PLAN-2.1), the event bus (PLAN-2.1), the CLI commands (PLAN-2.1), and the rendering integration (PLAN-3.1). Wave 2 and Wave 3 plans cannot begin until this crate compiles and passes its unit test.

## Design Notes

The WIT world follows the Zellij-inspired `load/update/render` lifecycle but uses typed `StyledLine` records instead of raw ANSI output. The `bindgen!` macro generates a `Host` trait that the `PluginHostData` struct implements. Each host import function either mutates the draw buffer (render-text), records event subscriptions (subscribe-event), reads config (get-config), or registers an MCP tool schema (register-mcp-tool).

The `PluginRuntime` is a process-wide singleton holding the wasmtime `Engine` and a pre-configured `component::Linker`. Individual plugin instances get their own `Store<PluginHostData>` with isolated `WasiCtx` (built from manifest permissions in PLAN-2.1; this plan uses a minimal no-capability WasiCtx for testing).

Memory enforcement: `StoreLimits { max_memory_size: Some(10 * 1024 * 1024) }` is set on every `Store` via `store.limiter(|data| &mut data.limits)`. This directly satisfies the "under 10MB per plugin" success criterion.

The unit test compiles a hand-written WAT component (embedded as a string literal) that implements the three guest exports as no-ops or trivial returns. This avoids requiring a full Rust-to-WASM compilation toolchain in CI for Wave 1. The test verifies: component loads, `load()` is callable, `update()` returns a bool, `render()` triggers a `render-text` host import callback, and the draw buffer contains the expected styled line.

## Tasks

<task id="1" files="Cargo.toml, arcterm-plugin/Cargo.toml, arcterm-plugin/wit/arcterm.wit" tdd="false">
  <action>Add `arcterm-plugin` to the workspace members list in root `Cargo.toml`. Add `wasmtime = { version = "42", features = ["component-model", "async", "cranelift"] }` and `wasmtime-wasi = "42"` to `[workspace.dependencies]`. Create `arcterm-plugin/Cargo.toml` with dependencies on wasmtime, wasmtime-wasi, tokio, serde, toml, serde_json, and log from workspace. Create `arcterm-plugin/wit/arcterm.wit` defining package `arcterm:plugin@0.1.0` with the full world: host interface imports (`render-text`, `subscribe-event`, `get-config`, `register-mcp-tool`, `log`) and guest exports (`load`, `update`, `render`). Define WIT types: `styled-line` record (text: string, fg: option<color>, bg: option<color>, bold: bool, italic: bool), `color` record (r: u8, g: u8, b: u8), `event-kind` enum (pane-opened, pane-closed, command-executed, workspace-switched), `plugin-event` variant matching event-kind with payload records, `tool-schema` record (name: string, description: string, input-schema: string).</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo check -p arcterm-plugin 2>&1 | tail -5</verify>
  <done>`cargo check -p arcterm-plugin` succeeds. The `arcterm.wit` file exists at `arcterm-plugin/wit/arcterm.wit` and contains all specified types and functions.</done>
</task>

<task id="2" files="arcterm-plugin/src/lib.rs, arcterm-plugin/src/runtime.rs, arcterm-plugin/src/host.rs, arcterm-plugin/src/types.rs" tdd="false">
  <action>Create `arcterm-plugin/src/types.rs` with re-exported Rust types mirroring WIT: `StyledLine`, `Color`, `EventKind`, `PluginEvent`, `ToolSchema`, and a `PluginId` newtype. Create `arcterm-plugin/src/host.rs` with `PluginHostData` struct containing fields: `wasi_ctx: WasiCtx`, `resource_table: ResourceTable`, `limits: StoreLimits`, `draw_buffer: Vec<StyledLine>`, `subscribed_events: Vec<EventKind>`, `registered_tools: Vec<ToolSchema>`, `config: HashMap<String, String>`. Implement `WasiView` for `PluginHostData`. Invoke `wasmtime::component::bindgen!` pointing to `wit/arcterm.wit`. Implement the generated `Host` trait on `PluginHostData`: `render_text` appends to draw_buffer, `subscribe_event` appends to subscribed_events, `get_config` looks up config HashMap, `register_mcp_tool` appends to registered_tools, `log` calls `log::info!`. Create `arcterm-plugin/src/runtime.rs` with `PluginRuntime` struct (engine: Engine, linker: component::Linker<PluginHostData>) and `PluginInstance` struct (store: Store<PluginHostData>, instance handle from bindgen). `PluginRuntime::new()` creates Engine with default Config + epoch_interruption enabled, builds Linker, adds WASI to linker, adds host functions to linker. `PluginRuntime::load_plugin(&self, wasm_bytes, config)` compiles Component, creates Store with StoreLimits(max_memory_size=10MB), instantiates, calls load(), returns PluginInstance. Wire everything in `arcterm-plugin/src/lib.rs` as `pub mod runtime; pub mod host; pub mod types;`.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo check -p arcterm-plugin 2>&1 | tail -5</verify>
  <done>`cargo check -p arcterm-plugin` succeeds with no errors. `PluginRuntime::new()` and `PluginRuntime::load_plugin()` are public. `PluginHostData` implements `WasiView`. StoreLimits enforces 10MB max memory.</done>
</task>

<task id="3" files="arcterm-plugin/src/runtime.rs" tdd="true">
  <action>Write an integration test in `arcterm-plugin/src/runtime.rs` (or `arcterm-plugin/tests/runtime_test.rs`) that: (a) constructs a minimal WASM Component from WAT source using `wasmtime::component::Component::new(&engine, wat_source)` -- the WAT defines a component that implements the three guest exports (load as no-op, update returning true, render calling the host render-text import with a single styled line "Hello from plugin"), (b) creates a PluginRuntime, (c) calls load_plugin with the WAT bytes and a test config map, (d) asserts load_plugin completes in under 50ms (measured with std::time::Instant), (e) calls update() on the instance and asserts it returns true, (f) calls render() and asserts the draw buffer contains exactly one StyledLine with text "Hello from plugin". Note: if constructing a valid Component Model WAT is impractical (the WAT encoding for components is verbose), use `wasm-tools` crate or `wasm-encoder` to programmatically build a minimal component binary in the test setup. Add `wasm-encoder` and `wat` as dev-dependencies if needed.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-plugin -- --nocapture 2>&1 | tail -20</verify>
  <done>All tests pass. The test proves: component loads in under 50ms, update() returns true, render() populates the draw buffer with the expected styled line. Test output shows timing measurement.</done>
</task>
