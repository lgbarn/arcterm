use std::collections::HashMap;

use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::p2::add_to_linker_sync;

use crate::host::{ArctermPlugin, PluginHostData};
use crate::host::arcterm::plugin::types::{PluginEvent as WitPluginEvent, StyledLine};

/// Process-wide singleton: owns the Engine and a pre-configured Linker.
pub struct PluginRuntime {
    engine: Engine,
    linker: Linker<PluginHostData>,
}

impl PluginRuntime {
    /// Create a new `PluginRuntime` with the Component Model enabled.
    pub fn new() -> anyhow::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.epoch_interruption(true);

        let engine = Engine::new(&config)?;

        // Spawn the epoch ticker: increments the engine epoch every 10ms so that
        // epoch deadlines fire. Without ticking, epoch_interruption(true) has no effect.
        // Uses a plain OS thread so PluginRuntime::new() works in both sync and async contexts.
        let engine_clone = engine.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(10));
            engine_clone.increment_epoch();
        });

        let mut linker: Linker<PluginHostData> = Linker::new(&engine);

        // Add WASI p2 (synchronous) host functions to the linker.
        add_to_linker_sync(&mut linker)?;

        // Add our custom host import functions to the linker.
        ArctermPlugin::add_to_linker::<_, HasSelf<_>>(&mut linker, |data| data)?;

        Ok(Self { engine, linker })
    }

    /// Compile a WASM component from raw bytes and return a `PluginInstance`.
    ///
    /// `wasm_bytes` may be either the binary `.wasm` format or WAT text.
    pub fn load_plugin(
        &self,
        wasm_bytes: &[u8],
        config: HashMap<String, String>,
    ) -> anyhow::Result<PluginInstance> {
        let component = Component::new(&self.engine, wasm_bytes)?;

        let host_data = PluginHostData::new(config);
        let mut store = Store::new(&self.engine, host_data);

        // Enforce the 10 MB memory limit on this store.
        store.limiter(|data| &mut data.limits);

        let instance = ArctermPlugin::instantiate(&mut store, &component, &self.linker)?;

        // 3000 epochs at 10ms tick interval = 30-second deadline.
        store.set_epoch_deadline(3000);
        instance.call_load(&mut store)?;

        Ok(PluginInstance { store, instance })
    }

    /// Compile a WASM component from raw bytes with a caller-supplied [`WasiCtx`] and permissions.
    ///
    /// This is the manifest-aware variant called by `PluginManager::load_from_dir`.
    /// The `wasi_ctx` is produced by `build_wasi_ctx(&manifest.permissions)` and
    /// enforces the filesystem/network sandbox declared in `plugin.toml`.
    /// The `permissions` are stored in `PluginHostData` to gate host function calls.
    pub fn load_plugin_with_wasi(
        &self,
        wasm_bytes: &[u8],
        config: HashMap<String, String>,
        wasi_ctx: wasmtime_wasi::WasiCtx,
        permissions: crate::manifest::Permissions,
    ) -> anyhow::Result<PluginInstance> {
        let component = Component::new(&self.engine, wasm_bytes)?;

        let host_data = PluginHostData::new_with_wasi(config, wasi_ctx, permissions);
        let mut store = Store::new(&self.engine, host_data);

        store.limiter(|data| &mut data.limits);

        let instance = ArctermPlugin::instantiate(&mut store, &component, &self.linker)?;

        // 3000 epochs at 10ms tick interval = 30-second deadline.
        store.set_epoch_deadline(3000);
        instance.call_load(&mut store)?;

        Ok(PluginInstance { store, instance })
    }

    /// Expose the underlying engine (useful for tests that need to compile WAT).
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

/// A single running plugin instance.
pub struct PluginInstance {
    store: Store<PluginHostData>,
    instance: ArctermPlugin,
}

impl PluginInstance {
    /// Deliver an event to the plugin. Returns `true` if the plugin consumed it.
    pub fn call_update(&mut self, event: WitPluginEvent) -> anyhow::Result<bool> {
        // 3000 epochs at 10ms tick interval = 30-second deadline.
        self.store.set_epoch_deadline(3000);
        let result = self.instance.call_update(&mut self.store, &event)?;
        Ok(result)
    }

    /// Ask the plugin to render its current state. The `render-text` calls land
    /// in the draw buffer via the host import, so we return the draw buffer contents.
    /// Clears the draw buffer before calling render.
    pub fn call_render(&mut self) -> anyhow::Result<Vec<StyledLine>> {
        // 3000 epochs at 10ms tick interval = 30-second deadline.
        self.store.set_epoch_deadline(3000);
        self.store.data_mut().draw_buffer.clear();
        self.instance.call_render(&mut self.store)?;
        Ok(self.store.data().draw_buffer.clone())
    }

    /// Dispatch a tool call to the WASM plugin's `call-tool` export.
    pub fn call_tool_export(&mut self, name: &str, args_json: &str) -> anyhow::Result<String> {
        // 3000 epochs at 10ms tick interval = 30-second deadline.
        self.store.set_epoch_deadline(3000);
        let result = self.instance.call_call_tool(&mut self.store, name, args_json)?;
        Ok(result)
    }

    /// Read-only access to the host data (for inspection in tests).
    pub fn host_data(&self) -> &PluginHostData {
        self.store.data()
    }
}
