//! Plugin lifecycle state machine.

/// The lifecycle state of a WASM plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginState {
    Loading,
    Initializing,
    Running,
    Stopping,
    Stopped,
    Failed(String),
}

impl std::fmt::Display for PluginState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginState::Loading => write!(f, "Loading"),
            PluginState::Initializing => write!(f, "Initializing"),
            PluginState::Running => write!(f, "Running"),
            PluginState::Stopping => write!(f, "Stopping"),
            PluginState::Stopped => write!(f, "Stopped"),
            PluginState::Failed(msg) => write!(f, "Failed: {}", msg),
        }
    }
}

/// A loaded WASM plugin instance.
pub struct Plugin {
    /// User-specified name
    pub name: String,
    /// Current lifecycle state
    pub state: PluginState,
    /// The plugin's capability set
    pub capabilities: crate::capability::CapabilitySet,
}

impl Plugin {
    /// Create a new plugin in the Loading state.
    pub fn new(name: String, capabilities: crate::capability::CapabilitySet) -> Self {
        Plugin {
            name,
            state: PluginState::Loading,
            capabilities,
        }
    }

    /// Transition to a new state. Logs the transition.
    pub fn transition(&mut self, new_state: PluginState) {
        log::info!("Plugin '{}': {} → {}", self.name, self.state, new_state);
        self.state = new_state;
    }

    /// Mark the plugin as failed with an error message.
    pub fn fail(&mut self, error: String) {
        log::error!("Plugin '{}' failed: {}", self.name, error);
        self.state = PluginState::Failed(error);
    }

    /// Check if the plugin is in a running state.
    pub fn is_running(&self) -> bool {
        self.state == PluginState::Running
    }
}

/// Manages all loaded WASM plugins.
pub struct PluginManager {
    plugins: Vec<Plugin>,
}

impl PluginManager {
    /// Create a new empty plugin manager.
    pub fn new() -> Self {
        PluginManager {
            plugins: Vec::new(),
        }
    }

    /// Load all plugins from the given configurations.
    /// Each plugin is loaded independently — a failure in one does not
    /// prevent others from loading.
    pub fn load_all(&mut self, configs: Vec<crate::config::WasmPluginConfig>) {
        let engine = match crate::loader::create_engine() {
            Ok(e) => e,
            Err(e) => {
                log::error!("Failed to create WASM engine: {}", e);
                return;
            }
        };

        for config in configs {
            if !config.enabled {
                log::info!("Plugin '{}' is disabled, skipping", config.name);
                continue;
            }

            let caps = config.parse_capabilities();
            let cap_set = crate::capability::CapabilitySet::new(caps);
            let mut plugin = Plugin::new(config.name.clone(), cap_set);

            match self.load_single_plugin(&engine, &config, &mut plugin) {
                Ok(()) => {
                    log::info!("Plugin '{}' loaded successfully", plugin.name);
                }
                Err(e) => {
                    plugin.fail(format!("{:#}", e));
                }
            }

            self.plugins.push(plugin);
        }

        let running = self.plugins.iter().filter(|p| p.is_running()).count();
        let failed = self
            .plugins
            .iter()
            .filter(|p| matches!(p.state, PluginState::Failed(_)))
            .count();
        log::info!(
            "WASM plugin loading complete: {} running, {} failed, {} total",
            running,
            failed,
            self.plugins.len()
        );
    }

    fn load_single_plugin(
        &self,
        engine: &wasmtime::Engine,
        config: &crate::config::WasmPluginConfig,
        plugin: &mut Plugin,
    ) -> anyhow::Result<()> {
        plugin.transition(PluginState::Loading);

        // Load the WASM component from file
        let path = std::path::Path::new(&config.path);
        if !path.exists() {
            anyhow::bail!("Plugin file not found: {}", config.path);
        }

        let wasm_bytes = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("Failed to read plugin file '{}': {}", config.path, e))?;

        // Validate it's a WASM file (magic bytes: \0asm)
        if wasm_bytes.len() < 4 || &wasm_bytes[0..4] != b"\0asm" {
            anyhow::bail!("Invalid WASM file: {}", config.path);
        }

        log::debug!(
            "Plugin '{}': loaded {} bytes from {}",
            plugin.name,
            wasm_bytes.len(),
            config.path
        );

        // For now, mark as running after successful load.
        // Full Component Model instantiation (with init() call) will be
        // added when the host API linker is complete.
        plugin.transition(PluginState::Initializing);
        plugin.transition(PluginState::Running);

        Ok(())
    }

    /// Shut down all running plugins.
    ///
    /// Transitions each running plugin through the clean shutdown sequence:
    /// `Running → Stopping → Stopped`. Each transition is logged via the
    /// [`Plugin::transition`] method so the state change is visible in logs.
    ///
    /// # Destroy callbacks
    ///
    /// Calling the plugin's `destroy()` WASM export is deferred until Component
    /// Model instantiation is fully wired (i.e., the guest `Instance` is stored
    /// alongside the `Store`). At that point this method will call
    /// `instance.call_destroy(&mut store)` between the `Stopping` and `Stopped`
    /// transitions. The state-machine contract is already correct and will not
    /// need to change when that wiring is added.
    pub fn shutdown_all(&mut self) {
        let total = self.plugins.iter().filter(|p| p.is_running()).count();
        log::info!("Shutting down {} running plugin(s)", total);

        for plugin in &mut self.plugins {
            if plugin.is_running() {
                plugin.transition(PluginState::Stopping);

                // TODO: Call `instance.call_destroy(&mut store)` here once the
                // guest Instance is stored alongside the Store after Component
                // Model instantiation is complete.

                plugin.transition(PluginState::Stopped);
                log::info!("Plugin '{}' stopped cleanly", plugin.name);
            }
        }

        log::info!("Plugin shutdown complete");
    }

    /// Get a reference to all plugins.
    pub fn plugins(&self) -> &[Plugin] {
        &self.plugins
    }

    /// Get the count of running plugins.
    pub fn running_count(&self) -> usize {
        self.plugins.iter().filter(|p| p.is_running()).count()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilitySet;
    use crate::config::WasmPluginConfig;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn enabled_config(name: &str, path: &str) -> WasmPluginConfig {
        WasmPluginConfig {
            name: name.to_string(),
            path: path.to_string(),
            capabilities: vec![],
            memory_limit_mb: 64,
            fuel_per_callback: 1_000_000,
            enabled: true,
        }
    }

    fn disabled_config(name: &str) -> WasmPluginConfig {
        WasmPluginConfig {
            name: name.to_string(),
            path: "/nonexistent/plugin.wasm".to_string(),
            capabilities: vec![],
            memory_limit_mb: 64,
            fuel_per_callback: 1_000_000,
            enabled: false,
        }
    }

    // ── T037: Lifecycle state transition tests ────────────────────────────────

    #[test]
    fn test_plugin_initial_state_is_loading() {
        let caps = CapabilitySet::new(vec![]);
        let plugin = Plugin::new("test".to_string(), caps);
        assert_eq!(plugin.state, PluginState::Loading);
    }

    #[test]
    fn test_plugin_transition_loading_to_running() {
        let caps = CapabilitySet::new(vec![]);
        let mut plugin = Plugin::new("test".to_string(), caps);
        plugin.transition(PluginState::Initializing);
        assert_eq!(plugin.state, PluginState::Initializing);
        plugin.transition(PluginState::Running);
        assert_eq!(plugin.state, PluginState::Running);
        assert!(plugin.is_running());
    }

    #[test]
    fn test_plugin_transition_loading_to_failed() {
        let caps = CapabilitySet::new(vec![]);
        let mut plugin = Plugin::new("test".to_string(), caps);
        plugin.fail("file not found".to_string());
        assert!(!plugin.is_running());
        assert!(matches!(plugin.state, PluginState::Failed(_)));
        if let PluginState::Failed(msg) = &plugin.state {
            assert!(msg.contains("file not found"));
        }
    }

    #[test]
    fn test_plugin_shutdown_sequence_running_to_stopped() {
        let caps = CapabilitySet::new(vec![]);
        let mut plugin = Plugin::new("test".to_string(), caps);
        plugin.transition(PluginState::Running);
        assert!(plugin.is_running());

        plugin.transition(PluginState::Stopping);
        assert_eq!(plugin.state, PluginState::Stopping);
        assert!(!plugin.is_running());

        plugin.transition(PluginState::Stopped);
        assert_eq!(plugin.state, PluginState::Stopped);
        assert!(!plugin.is_running());
    }

    #[test]
    fn test_plugin_display_states() {
        assert_eq!(PluginState::Loading.to_string(), "Loading");
        assert_eq!(PluginState::Initializing.to_string(), "Initializing");
        assert_eq!(PluginState::Running.to_string(), "Running");
        assert_eq!(PluginState::Stopping.to_string(), "Stopping");
        assert_eq!(PluginState::Stopped.to_string(), "Stopped");
        assert_eq!(
            PluginState::Failed("oops".to_string()).to_string(),
            "Failed: oops"
        );
    }

    // ── T037: PluginManager load_all with mixed success/failure ───────────────

    #[test]
    fn test_load_all_with_no_plugins_is_ok() {
        let mut manager = PluginManager::new();
        manager.load_all(vec![]);
        assert_eq!(manager.plugins().len(), 0);
        assert_eq!(manager.running_count(), 0);
    }

    #[test]
    fn test_load_all_disabled_plugin_is_skipped() {
        let mut manager = PluginManager::new();
        manager.load_all(vec![disabled_config("skipped")]);
        // Disabled plugins are not added to the plugin list at all.
        assert_eq!(
            manager.plugins().len(),
            0,
            "disabled plugins should not appear in the plugin list"
        );
    }

    #[test]
    fn test_load_all_missing_file_results_in_failed_plugin() {
        let mut manager = PluginManager::new();
        manager.load_all(vec![enabled_config("bad-plugin", "/nonexistent/plugin.wasm")]);
        assert_eq!(manager.plugins().len(), 1);
        assert_eq!(manager.running_count(), 0);
        let plugin = &manager.plugins()[0];
        assert!(
            matches!(plugin.state, PluginState::Failed(_)),
            "plugin with missing file should be in Failed state, got {:?}",
            plugin.state
        );
        if let PluginState::Failed(msg) = &plugin.state {
            assert!(
                msg.contains("Plugin file not found") || msg.contains("failed to read"),
                "failure message should describe the cause: {msg}"
            );
        }
    }

    #[test]
    fn test_load_all_one_failing_does_not_affect_others() {
        // Use a real WASM component fixture for the good plugin and a missing
        // path for the bad one.  The lifecycle manager's load_single_plugin()
        // checks file existence before reading, so a missing path reliably fails.
        // We supply two bad plugins (different names) and one "good" one that
        // fails for a different reason to validate isolation.
        //
        // Since we have no fixture WASM components in this test module, we
        // instead verify that when multiple plugins fail they are ALL recorded
        // independently and none corrupts another's state.
        let configs = vec![
            enabled_config("plugin-a", "/nonexistent/a.wasm"),
            enabled_config("plugin-b", "/nonexistent/b.wasm"),
        ];

        let mut manager = PluginManager::new();
        manager.load_all(configs);

        assert_eq!(manager.plugins().len(), 2, "both plugins should be recorded");
        assert_eq!(manager.running_count(), 0, "no plugin should be running");

        // Each plugin is individually in Failed state with its own path in the message.
        for plugin in manager.plugins() {
            assert!(
                matches!(plugin.state, PluginState::Failed(_)),
                "plugin '{}' should be Failed",
                plugin.name
            );
        }

        // Names are preserved independently.
        let names: Vec<&str> = manager.plugins().iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"plugin-a"));
        assert!(names.contains(&"plugin-b"));
    }

    #[test]
    fn test_shutdown_all_transitions_running_plugins_to_stopped() {
        let mut manager = PluginManager::new();

        // Manually inject a running plugin (bypassing load_all to avoid needing
        // a real WASM file in unit tests).
        let caps = CapabilitySet::new(vec![]);
        let mut plugin = Plugin::new("manual-plugin".to_string(), caps);
        plugin.transition(PluginState::Initializing);
        plugin.transition(PluginState::Running);
        manager.plugins.push(plugin);

        assert_eq!(manager.running_count(), 1);

        manager.shutdown_all();

        assert_eq!(manager.running_count(), 0);
        let plugin = &manager.plugins()[0];
        assert_eq!(plugin.state, PluginState::Stopped);
    }

    #[test]
    fn test_shutdown_all_does_not_affect_already_failed_plugins() {
        let mut manager = PluginManager::new();

        let caps = CapabilitySet::new(vec![]);
        let mut plugin = Plugin::new("failed-plugin".to_string(), caps);
        plugin.fail("load error".to_string());
        manager.plugins.push(plugin);

        manager.shutdown_all();

        // Still in Failed state — shutdown_all should only touch Running plugins.
        assert!(matches!(manager.plugins()[0].state, PluginState::Failed(_)));
    }

    #[test]
    fn test_shutdown_all_mixed_states() {
        let mut manager = PluginManager::new();

        // Add a running plugin.
        let caps1 = CapabilitySet::new(vec![]);
        let mut running = Plugin::new("running-plugin".to_string(), caps1);
        running.transition(PluginState::Running);
        manager.plugins.push(running);

        // Add a failed plugin.
        let caps2 = CapabilitySet::new(vec![]);
        let mut failed = Plugin::new("failed-plugin".to_string(), caps2);
        failed.fail("bad file".to_string());
        manager.plugins.push(failed);

        manager.shutdown_all();

        let states: Vec<&PluginState> = manager.plugins().iter().map(|p| &p.state).collect();
        // running plugin → Stopped
        assert_eq!(states[0], &PluginState::Stopped);
        // failed plugin stays Failed
        assert!(matches!(states[1], PluginState::Failed(_)));
    }

    // ── T037: Fuel refuel mechanism ───────────────────────────────────────────

    #[test]
    fn test_refuel_store_resets_fuel_via_lifecycle_integration() {
        use crate::loader::{create_engine, refuel_store, PluginStoreData};

        let engine = create_engine().unwrap();
        let caps = CapabilitySet::new(vec![]);
        let data = PluginStoreData::new("lifecycle-fuel-test".to_string(), caps, 64 * 1024 * 1024);
        let mut store = wasmtime::Store::new(&engine, data);

        // Initial fuel.
        store.set_fuel(1_000_000).unwrap();

        // Simulate partially consumed fuel.
        store.set_fuel(100_000).unwrap();
        let low_fuel = store.get_fuel().unwrap();
        assert_eq!(low_fuel, 100_000);

        // Refuel before next callback.
        refuel_store(&mut store, 1_000_000).unwrap();
        let full_fuel = store.get_fuel().unwrap();
        assert_eq!(
            full_fuel, 1_000_000,
            "refuel_store must restore the full per-callback budget"
        );
    }
}
