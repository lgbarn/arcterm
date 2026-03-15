---
phase: wasm-plugin-system
plan: "3.1"
wave: 3
dependencies: ["1.1", "2.1"]
must_haves:
  - PaneNode::PluginPane variant integrating plugin panes into the layout engine
  - Plugin draw buffer rendered via existing TextRenderer.prepare_overlay_text or a new prepare_plugin_pane method
  - Keyboard input forwarded to focused plugin pane via a WIT key-input host import or update() event
  - Hello-world example plugin (Rust, compiles to wasm32-wasip2) that renders "Hello from WASM plugin" and responds to key presses
  - System monitor example plugin demonstrating full API surface (events, config, render, MCP tool registration)
  - MCP tool registry accessible via PluginManager with list_tools() method returning JSON-compatible tool schemas
  - Both example plugins load in under 50ms and use under 10MB memory
files_touched:
  - arcterm-app/src/layout.rs
  - arcterm-app/src/main.rs
  - arcterm-render/src/text.rs
  - arcterm-plugin/src/manager.rs
  - arcterm-plugin/wit/arcterm.wit
  - examples/plugins/hello-world/Cargo.toml
  - examples/plugins/hello-world/src/lib.rs
  - examples/plugins/hello-world/plugin.toml
  - examples/plugins/system-monitor/Cargo.toml
  - examples/plugins/system-monitor/src/lib.rs
  - examples/plugins/system-monitor/plugin.toml
tdd: false
---

# PLAN-3.1 -- Plugin Rendering Integration, Keyboard Input, and Example Plugins

## Goal

Wire plugin panes into the rendering pipeline and input handling so that WASM plugins are visible, interactive panes in the terminal. Build two example plugins that prove the full API surface works end-to-end: a hello-world plugin (success criterion 1) and a system monitor plugin (success criterion 5). Register MCP tool schemas from the system monitor plugin to satisfy the MCP basics requirement.

## Why Wave 3

This plan depends on both Wave 1 (the WIT interface and host runtime) and Wave 2 (the PluginManager, manifest system, event bus, and CLI). It is pure integration and end-to-end validation -- no new foundational types are introduced.

## Design Notes

**PaneNode extension:** A new `PaneNode::PluginPane { pane_id: PaneId, plugin_id: PluginId }` variant is added to the existing enum in `layout.rs`. The layout engine (`compute_rects`, `collect_pane_ids`, `find_neighbor`, etc.) treats PluginPane identically to Leaf for geometry and navigation purposes. The difference is in the render path: instead of reading a `Terminal` grid, the renderer reads the plugin's draw buffer.

**Rendering:** The plugin's draw buffer (`Arc<Mutex<Vec<StyledLine>>>`) is read during the `RedrawRequested` handler. For each PluginPane in the current tab's layout, the code locks the draw buffer, converts each `StyledLine` into glyphon `BufferLine` entries, and calls `TextRenderer::prepare_plugin_pane(pane_rect, styled_lines)` -- a new method similar to `prepare_overlay_text` but positioned within a pane rect and supporting fg/bg colors and bold/italic attributes from the StyledLine records.

**Keyboard input:** The WIT world in PLAN-1.1 defines `plugin-event` with a `key-input` variant. When the focused pane is a PluginPane, the `KeyboardInput` handler in `main.rs` translates the key event to a `PluginEvent::KeyInput { key, modifiers }` and sends it directly to the plugin's update() export (via PluginManager). If update() returns true, render() is called immediately.

**Hello-world plugin:** A minimal Rust crate at `examples/plugins/hello-world/` using `wit-bindgen::generate!` (guest side). The `load()` function stores the config. The `render()` function calls `host::render_text` with a single StyledLine: "Hello from WASM plugin! Press any key..." in green text. The `update()` function handles KeyInput events by appending the key character to a display string and returning true (triggering re-render).

**System monitor plugin:** A Rust crate at `examples/plugins/system-monitor/` that demonstrates the full API. On `load()`, it calls `subscribe_event(CommandExecuted)` and `register_mcp_tool` with a "get-system-info" tool schema. On `render()`, it displays system info lines (hostname, OS, uptime -- obtained via WASI filesystem reads of `/proc/` or equivalent). On `update(CommandExecuted)`, it refreshes its display. The plugin.toml declares `filesystem = ["/proc"]` (Linux) or appropriate paths, `panes = "read"`, `ai = true`.

**MCP registry:** `PluginManager` exposes `list_tools() -> Vec<ToolSchema>` that collects all `registered_tools` from all loaded plugin instances. This returns tool schemas in a format compatible with MCP `tools/list` response (name, description, inputSchema as JSON string). Full MCP JSON-RPC serving is deferred to Phase 7.

## Tasks

<task id="1" files="arcterm-app/src/layout.rs, arcterm-app/src/main.rs, arcterm-render/src/text.rs, arcterm-plugin/src/manager.rs, arcterm-plugin/wit/arcterm.wit" tdd="false">
  <action>Add `PluginPane { pane_id: PaneId, plugin_id: PluginId }` variant to `PaneNode` enum in `layout.rs`. Update all match arms on PaneNode throughout layout.rs to handle PluginPane identically to Leaf (geometry, collect_pane_ids, find_neighbor, zoom). Import PluginId type from arcterm-plugin. Add `prepare_plugin_pane(&mut self, rect: PixelRect, lines: &[StyledLine], cell_width: f32, cell_height: f32)` method to `TextRenderer` in `arcterm-render/src/text.rs` -- this method creates a glyphon Buffer per line, positions it within the pane rect using the same cell grid math as prepare_grid_at, and applies fg/bg/bold/italic from each StyledLine. In the `RedrawRequested` handler in `main.rs`, after the existing terminal pane rendering loop, add a second loop over PluginPane nodes: for each, read the draw buffer from plugin_manager, call prepare_plugin_pane. Add a `key-input` case to the WIT `plugin-event` variant in arcterm.wit (fields: key-char as option<char>, key-name as string, ctrl: bool, alt: bool, shift: bool). In the `KeyboardInput` handler in `main.rs`, when the focused pane is a PluginPane, translate the key event to PluginEvent::KeyInput and send to the plugin via PluginManager. Add `take_draw_buffer(id: PluginId) -> Vec<StyledLine>` method to PluginManager. Add `list_tools() -> Vec<ToolSchema>` method to PluginManager that collects registered_tools from all instances.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app 2>&1 | tail -10</verify>
  <done>`cargo build` succeeds with the PluginPane variant handled in all PaneNode match arms. TextRenderer has prepare_plugin_pane method. Keyboard input routes to plugin panes. MCP list_tools compiles.</done>
</task>

<task id="2" files="examples/plugins/hello-world/Cargo.toml, examples/plugins/hello-world/src/lib.rs, examples/plugins/hello-world/plugin.toml" tdd="false">
  <action>Create `examples/plugins/hello-world/` directory. Create `Cargo.toml` with: `[package] name = "hello-world-plugin"`, edition 2024, `[lib] crate-type = ["cdylib"]`, dependencies `wit-bindgen = "0.53"`. Create `src/lib.rs` that calls `wit_bindgen::generate!` pointing to the arcterm WIT file (use a relative path `../../../arcterm-plugin/wit/`). Implement the guest trait: `load()` stores config in a static `OnceLock` or thread-local, `render(rows, cols)` calls `host::render_text` with styled lines -- line 1: "Hello from WASM plugin!" (green fg, bold), line 2: shows the last key pressed (updated by update), remaining lines blank. `update(event)` handles KeyInput by recording the key character in a static mut or RefCell, returns true for KeyInput events, false otherwise. Create `plugin.toml` with: `name = "hello-world"`, `version = "0.1.0"`, `api_version = "0.1"`, `wasm = "hello_world_plugin.wasm"`, `[permissions]` section with all defaults (no filesystem, no network, panes = "none", ai = false). Add a note in the Cargo.toml or a README that building requires: `cargo build --target wasm32-wasip2 --release`.</action>
  <verify>cd /Users/lgbarn/Personal/myterm/examples/plugins/hello-world && cargo build --target wasm32-wasip2 --release 2>&1 | tail -10</verify>
  <done>Hello-world plugin compiles to a .wasm component. The plugin.toml is valid. The wasm file is under 1MB. Loading it via `arcterm-app plugin dev ./examples/plugins/hello-world` shows "Hello from WASM plugin!" in a pane and responds to key presses.</done>
</task>

<task id="3" files="examples/plugins/system-monitor/Cargo.toml, examples/plugins/system-monitor/src/lib.rs, examples/plugins/system-monitor/plugin.toml" tdd="false">
  <action>Create `examples/plugins/system-monitor/` directory. Create `Cargo.toml` with: `[package] name = "system-monitor-plugin"`, edition 2024, `[lib] crate-type = ["cdylib"]`, dependencies `wit-bindgen = "0.53"`. Create `src/lib.rs` implementing the guest trait: `load()` calls `host::subscribe_event(EventKind::CommandExecuted)`, calls `host::register_mcp_tool` with a tool schema for "get-system-info" (description: "Returns current system information", input_schema: `{"type":"object","properties":{}}`), and stores config. `render(rows, cols)` outputs styled lines: title "System Monitor" (bold, cyan fg), separator line, hostname line, current working directory from config, uptime or timestamp (read via WASI clock or a simple counter), a "Commands executed: N" counter incremented on each CommandExecuted event, and a "Tools registered: 1" line. `update(event)` increments the command counter on CommandExecuted events, returns true. Create `plugin.toml` with: `name = "system-monitor"`, `version = "0.1.0"`, `api_version = "0.1"`, `wasm = "system_monitor_plugin.wasm"`, `[permissions]` with `panes = "read"`, `ai = true`, filesystem and network defaults. Verify it builds with `cargo build --target wasm32-wasip2 --release`.</action>
  <verify>cd /Users/lgbarn/Personal/myterm/examples/plugins/system-monitor && cargo build --target wasm32-wasip2 --release 2>&1 | tail -10</verify>
  <done>System monitor plugin compiles to .wasm. It exercises the full API: event subscription (CommandExecuted), MCP tool registration (get-system-info), multi-line styled rendering, and config access. Loading via `arcterm-app plugin dev ./examples/plugins/system-monitor` shows the monitor dashboard, updates on command execution, and `PluginManager::list_tools()` returns the registered tool schema.</done>
</task>
