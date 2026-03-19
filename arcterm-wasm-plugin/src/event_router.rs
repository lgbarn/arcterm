//! Event routing from MuxNotification to WASM plugin callbacks.
//!
//! Routes terminal events (output changes, bell, focus) from the mux
//! notification system to WASM plugins that have registered handlers.

/// Events that can be delivered to WASM plugins.
#[derive(Debug, Clone)]
pub enum PluginEvent {
    /// Terminal output changed (debounced).
    OutputChanged { text: String },
    /// Bell character received.
    Bell,
    /// Pane gained or lost focus.
    FocusChanged { focused: bool },
    /// A registered key binding was triggered.
    KeyBindingTriggered { binding_id: u32 },
}

/// Routes events from the terminal to registered plugin handlers.
pub struct EventRouter {
    /// Plugins registered for output change events.
    output_subscribers: Vec<String>,
    /// Plugins registered for bell events.
    bell_subscribers: Vec<String>,
    /// Plugins registered for focus events.
    focus_subscribers: Vec<String>,
}

impl EventRouter {
    /// Create a new empty event router.
    pub fn new() -> Self {
        Self {
            output_subscribers: Vec::new(),
            bell_subscribers: Vec::new(),
            focus_subscribers: Vec::new(),
        }
    }

    /// Register a plugin for output change events.
    pub fn subscribe_output(&mut self, plugin_name: String) {
        self.output_subscribers.push(plugin_name);
    }

    /// Register a plugin for bell events.
    pub fn subscribe_bell(&mut self, plugin_name: String) {
        self.bell_subscribers.push(plugin_name);
    }

    /// Register a plugin for focus events.
    pub fn subscribe_focus(&mut self, plugin_name: String) {
        self.focus_subscribers.push(plugin_name);
    }

    /// Get the names of plugins subscribed to a given event type.
    pub fn subscribers_for(&self, event: &PluginEvent) -> &[String] {
        match event {
            PluginEvent::OutputChanged { .. } => &self.output_subscribers,
            PluginEvent::Bell => &self.bell_subscribers,
            PluginEvent::FocusChanged { .. } => &self.focus_subscribers,
            PluginEvent::KeyBindingTriggered { .. } => &[], // handled separately
        }
    }

    /// Dispatch an event to all subscribed plugins.
    /// Returns the list of plugin names that should receive the event.
    pub fn dispatch(&self, event: &PluginEvent) -> Vec<String> {
        self.subscribers_for(event).to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_router_has_no_subscribers() {
        let router = EventRouter::new();
        let event = PluginEvent::Bell;
        assert!(router.dispatch(&event).is_empty());
    }

    #[test]
    fn test_subscribe_and_dispatch() {
        let mut router = EventRouter::new();
        router.subscribe_output("watcher".to_string());
        router.subscribe_bell("alerter".to_string());

        let output_event = PluginEvent::OutputChanged { text: "hello".to_string() };
        assert_eq!(router.dispatch(&output_event), vec!["watcher"]);

        let bell_event = PluginEvent::Bell;
        assert_eq!(router.dispatch(&bell_event), vec!["alerter"]);
    }

    #[test]
    fn test_multiple_subscribers() {
        let mut router = EventRouter::new();
        router.subscribe_output("plugin1".to_string());
        router.subscribe_output("plugin2".to_string());

        let event = PluginEvent::OutputChanged { text: "test".to_string() };
        let subs = router.dispatch(&event);
        assert_eq!(subs.len(), 2);
        assert!(subs.contains(&"plugin1".to_string()));
        assert!(subs.contains(&"plugin2".to_string()));
    }
}
