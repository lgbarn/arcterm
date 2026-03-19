//! WASM plugin loader using wasmtime Component Model.

// TODO: Add wasmtime::component::bindgen! macro invocation once WIT is finalized
// and wasmtime version is confirmed compatible.
//
// For now, this module provides the engine and store creation utilities.

use crate::config::WasmPluginConfig;
use anyhow::Context;

/// Create a wasmtime Engine configured for plugin execution.
pub fn create_engine() -> anyhow::Result<wasmtime::Engine> {
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    config.wasm_component_model(true);
    wasmtime::Engine::new(&config).context("Failed to create wasmtime engine")
}
