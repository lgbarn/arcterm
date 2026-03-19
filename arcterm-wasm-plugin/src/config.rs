//! Plugin configuration types and Lua registration.

use crate::capability::Capability;

/// Configuration for a single WASM plugin, as declared in the user's config.
#[derive(Debug, Clone)]
pub struct WasmPluginConfig {
    /// User-specified plugin name (unique within config)
    pub name: String,
    /// Path to the .wasm file
    pub path: String,
    /// Capability strings to parse (e.g., "fs:read:/home/user")
    pub capabilities: Vec<String>,
    /// Maximum memory in MB (default: 64)
    pub memory_limit_mb: u32,
    /// Fuel budget per callback invocation (default: 1_000_000)
    pub fuel_per_callback: u64,
    /// Whether the plugin is enabled (default: true)
    pub enabled: bool,
}

impl Default for WasmPluginConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            path: String::new(),
            capabilities: vec![],
            memory_limit_mb: 64,
            fuel_per_callback: 1_000_000,
            enabled: true,
        }
    }
}

impl WasmPluginConfig {
    /// Parse capability strings into Capability objects.
    /// Invalid capabilities are logged as warnings and skipped.
    pub fn parse_capabilities(&self) -> Vec<Capability> {
        self.capabilities
            .iter()
            .filter_map(|s| match Capability::parse(s) {
                Ok(cap) => Some(cap),
                Err(e) => {
                    log::warn!("Plugin '{}': invalid capability '{}': {}", self.name, s, e);
                    None
                }
            })
            .collect()
    }
}

// Global storage for plugin registrations from Lua config.
// Populated during config evaluation, consumed during plugin loading.
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref REGISTERED_PLUGINS: Mutex<Vec<WasmPluginConfig>> = Mutex::new(Vec::new());
}

/// Register a plugin from Lua config. Called by wezterm.plugin.register_wasm().
pub fn register_plugin(config: WasmPluginConfig) {
    match REGISTERED_PLUGINS.lock() {
        Ok(mut plugins) => plugins.push(config),
        Err(poisoned) => {
            log::error!(
                "WASM plugin registration failed: mutex poisoned. Plugin '{}' will not load.",
                config.name
            );
            // Recover the poisoned guard to prevent cascading failures
            poisoned.into_inner().push(config);
        }
    }
}

/// Take all registered plugin configs (drains the global list).
pub fn take_registered_plugins() -> Vec<WasmPluginConfig> {
    REGISTERED_PLUGINS
        .lock()
        .map(|mut v| std::mem::take(&mut *v))
        .unwrap_or_default()
}
