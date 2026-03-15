/// Unique identifier for a loaded plugin instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginId(pub String);

impl PluginId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

// Re-export the bindgen-generated types as the canonical public API of this crate.
// These are generated from arcterm-plugin/wit/arcterm.wit by the bindgen! macro.
pub use crate::host::arcterm::plugin::types::{
    Color, EventKind, PluginEvent, StyledLine, ToolSchema,
};
