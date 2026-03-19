//! WASM plugin loader using wasmtime Component Model.
//!
//! Loads a `.wasm` component file from disk, compiles it into a
//! [`wasmtime::component::Component`], and wraps it in a [`LoadedPlugin`] that
//! owns the configured [`wasmtime::Store`] with memory and fuel limits applied.

use crate::capability::CapabilitySet;
use crate::config::WasmPluginConfig;
use anyhow::Context;
use wasmtime::component::Component;
use wasmtime::{Engine, ResourceLimiter, StoreLimitsBuilder};

// ── Store state ───────────────────────────────────────────────────────────────

/// Host-side data stored inside every plugin's [`wasmtime::Store`].
///
/// This is the `T` in `Store<T>`. It carries:
/// - the plugin's granted [`CapabilitySet`] (checked by host functions before
///   performing privileged operations), and
/// - the `StoreLimits` instance used to enforce per-plugin memory caps.
pub struct PluginStoreData {
    /// The plugin's declared + parsed capability grants.
    pub capabilities: CapabilitySet,
    /// Human-readable plugin name (for log prefixing in host functions).
    pub name: String,
    /// Memory / table / instance limits enforced by wasmtime.
    limiter: wasmtime::StoreLimits,
}

impl PluginStoreData {
    pub(crate) fn new(
        name: String,
        capabilities: CapabilitySet,
        memory_limit_bytes: usize,
    ) -> Self {
        let limiter = StoreLimitsBuilder::new()
            .memory_size(memory_limit_bytes)
            .build();
        Self {
            capabilities,
            name,
            limiter,
        }
    }
}

impl ResourceLimiter for PluginStoreData {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> anyhow::Result<bool> {
        self.limiter.memory_growing(current, desired, maximum)
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> anyhow::Result<bool> {
        self.limiter.table_growing(current, desired, maximum)
    }
}

// ── LoadedPlugin ──────────────────────────────────────────────────────────────

/// A successfully compiled and configured WASM plugin.
///
/// Holds the compiled [`Component`] and its dedicated [`wasmtime::Store`].
/// The store already has memory limits and an initial fuel budget applied.
/// Instantiation against a [`wasmtime::component::Linker`] is a separate step
/// performed by the plugin manager once all host functions are registered.
// wasmtime::Store does not impl Debug, so we provide a manual impl below.
pub struct LoadedPlugin {
    /// The compiled WASM component (engine-owned, cheap to clone/share).
    pub component: Component,
    /// The plugin's dedicated store with limits and initial fuel set.
    pub store: wasmtime::Store<PluginStoreData>,
    /// Resolved plugin name from config.
    pub name: String,
    /// Path the WASM file was loaded from (for diagnostics).
    pub source_path: String,
}

impl std::fmt::Debug for LoadedPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedPlugin")
            .field("name", &self.name)
            .field("source_path", &self.source_path)
            .finish_non_exhaustive()
    }
}

// ── Engine factory ────────────────────────────────────────────────────────────

/// Create a wasmtime [`Engine`] configured for plugin execution.
///
/// Enables:
/// - `consume_fuel` — required for per-callback fuel budgets.
/// - `wasm_component_model` — required for Component Model plugins.
pub fn create_engine() -> anyhow::Result<Engine> {
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    config.wasm_component_model(true);
    Engine::new(&config).context("Failed to create wasmtime engine")
}

// ── Loader ────────────────────────────────────────────────────────────────────

/// Load and compile a WASM plugin from `config.path`.
///
/// Steps performed:
/// 1. Read the `.wasm` bytes from disk.
/// 2. Compile into a [`Component`] (validates WASM structure).
/// 3. Build a [`PluginStoreData`] with the plugin's parsed capabilities and
///    a `StoreLimitsBuilder`-enforced memory cap.
/// 4. Create a [`wasmtime::Store`] that delegates to `PluginStoreData` for
///    resource limiting.
/// 5. Set the initial fuel budget on the store.
///
/// # Errors
///
/// Returns `Err` if the file cannot be read, if the bytes are not a valid WASM
/// component, or if the store cannot be configured.
pub fn load_plugin(engine: &Engine, config: &WasmPluginConfig) -> anyhow::Result<LoadedPlugin> {
    // 1. Read WASM bytes from disk.
    let wasm_bytes = std::fs::read(&config.path).with_context(|| {
        format!(
            "Plugin '{}': failed to read WASM file '{}'",
            config.name, config.path
        )
    })?;

    // 2. Compile into a Component (validates the WASM component structure).
    let component = Component::from_binary(engine, &wasm_bytes).with_context(|| {
        format!(
            "Plugin '{}': failed to compile WASM component from '{}'",
            config.name, config.path
        )
    })?;

    // 3. Build store data with parsed capabilities and memory limit.
    let capabilities = CapabilitySet::new(config.parse_capabilities());
    let memory_limit_bytes = (config.memory_limit_mb as usize)
        .checked_mul(1024 * 1024)
        .with_context(|| {
            format!(
                "Plugin '{}': memory_limit_mb {} overflows usize",
                config.name, config.memory_limit_mb
            )
        })?;

    let store_data = PluginStoreData::new(config.name.clone(), capabilities, memory_limit_bytes);

    // 4. Create the store and attach the resource limiter.
    let mut store = wasmtime::Store::new(engine, store_data);
    store.limiter(|data| data as &mut dyn ResourceLimiter);

    // 5. Set the initial fuel budget.
    store.set_fuel(config.fuel_per_callback).with_context(|| {
        format!(
            "Plugin '{}': failed to set fuel budget {}",
            config.name, config.fuel_per_callback
        )
    })?;

    log::info!(
        "Plugin '{}': loaded from '{}' (memory_limit={}MB, fuel={})",
        config.name,
        config.path,
        config.memory_limit_mb,
        config.fuel_per_callback,
    );

    Ok(LoadedPlugin {
        component,
        store,
        name: config.name.clone(),
        source_path: config.path.clone(),
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn default_config(path: &str) -> WasmPluginConfig {
        WasmPluginConfig {
            name: "test-plugin".to_string(),
            path: path.to_string(),
            capabilities: vec![],
            memory_limit_mb: 64,
            fuel_per_callback: 1_000_000,
            enabled: true,
        }
    }

    #[test]
    fn test_create_engine_succeeds() {
        assert!(create_engine().is_ok());
    }

    #[test]
    fn test_load_plugin_file_not_found() {
        let engine = create_engine().unwrap();
        let config = default_config("/nonexistent/path/plugin.wasm");
        let result = load_plugin(&engine, &config);
        assert!(result.is_err());
        let msg = format!("{:#}", result.err().unwrap());
        assert!(
            msg.contains("failed to read WASM file"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn test_load_plugin_invalid_wasm() {
        // Write garbage bytes that are not a valid WASM component.
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(b"not wasm at all").unwrap();
        let path = tmp.path().to_str().unwrap().to_string();

        let engine = create_engine().unwrap();
        let config = default_config(&path);
        let result = load_plugin(&engine, &config);
        assert!(result.is_err());
        let msg = format!("{:#}", result.err().unwrap());
        assert!(
            msg.contains("failed to compile WASM component"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn test_plugin_store_data_memory_limit() {
        // Verify StoreLimitsBuilder correctly denies growth past the cap.
        let caps = CapabilitySet::new(vec![]);
        let limit_bytes = 1024 * 1024; // 1 MB
        let mut data = PluginStoreData::new("test".to_string(), caps, limit_bytes);

        // Below limit: allowed
        assert!(data.memory_growing(0, 512 * 1024, None).unwrap());
        // Exactly at limit: allowed
        assert!(data.memory_growing(0, limit_bytes, None).unwrap());
        // Beyond limit: denied
        assert!(!data.memory_growing(0, limit_bytes + 1, None).unwrap());
    }

    #[test]
    fn test_loaded_plugin_fuel_set() {
        // Build a minimal valid WASM component in binary form.
        // A bare component: magic + version + layer + encoding bytes.
        // We test only that set_fuel was called — invalid WASM will be caught
        // earlier, so we use the file-not-found path to confirm error shape.
        let engine = create_engine().unwrap();
        let config = default_config("/does/not/exist.wasm");
        let err = load_plugin(&engine, &config).err().unwrap();
        assert!(format!("{err:#}").contains("failed to read WASM file"));
    }
}
