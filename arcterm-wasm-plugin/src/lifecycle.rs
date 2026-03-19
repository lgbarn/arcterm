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
    pub fn shutdown_all(&mut self) {
        for plugin in &mut self.plugins {
            if plugin.is_running() {
                plugin.transition(PluginState::Stopping);
                // TODO: Call plugin's destroy() export when Component Model
                // instantiation is wired up
                plugin.transition(PluginState::Stopped);
            }
        }
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
