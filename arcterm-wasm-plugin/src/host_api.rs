//! Host API implementations for WASM plugins.
//!
//! Registers host functions that plugins can import. Currently provides the
//! `arcterm:plugin/log` interface with `info`, `warn`, and `error` functions.
//!
//! Each host function is prefixed with the plugin name (retrieved from store
//! data) so log output is unambiguously attributed to the calling plugin.
//!
//! # Component Model import path
//!
//! Plugins import log functions via:
//! ```wit
//! import arcterm:plugin/log@0.1.0;
//! ```
//!
//! which maps to the linker path `arcterm:plugin/log`.

use crate::loader::PluginStoreData;
use anyhow::Context;
use wasmtime::component::Linker;

/// The WIT interface namespace + package + interface used for log imports.
const LOG_INTERFACE: &str = "arcterm:plugin/log";

/// Register the `arcterm:plugin/log` host functions into `linker`.
///
/// Registers three functions under `arcterm:plugin/log`:
/// - `info(msg: string)` — logs at INFO level
/// - `warn(msg: string)` — logs at WARN level
/// - `error(msg: string)` — logs at ERROR level
///
/// Each function retrieves the plugin name from [`PluginStoreData`] so that
/// every log line is prefixed with `[plugin/<name>]`.
///
/// # Errors
///
/// Returns an error if the interface or function names cannot be registered
/// in the linker (e.g., duplicate definitions).
pub fn register_log_functions(linker: &mut Linker<PluginStoreData>) -> anyhow::Result<()> {
    let mut instance = linker
        .instance(LOG_INTERFACE)
        .with_context(|| format!("failed to create linker instance for '{LOG_INTERFACE}'"))?;

    instance
        .func_wrap("info", |ctx: wasmtime::StoreContextMut<'_, PluginStoreData>, (msg,): (String,)| {
            log::info!("[plugin/{}] {}", ctx.data().name, msg);
            Ok(())
        })
        .context("failed to register log::info host function")?;

    instance
        .func_wrap("warn", |ctx: wasmtime::StoreContextMut<'_, PluginStoreData>, (msg,): (String,)| {
            log::warn!("[plugin/{}] {}", ctx.data().name, msg);
            Ok(())
        })
        .context("failed to register log::warn host function")?;

    instance
        .func_wrap("error", |ctx: wasmtime::StoreContextMut<'_, PluginStoreData>, (msg,): (String,)| {
            log::error!("[plugin/{}] {}", ctx.data().name, msg);
            Ok(())
        })
        .context("failed to register log::error host function")?;

    Ok(())
}

/// Convenience function: create a fresh [`Linker`] and register all built-in
/// host functions.
///
/// Callers that need to add additional host functions should call
/// [`register_log_functions`] directly on their own [`Linker`].
pub fn create_default_linker(
    engine: &wasmtime::Engine,
) -> anyhow::Result<Linker<PluginStoreData>> {
    let mut linker = Linker::new(engine);
    register_log_functions(&mut linker)
        .context("failed to register default host functions")?;
    Ok(linker)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilitySet;
    use crate::loader::create_engine;

    #[test]
    fn test_register_log_functions_no_error() {
        let engine = create_engine().unwrap();
        let mut linker: Linker<PluginStoreData> = Linker::new(&engine);
        assert!(
            register_log_functions(&mut linker).is_ok(),
            "log function registration should succeed"
        );
    }

    #[test]
    fn test_create_default_linker_succeeds() {
        let engine = create_engine().unwrap();
        assert!(
            create_default_linker(&engine).is_ok(),
            "default linker creation should succeed"
        );
    }

    #[test]
    fn test_register_log_functions_duplicate_fails() {
        // Registering the same interface+function twice should fail because
        // the linker disallows shadowing by default.
        let engine = create_engine().unwrap();
        let mut linker: Linker<PluginStoreData> = Linker::new(&engine);
        register_log_functions(&mut linker).unwrap();
        let result = register_log_functions(&mut linker);
        assert!(
            result.is_err(),
            "duplicate registration should return an error"
        );
    }

    #[test]
    fn test_plugin_store_data_name_accessible() {
        // Verify that PluginStoreData exposes the name field used by log funcs.
        let caps = CapabilitySet::new(vec![]);
        let data = crate::loader::PluginStoreData::new(
            "my-plugin".to_string(),
            caps,
            64 * 1024 * 1024,
        );
        assert_eq!(data.name, "my-plugin");
    }
}
