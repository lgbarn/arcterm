use std::collections::HashMap;

use wasmtime::component::ResourceTable;
use wasmtime::StoreLimitsBuilder;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

// Generate the host-side bindings from the WIT file.
// The macro emits the following in the current module scope:
//   - `ArctermPlugin` struct (world name -> Arcterm prefix)
//   - `ArctermPluginImports` trait (for flat world imports)
//   - Sub-modules `arcterm::plugin::types` with generated types
//   - `plugin::types::Host` trait (for the `types` interface, which has no functions but bindgen still generates it)
wasmtime::component::bindgen!({
    path: "wit/arcterm.wit",
    world: "arcterm-plugin",
});

/// Host-side state threaded through every plugin `Store<T>`.
pub struct PluginHostData {
    /// WASI context for this plugin instance.
    pub(crate) wasi_ctx: WasiCtx,
    /// WASI resource table (required by WasiView).
    pub(crate) resource_table: ResourceTable,
    /// Memory growth limiter (enforces 10 MB cap).
    pub(crate) limits: wasmtime::StoreLimits,
    /// Lines rendered by the plugin since last flush.
    pub draw_buffer: Vec<arcterm::plugin::types::StyledLine>,
    /// Event kinds the plugin has subscribed to.
    pub subscribed_events: Vec<arcterm::plugin::types::EventKind>,
    /// MCP tool schemas the plugin has registered.
    pub registered_tools: Vec<arcterm::plugin::types::ToolSchema>,
    /// Static config key→value map provided at load time.
    pub config: HashMap<String, String>,
}

impl PluginHostData {
    /// Construct a new `PluginHostData` with the given config and a minimal
    /// WASI context (no filesystem, no network).
    pub fn new(config: HashMap<String, String>) -> Self {
        Self {
            wasi_ctx: WasiCtxBuilder::new().build(),
            resource_table: ResourceTable::new(),
            limits: StoreLimitsBuilder::new()
                .memory_size(10 * 1024 * 1024) // 10 MB
                .build(),
            draw_buffer: Vec::new(),
            subscribed_events: Vec::new(),
            registered_tools: Vec::new(),
            config,
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// WasiView implementation
// ──────────────────────────────────────────────────────────────────

impl WasiView for PluginHostData {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.resource_table,
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// `types` interface Host impl
//
// The `types` WIT interface has no functions, only type definitions, but
// bindgen! still generates an empty `Host` trait for it. We implement it.
// ──────────────────────────────────────────────────────────────────

impl arcterm::plugin::types::Host for PluginHostData {}

// ──────────────────────────────────────────────────────────────────
// Host import implementations
//
// For a flat world with direct function imports, bindgen! generates a trait
// named after the world: `ArctermPluginImports`.
// Infallible functions return () directly; no Result wrapping.
// ──────────────────────────────────────────────────────────────────

impl ArctermPluginImports for PluginHostData {
    fn log(&mut self, msg: String) {
        log::info!("[plugin] {}", msg);
    }

    fn render_text(&mut self, line: arcterm::plugin::types::StyledLine) {
        self.draw_buffer.push(line);
    }

    fn subscribe_event(&mut self, kind: arcterm::plugin::types::EventKind) {
        self.subscribed_events.push(kind);
    }

    fn get_config(&mut self, key: String) -> Option<String> {
        self.config.get(&key).cloned()
    }

    fn register_mcp_tool(&mut self, schema: arcterm::plugin::types::ToolSchema) {
        self.registered_tools.push(schema);
    }
}
