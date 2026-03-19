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
        log::info!(
            "Plugin '{}': {} → {}",
            self.name,
            self.state,
            new_state
        );
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
