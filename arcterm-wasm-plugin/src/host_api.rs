//! Host API implementations for WASM plugins.
//!
//! Registers host functions that plugins can import. Provides four interfaces:
//!
//! | Interface                    | Functions                              |
//! |------------------------------|----------------------------------------|
//! | `arcterm:plugin/log`         | `info`, `warn`, `error`                |
//! | `arcterm:plugin/filesystem`  | `read-file`, `write-file`              |
//! | `arcterm:plugin/network`     | `http-get`, `http-post`                |
//! | `arcterm:plugin/terminal`    | `send-text`, `inject-output`           |
//!
//! Every privileged host function checks `ctx.data().capabilities.check()`
//! before performing any operation. On denial the function returns
//! `Err(denial_message)` — it does **not** trap the WASM guest.
//!
//! # Component Model import paths
//!
//! Plugins import functions via WIT:
//! ```wit
//! import arcterm:plugin/log@0.1.0;
//! import arcterm:plugin/filesystem@0.1.0;
//! import arcterm:plugin/network@0.1.0;
//! import arcterm:plugin/terminal@0.1.0;
//! ```

use crate::capability::{Capability, CapabilityOperation, CapabilityResource};
use crate::loader::PluginStoreData;
use anyhow::Context;
use wasmtime::component::Linker;

// ── Interface name constants ───────────────────────────────────────────────────

const LOG_INTERFACE: &str = "arcterm:plugin/log";
const FILESYSTEM_INTERFACE: &str = "arcterm:plugin/filesystem";
const NETWORK_INTERFACE: &str = "arcterm:plugin/network";
const TERMINAL_INTERFACE: &str = "arcterm:plugin/terminal";

// ── Log API ───────────────────────────────────────────────────────────────────

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
        .func_wrap(
            "info",
            |ctx: wasmtime::StoreContextMut<'_, PluginStoreData>, (msg,): (String,)| {
                let name = ctx.data().name.clone();
                log::info!("[plugin/{name}] {msg}");
                Ok(())
            },
        )
        .context("failed to register log::info host function")?;

    instance
        .func_wrap(
            "warn",
            |ctx: wasmtime::StoreContextMut<'_, PluginStoreData>, (msg,): (String,)| {
                let name = ctx.data().name.clone();
                log::warn!("[plugin/{name}] {msg}");
                Ok(())
            },
        )
        .context("failed to register log::warn host function")?;

    instance
        .func_wrap(
            "error",
            |ctx: wasmtime::StoreContextMut<'_, PluginStoreData>, (msg,): (String,)| {
                let name = ctx.data().name.clone();
                log::error!("[plugin/{name}] {msg}");
                Ok(())
            },
        )
        .context("failed to register log::error host function")?;

    Ok(())
}

// ── Filesystem API ─────────────────────────────────────────────────────────────

/// Register the `arcterm:plugin/filesystem` host functions into `linker`.
///
/// Registers two functions:
/// - `read-file(path: string) -> result<list<u8>, string>` — checks `fs:read`
///   capability with path as the target prefix, then reads the file.
/// - `write-file(path: string, data: list<u8>) -> result<_, string>` — checks
///   `fs:write` capability with path as the target prefix, then writes the file.
///
/// On capability denial both functions return `Err(String)` (not a trap).
///
/// # Errors
///
/// Returns an error if the interface or function names cannot be registered
/// in the linker (e.g., duplicate definitions).
pub fn register_filesystem_functions(linker: &mut Linker<PluginStoreData>) -> anyhow::Result<()> {
    let mut instance = linker
        .instance(FILESYSTEM_INTERFACE)
        .with_context(|| {
            format!("failed to create linker instance for '{FILESYSTEM_INTERFACE}'")
        })?;

    // read-file(path: string) -> result<list<u8>, string>
    instance
        .func_wrap(
            "read-file",
            |ctx: wasmtime::StoreContextMut<'_, PluginStoreData>,
             (path,): (String,)|
             -> anyhow::Result<(Result<Vec<u8>, String>,)> {
                let required = Capability {
                    resource: CapabilityResource::Filesystem,
                    operation: CapabilityOperation::Read,
                    target: Some(path.clone()),
                };

                if let Err(denied) = ctx.data().capabilities.check(&required) {
                    return Ok((Err(format!(
                        "Plugin denied capability fs:read — not granted (path: {path}): {denied}"
                    )),));
                }

                match std::fs::read(&path) {
                    Ok(bytes) => Ok((Ok(bytes),)),
                    Err(e) => Ok((Err(format!("fs:read '{path}' failed: {e}")),)),
                }
            },
        )
        .context("failed to register filesystem::read-file host function")?;

    // write-file(path: string, data: list<u8>) -> result<_, string>
    instance
        .func_wrap(
            "write-file",
            |ctx: wasmtime::StoreContextMut<'_, PluginStoreData>,
             (path, data): (String, Vec<u8>)|
             -> anyhow::Result<(Result<(), String>,)> {
                let required = Capability {
                    resource: CapabilityResource::Filesystem,
                    operation: CapabilityOperation::Write,
                    target: Some(path.clone()),
                };

                if let Err(denied) = ctx.data().capabilities.check(&required) {
                    return Ok((Err(format!(
                        "Plugin denied capability fs:write — not granted (path: {path}): {denied}"
                    )),));
                }

                match std::fs::write(&path, &data) {
                    Ok(()) => Ok((Ok(()),)),
                    Err(e) => Ok((Err(format!("fs:write '{path}' failed: {e}")),)),
                }
            },
        )
        .context("failed to register filesystem::write-file host function")?;

    Ok(())
}

// ── Network API ────────────────────────────────────────────────────────────────

/// Register the `arcterm:plugin/network` host functions into `linker`.
///
/// Registers two functions:
/// - `http-get(url: string) -> result<tuple<u16, list<u8>>, string>` — checks
///   `net:connect` capability, then performs an HTTP GET.
/// - `http-post(url: string, body: list<u8>) -> result<tuple<u16, list<u8>>, string>`
///   — checks `net:connect` capability, then performs an HTTP POST.
///
/// **Placeholder**: both functions currently return
/// `Err("network not yet implemented")` after the capability check passes.
/// The actual HTTP client will be wired in a future task.
///
/// On capability denial both functions return `Err(String)` (not a trap).
///
/// # Errors
///
/// Returns an error if the interface or function names cannot be registered
/// in the linker (e.g., duplicate definitions).
pub fn register_network_functions(linker: &mut Linker<PluginStoreData>) -> anyhow::Result<()> {
    let mut instance = linker
        .instance(NETWORK_INTERFACE)
        .with_context(|| format!("failed to create linker instance for '{NETWORK_INTERFACE}'"))?;

    // http-get(url: string) -> result<(u16, list<u8>), string>
    instance
        .func_wrap(
            "http-get",
            |ctx: wasmtime::StoreContextMut<'_, PluginStoreData>,
             (url,): (String,)|
             -> anyhow::Result<(Result<(u32, Vec<u8>), String>,)> {
                // Extract host:port from URL for capability target matching.
                let target = extract_host_port(&url);
                let required = Capability {
                    resource: CapabilityResource::Network,
                    operation: CapabilityOperation::Connect,
                    target: Some(target),
                };

                if let Err(denied) = ctx.data().capabilities.check(&required) {
                    return Ok((Err(format!(
                        "Plugin denied capability net:connect — not granted (url: {url}): {denied}"
                    )),));
                }

                // Placeholder: HTTP client integration is deferred.
                Ok((Err("network not yet implemented".to_string()),))
            },
        )
        .context("failed to register network::http-get host function")?;

    // http-post(url: string, body: list<u8>) -> result<(u16, list<u8>), string>
    instance
        .func_wrap(
            "http-post",
            |ctx: wasmtime::StoreContextMut<'_, PluginStoreData>,
             (url, _body): (String, Vec<u8>)|
             -> anyhow::Result<(Result<(u32, Vec<u8>), String>,)> {
                let target = extract_host_port(&url);
                let required = Capability {
                    resource: CapabilityResource::Network,
                    operation: CapabilityOperation::Connect,
                    target: Some(target),
                };

                if let Err(denied) = ctx.data().capabilities.check(&required) {
                    return Ok((Err(format!(
                        "Plugin denied capability net:connect — not granted (url: {url}): {denied}"
                    )),));
                }

                // Placeholder: HTTP client integration is deferred.
                Ok((Err("network not yet implemented".to_string()),))
            },
        )
        .context("failed to register network::http-post host function")?;

    Ok(())
}

/// Extract `host:port` from a URL for capability target matching.
///
/// Handles `scheme://host:port/path` and bare `host:port` forms.
/// Falls back to returning the full URL if parsing fails.
fn extract_host_port(url: &str) -> String {
    // Strip scheme if present.
    let after_scheme = if let Some(pos) = url.find("://") {
        &url[pos + 3..]
    } else {
        url
    };

    // Take the authority (everything before the first '/').
    let authority = if let Some(pos) = after_scheme.find('/') {
        &after_scheme[..pos]
    } else {
        after_scheme
    };

    authority.to_string()
}

// ── Terminal-write API ─────────────────────────────────────────────────────────

/// Register the `arcterm:plugin/terminal` host functions into `linker`.
///
/// Registers two functions:
/// - `send-text(text: string) -> result<_, string>` — checks `terminal:write`
///   capability, then logs the text (pane integration deferred).
/// - `inject-output(text: string) -> result<_, string>` — same as above.
///
/// **Placeholder**: both functions log the text and return `Ok(())` once the
/// capability check passes. Actual pane routing will be wired during GUI
/// integration in a future task.
///
/// On capability denial both functions return `Err(String)` (not a trap).
///
/// # Errors
///
/// Returns an error if the interface or function names cannot be registered
/// in the linker (e.g., duplicate definitions).
pub fn register_terminal_write_functions(
    linker: &mut Linker<PluginStoreData>,
) -> anyhow::Result<()> {
    let mut instance = linker
        .instance(TERMINAL_INTERFACE)
        .with_context(|| format!("failed to create linker instance for '{TERMINAL_INTERFACE}'"))?;

    // send-text(text: string) -> result<_, string>
    instance
        .func_wrap(
            "send-text",
            |ctx: wasmtime::StoreContextMut<'_, PluginStoreData>,
             (text,): (String,)|
             -> anyhow::Result<(Result<(), String>,)> {
                let required = Capability {
                    resource: CapabilityResource::Terminal,
                    operation: CapabilityOperation::Write,
                    target: None,
                };

                if let Err(denied) = ctx.data().capabilities.check(&required) {
                    return Ok((Err(format!(
                        "Plugin denied capability terminal:write — not granted: {denied}"
                    )),));
                }

                let name = ctx.data().name.clone();
                log::info!("[plugin/{name}] send-text: {text}");
                Ok((Ok(()),))
            },
        )
        .context("failed to register terminal::send-text host function")?;

    // inject-output(text: string) -> result<_, string>
    instance
        .func_wrap(
            "inject-output",
            |ctx: wasmtime::StoreContextMut<'_, PluginStoreData>,
             (text,): (String,)|
             -> anyhow::Result<(Result<(), String>,)> {
                let required = Capability {
                    resource: CapabilityResource::Terminal,
                    operation: CapabilityOperation::Write,
                    target: None,
                };

                if let Err(denied) = ctx.data().capabilities.check(&required) {
                    return Ok((Err(format!(
                        "Plugin denied capability terminal:write — not granted: {denied}"
                    )),));
                }

                let name = ctx.data().name.clone();
                log::info!("[plugin/{name}] inject-output: {text}");
                Ok((Ok(()),))
            },
        )
        .context("failed to register terminal::inject-output host function")?;

    Ok(())
}

// ── Default linker ─────────────────────────────────────────────────────────────

/// Convenience function: create a fresh [`Linker`] and register all built-in
/// host functions (log, filesystem, network, terminal-write).
///
/// Callers that need to add additional host functions should call the
/// individual `register_*` functions directly on their own [`Linker`].
pub fn create_default_linker(engine: &wasmtime::Engine) -> anyhow::Result<Linker<PluginStoreData>> {
    let mut linker = Linker::new(engine);
    register_log_functions(&mut linker).context("failed to register log host functions")?;
    register_filesystem_functions(&mut linker)
        .context("failed to register filesystem host functions")?;
    register_network_functions(&mut linker)
        .context("failed to register network host functions")?;
    register_terminal_write_functions(&mut linker)
        .context("failed to register terminal-write host functions")?;
    Ok(linker)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{Capability, CapabilitySet};
    use crate::loader::{create_engine, PluginStoreData};
    use wasmtime::Store;

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Build a store with an empty capability set (only default terminal:read).
    fn store_no_caps(engine: &wasmtime::Engine) -> Store<PluginStoreData> {
        let caps = CapabilitySet::new(vec![]);
        let data = PluginStoreData::new("test-plugin".to_string(), caps, 64 * 1024 * 1024);
        Store::new(engine, data)
    }

    /// Build a store with explicit capability strings parsed into a set.
    fn store_with_caps(engine: &wasmtime::Engine, cap_strs: &[&str]) -> Store<PluginStoreData> {
        let caps: Vec<Capability> = cap_strs
            .iter()
            .map(|s| Capability::parse(s).unwrap())
            .collect();
        let set = CapabilitySet::new(caps);
        let data = PluginStoreData::new("test-plugin".to_string(), set, 64 * 1024 * 1024);
        Store::new(engine, data)
    }

    // ── Log registration tests ─────────────────────────────────────────────────

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
        // Registering the same interface+function twice should fail.
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
        let caps = CapabilitySet::new(vec![]);
        let data =
            crate::loader::PluginStoreData::new("my-plugin".to_string(), caps, 64 * 1024 * 1024);
        assert_eq!(data.name, "my-plugin");
    }

    // ── Filesystem registration tests ─────────────────────────────────────────

    #[test]
    fn test_register_filesystem_functions_succeeds() {
        let engine = create_engine().unwrap();
        let mut linker: Linker<PluginStoreData> = Linker::new(&engine);
        assert!(
            register_filesystem_functions(&mut linker).is_ok(),
            "filesystem function registration should succeed"
        );
    }

    #[test]
    fn test_register_filesystem_duplicate_fails() {
        let engine = create_engine().unwrap();
        let mut linker: Linker<PluginStoreData> = Linker::new(&engine);
        register_filesystem_functions(&mut linker).unwrap();
        assert!(register_filesystem_functions(&mut linker).is_err());
    }

    // ── Filesystem capability enforcement tests ───────────────────────────────

    /// Helper: call the registered `read-file` typed function against a store.
    ///
    /// We exercise capability enforcement by directly checking `capabilities`
    /// on the store data rather than calling through the wasmtime linker
    /// (which requires a compiled WASM guest).  This keeps the tests fast and
    /// dependency-free while validating the exact logic branches used in the
    /// host function closures.
    fn check_fs_read(store: &Store<PluginStoreData>, path: &str) -> Result<(), String> {
        let required = Capability {
            resource: CapabilityResource::Filesystem,
            operation: CapabilityOperation::Read,
            target: Some(path.to_string()),
        };
        store
            .data()
            .capabilities
            .check(&required)
            .map_err(|denied| {
                format!("Plugin denied capability fs:read — not granted (path: {path}): {denied}")
            })
    }

    fn check_fs_write(store: &Store<PluginStoreData>, path: &str) -> Result<(), String> {
        let required = Capability {
            resource: CapabilityResource::Filesystem,
            operation: CapabilityOperation::Write,
            target: Some(path.to_string()),
        };
        store
            .data()
            .capabilities
            .check(&required)
            .map_err(|denied| {
                format!("Plugin denied capability fs:write — not granted (path: {path}): {denied}")
            })
    }

    #[test]
    fn test_fs_read_denied_without_capability() {
        let engine = create_engine().unwrap();
        let store = store_no_caps(&engine);
        let result = check_fs_read(&store, "/etc/passwd");
        assert!(
            result.is_err(),
            "fs:read should be denied without the capability"
        );
        let msg = result.unwrap_err();
        assert!(
            msg.contains("fs:read — not granted"),
            "denial message should mention fs:read: {msg}"
        );
    }

    #[test]
    fn test_fs_write_denied_without_capability() {
        let engine = create_engine().unwrap();
        let store = store_no_caps(&engine);
        let result = check_fs_write(&store, "/tmp/evil.txt");
        assert!(
            result.is_err(),
            "fs:write should be denied without the capability"
        );
        let msg = result.unwrap_err();
        assert!(
            msg.contains("fs:write — not granted"),
            "denial message should mention fs:write: {msg}"
        );
    }

    #[test]
    fn test_fs_read_allowed_within_granted_prefix() {
        let engine = create_engine().unwrap();
        let store = store_with_caps(&engine, &["fs:read:/tmp"]);
        assert!(check_fs_read(&store, "/tmp/allowed.txt").is_ok());
    }

    #[test]
    fn test_fs_read_denied_outside_granted_prefix() {
        let engine = create_engine().unwrap();
        let store = store_with_caps(&engine, &["fs:read:/tmp"]);
        let result = check_fs_read(&store, "/etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_fs_write_allowed_within_granted_prefix() {
        let engine = create_engine().unwrap();
        let store = store_with_caps(&engine, &["fs:write:/tmp"]);
        assert!(check_fs_write(&store, "/tmp/output.txt").is_ok());
    }

    // ── Network registration tests ────────────────────────────────────────────

    #[test]
    fn test_register_network_functions_succeeds() {
        let engine = create_engine().unwrap();
        let mut linker: Linker<PluginStoreData> = Linker::new(&engine);
        assert!(
            register_network_functions(&mut linker).is_ok(),
            "network function registration should succeed"
        );
    }

    #[test]
    fn test_register_network_duplicate_fails() {
        let engine = create_engine().unwrap();
        let mut linker: Linker<PluginStoreData> = Linker::new(&engine);
        register_network_functions(&mut linker).unwrap();
        assert!(register_network_functions(&mut linker).is_err());
    }

    // ── Network capability enforcement tests ──────────────────────────────────

    fn check_net_connect(store: &Store<PluginStoreData>, url: &str) -> Result<(), String> {
        let target = extract_host_port(url);
        let required = Capability {
            resource: CapabilityResource::Network,
            operation: CapabilityOperation::Connect,
            target: Some(target),
        };
        store
            .data()
            .capabilities
            .check(&required)
            .map_err(|denied| {
                format!("Plugin denied capability net:connect — not granted (url: {url}): {denied}")
            })
    }

    #[test]
    fn test_net_get_denied_without_capability() {
        let engine = create_engine().unwrap();
        let store = store_no_caps(&engine);
        let result = check_net_connect(&store, "https://api.example.com:443/data");
        assert!(
            result.is_err(),
            "net:connect should be denied without the capability"
        );
        let msg = result.unwrap_err();
        assert!(
            msg.contains("net:connect — not granted"),
            "denial message should mention net:connect: {msg}"
        );
    }

    #[test]
    fn test_net_post_denied_without_capability() {
        let engine = create_engine().unwrap();
        let store = store_no_caps(&engine);
        let result = check_net_connect(&store, "https://api.example.com:443/upload");
        assert!(result.is_err());
    }

    #[test]
    fn test_net_connect_allowed_with_matching_capability() {
        let engine = create_engine().unwrap();
        let store = store_with_caps(&engine, &["net:connect:api.example.com:443"]);
        let result = check_net_connect(&store, "https://api.example.com:443/data");
        assert!(
            result.is_ok(),
            "net:connect should be allowed when host:port matches: {:?}",
            result
        );
    }

    #[test]
    fn test_net_connect_denied_wrong_host() {
        let engine = create_engine().unwrap();
        let store = store_with_caps(&engine, &["net:connect:api.example.com:443"]);
        let result = check_net_connect(&store, "https://evil.example.com:443/steal");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_host_port_with_scheme_and_path() {
        assert_eq!(
            extract_host_port("https://api.example.com:443/some/path"),
            "api.example.com:443"
        );
    }

    #[test]
    fn test_extract_host_port_bare() {
        assert_eq!(
            extract_host_port("api.example.com:443"),
            "api.example.com:443"
        );
    }

    #[test]
    fn test_extract_host_port_no_port() {
        assert_eq!(extract_host_port("https://example.com/"), "example.com");
    }

    // ── Terminal-write registration tests ─────────────────────────────────────

    #[test]
    fn test_register_terminal_write_functions_succeeds() {
        let engine = create_engine().unwrap();
        let mut linker: Linker<PluginStoreData> = Linker::new(&engine);
        assert!(
            register_terminal_write_functions(&mut linker).is_ok(),
            "terminal-write function registration should succeed"
        );
    }

    #[test]
    fn test_register_terminal_write_duplicate_fails() {
        let engine = create_engine().unwrap();
        let mut linker: Linker<PluginStoreData> = Linker::new(&engine);
        register_terminal_write_functions(&mut linker).unwrap();
        assert!(register_terminal_write_functions(&mut linker).is_err());
    }

    // ── Terminal-write capability enforcement tests ───────────────────────────

    fn check_terminal_write(store: &Store<PluginStoreData>) -> Result<(), String> {
        let required = Capability {
            resource: CapabilityResource::Terminal,
            operation: CapabilityOperation::Write,
            target: None,
        };
        store
            .data()
            .capabilities
            .check(&required)
            .map_err(|denied| {
                format!("Plugin denied capability terminal:write — not granted: {denied}")
            })
    }

    #[test]
    fn test_terminal_send_text_denied_without_capability() {
        let engine = create_engine().unwrap();
        let store = store_no_caps(&engine);
        let result = check_terminal_write(&store);
        assert!(
            result.is_err(),
            "terminal:write should be denied without the capability"
        );
        let msg = result.unwrap_err();
        assert!(
            msg.contains("terminal:write — not granted"),
            "denial message should mention terminal:write: {msg}"
        );
    }

    #[test]
    fn test_terminal_inject_output_denied_without_capability() {
        let engine = create_engine().unwrap();
        let store = store_no_caps(&engine);
        // inject-output uses the same terminal:write capability as send-text.
        let result = check_terminal_write(&store);
        assert!(result.is_err());
    }

    #[test]
    fn test_terminal_write_allowed_with_capability() {
        let engine = create_engine().unwrap();
        let store = store_with_caps(&engine, &["terminal:write"]);
        let result = check_terminal_write(&store);
        assert!(
            result.is_ok(),
            "terminal:write should be allowed when capability is granted"
        );
    }

    // ── Default linker integration test ───────────────────────────────────────

    #[test]
    fn test_default_linker_registers_all_interfaces() {
        // Verifies that create_default_linker successfully registers all four
        // interface groups without error.
        let engine = create_engine().unwrap();
        let result = create_default_linker(&engine);
        assert!(
            result.is_ok(),
            "default linker should register all interfaces: {:?}",
            result.err()
        );
    }
}
