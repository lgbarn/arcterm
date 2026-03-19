//! WASM plugin system for ArcTerm.
//!
//! Provides capability-based sandboxed WASM plugin execution using wasmtime
//! with the Component Model. Plugins are configured via the Lua config system
//! and coexist with existing Lua plugins.

pub mod capability;
pub mod config;
pub mod event_router;
pub mod host_api;
pub mod lifecycle;
pub mod loader;
