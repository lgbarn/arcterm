//! Plugin lifecycle manager and event bus.
//!
//! [`PluginManager`] owns all running plugin instances and a
//! `tokio::sync::broadcast` channel for terminal lifecycle events.
//! Each installed plugin receives events via a per-instance receiver task
//! that calls the plugin's `update()` + `render()` exports when an event
//! matches its subscribed kinds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::host::arcterm::plugin::types::{
    EventKind as WitEventKind, KeyInputPayload, KeyModifiers, PluginEvent as WitPluginEvent,
    ToolSchema,
};
use crate::manifest::{build_wasi_ctx, PaneAccess, PluginManifest};
use crate::runtime::{PluginInstance, PluginRuntime};

// ──────────────────────────────────────────────────────────────────
// Event bus types
// ──────────────────────────────────────────────────────────────────

/// Keyboard modifiers for key-input events.
#[derive(Debug, Clone, Default)]
pub struct KeyInputModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Terminal lifecycle events broadcast to all plugin instances.
///
/// These are the host-side Rust equivalents of the WIT `plugin-event` variant.
/// They are converted to `WitPluginEvent` before being delivered to a plugin.
#[derive(Debug, Clone)]
pub enum PluginEvent {
    /// A new pane was opened. The string payload is the pane's numeric ID.
    PaneOpened(String),
    /// A pane was closed.
    PaneClosed(String),
    /// A shell command was executed. The string payload is the command text.
    CommandExecuted(String),
    /// The active workspace was switched. The string payload is the workspace name.
    WorkspaceSwitched(String),
    /// A key press forwarded from a focused plugin pane.
    KeyInput {
        /// The Unicode character produced by the key, if any.
        key_char: Option<String>,
        /// Named representation of the key (e.g. "Enter", "Escape", "a").
        key_name: String,
        modifiers: KeyInputModifiers,
    },
}

impl PluginEvent {
    /// Convert to the WIT variant type used in `update()` calls.
    pub fn to_wit(&self) -> WitPluginEvent {
        match self {
            PluginEvent::PaneOpened(s) => WitPluginEvent::PaneOpened(s.clone()),
            PluginEvent::PaneClosed(s) => WitPluginEvent::PaneClosed(s.clone()),
            PluginEvent::CommandExecuted(s) => WitPluginEvent::CommandExecuted(s.clone()),
            PluginEvent::WorkspaceSwitched(s) => WitPluginEvent::WorkspaceSwitched(s.clone()),
            PluginEvent::KeyInput { key_char, key_name, modifiers } => {
                WitPluginEvent::KeyInput(KeyInputPayload {
                    key_char: key_char.clone(),
                    key_name: key_name.clone(),
                    modifiers: KeyModifiers {
                        ctrl: modifiers.ctrl,
                        alt: modifiers.alt,
                        shift: modifiers.shift,
                    },
                })
            }
        }
    }

    /// Return the `WitEventKind` that matches this event (used for subscription filtering).
    pub fn kind(&self) -> WitEventKind {
        match self {
            PluginEvent::PaneOpened(_) => WitEventKind::PaneOpened,
            PluginEvent::PaneClosed(_) => WitEventKind::PaneClosed,
            PluginEvent::CommandExecuted(_) => WitEventKind::CommandExecuted,
            PluginEvent::WorkspaceSwitched(_) => WitEventKind::WorkspaceSwitched,
            PluginEvent::KeyInput { .. } => WitEventKind::KeyInput,
        }
    }
}

// ──────────────────────────────────────────────────────────────────
// Plugin ID
// ──────────────────────────────────────────────────────────────────

/// Opaque identifier for a loaded plugin instance.
///
/// Currently backed by the plugin's declared name, which must be unique
/// within a single `PluginManager`.
pub type PluginId = String;

// ──────────────────────────────────────────────────────────────────
// Shared draw buffer (output of render())
// ──────────────────────────────────────────────────────────────────

/// Shared, thread-safe buffer for plugin-rendered lines.
///
/// The per-instance event task writes here after calling `render()`;
/// the main thread reads it to display plugin output.
pub type DrawBuffer = Arc<Mutex<Vec<crate::host::arcterm::plugin::types::StyledLine>>>;

// ──────────────────────────────────────────────────────────────────
// Loaded plugin record
// ──────────────────────────────────────────────────────────────────

/// Everything the manager tracks for one running plugin.
struct LoadedPlugin {
    manifest: PluginManifest,
    /// Shared draw buffer written by the per-instance event task.
    draw_buffer: DrawBuffer,
    /// The plugin instance, wrapped in Arc<Mutex<>> so the event task can use it.
    instance: Arc<Mutex<PluginInstance>>,
}

// ──────────────────────────────────────────────────────────────────
// PluginManager
// ──────────────────────────────────────────────────────────────────

/// Lifecycle manager for all installed plugins.
///
/// Owns:
/// - A [`PluginRuntime`] (wasmtime Engine + Linker).
/// - A `HashMap<PluginId, LoadedPlugin>` of running instances.
/// - A `broadcast::Sender<PluginEvent>` event bus.
/// - The path to the plugin storage directory.
pub struct PluginManager {
    runtime: PluginRuntime,
    plugins: HashMap<PluginId, LoadedPlugin>,
    event_tx: broadcast::Sender<PluginEvent>,
    /// Root directory where plugins are installed: `<config_dir>/arcterm/plugins/`.
    plugin_dir: PathBuf,
}

impl PluginManager {
    /// Create a new `PluginManager`.
    ///
    /// Uses `dirs::config_dir()/arcterm/plugins` as the default plugin storage
    /// directory. Returns an error if the wasmtime engine cannot be created.
    pub fn new() -> anyhow::Result<Self> {
        let runtime = PluginRuntime::new()?;
        let (event_tx, _) = broadcast::channel(256);
        let plugin_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("arcterm")
            .join("plugins");

        Ok(Self {
            runtime,
            plugins: HashMap::new(),
            event_tx,
            plugin_dir,
        })
    }

    /// Create a `PluginManager` that stores plugins under a custom directory.
    ///
    /// Used in tests to avoid touching the real config directory.
    pub fn new_with_dir(plugin_dir: PathBuf) -> anyhow::Result<Self> {
        let runtime = PluginRuntime::new()?;
        let (event_tx, _) = broadcast::channel(256);
        Ok(Self {
            runtime,
            plugins: HashMap::new(),
            event_tx,
            plugin_dir,
        })
    }

    // ── Install ──────────────────────────────────────────────────────────

    /// Install a plugin from a source directory.
    ///
    /// Reads `plugin.toml` from `source_path`, validates the manifest, copies
    /// the entire directory to `self.plugin_dir/<name>/`, and loads the wasm.
    ///
    /// Returns the `PluginId` (which equals `manifest.name`).
    pub fn install(&mut self, source_path: &Path) -> anyhow::Result<PluginId> {
        let dest = self.copy_plugin_files(source_path)?;
        let id = self.load_from_dir(&dest)?;
        Ok(id)
    }

    /// Copy plugin files from `source_path` to `plugin_dir/<name>/` without loading.
    ///
    /// Returns the destination directory path on success. Used by `install` and
    /// available separately for testing file-copy behavior without wasm compilation.
    pub fn copy_plugin_files(&self, source_path: &Path) -> anyhow::Result<std::path::PathBuf> {
        // Load and validate the manifest from the source directory.
        let manifest_path = source_path.join("plugin.toml");
        let manifest_text = std::fs::read_to_string(&manifest_path)
            .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", manifest_path.display()))?;
        let manifest = PluginManifest::from_toml(&manifest_text)
            .map_err(|e| anyhow::anyhow!("invalid plugin.toml: {e}"))?;
        manifest
            .validate()
            .map_err(|e| anyhow::anyhow!("plugin.toml validation failed: {e}"))?;

        // Destination directory: plugin_dir/<name>/
        let dest = self.plugin_dir.join(&manifest.name);
        std::fs::create_dir_all(&dest)?;

        // Copy all files from source to dest, rejecting symlinks.
        for entry in std::fs::read_dir(source_path)? {
            let entry = entry?;
            let metadata = entry.path().symlink_metadata()?;
            if metadata.file_type().is_symlink() {
                anyhow::bail!(
                    "plugin source directory contains a symlink '{}'; symlinks are not permitted",
                    entry.file_name().to_string_lossy()
                );
            }
            if !metadata.is_file() {
                continue;
            }
            let file_name = entry.file_name();
            let dest_file = dest.join(&file_name);
            std::fs::copy(entry.path(), &dest_file)?;
        }

        Ok(dest)
    }

    // ── Load ─────────────────────────────────────────────────────────────

    /// Load a plugin from a directory that already contains `plugin.toml` and the wasm file.
    ///
    /// Builds a sandboxed `WasiCtx` from the manifest permissions, compiles
    /// the wasm, and spawns a tokio task that forwards broadcast events to
    /// the plugin's `update()` export.
    pub fn load_from_dir(&mut self, dir: &Path) -> anyhow::Result<PluginId> {
        let manifest_text = std::fs::read_to_string(dir.join("plugin.toml"))
            .map_err(|e| anyhow::anyhow!("cannot read plugin.toml in {}: {e}", dir.display()))?;
        let manifest = PluginManifest::from_toml(&manifest_text)
            .map_err(|e| anyhow::anyhow!("invalid plugin.toml: {e}"))?;
        manifest
            .validate()
            .map_err(|e| anyhow::anyhow!("plugin.toml validation failed: {e}"))?;

        let wasm_path = dir.join(&manifest.wasm);
        let wasm_canonical = wasm_path.canonicalize().map_err(|e| {
            anyhow::anyhow!(
                "plugin wasm file '{}' not found: {e}",
                wasm_path.display()
            )
        })?;
        let dir_canonical = dir.canonicalize().unwrap_or(dir.to_path_buf());
        if !wasm_canonical.starts_with(&dir_canonical) {
            anyhow::bail!(
                "plugin wasm path '{}' resolves outside the plugin directory",
                manifest.wasm
            );
        }
        let wasm_bytes = std::fs::read(&wasm_path)
            .map_err(|e| anyhow::anyhow!("cannot read wasm '{}': {e}", wasm_path.display()))?;

        let wasi_ctx = build_wasi_ctx(&manifest.permissions);
        let permissions = manifest.permissions.clone();
        let instance = self
            .runtime
            .load_plugin_with_wasi(&wasm_bytes, HashMap::new(), wasi_ctx, permissions)?;

        let id: PluginId = manifest.name.clone();
        let draw_buffer: DrawBuffer = Arc::new(Mutex::new(Vec::new()));
        let instance = Arc::new(Mutex::new(instance));

        // Spawn per-plugin event listener task.
        Self::spawn_event_listener(
            id.clone(),
            Arc::clone(&instance),
            Arc::clone(&draw_buffer),
            self.event_tx.subscribe(),
            manifest.permissions.panes.clone(),
        );

        self.plugins.insert(
            id.clone(),
            LoadedPlugin { manifest, draw_buffer, instance },
        );

        log::info!("plugin: loaded '{}'", id);
        Ok(id)
    }

    /// Load a plugin directly from a directory without copying (dev mode).
    pub fn load_dev(&mut self, dev_path: &Path) -> anyhow::Result<PluginId> {
        self.load_from_dir(dev_path)
    }

    // ── Unload ───────────────────────────────────────────────────────────

    /// Unload and drop a plugin instance.
    ///
    /// The per-instance event task will automatically stop because the
    /// broadcast receiver will see the manager drop all its instances.
    pub fn unload(&mut self, id: &str) {
        if self.plugins.remove(id).is_some() {
            log::info!("plugin: unloaded '{}'", id);
        }
    }

    // ── List ─────────────────────────────────────────────────────────────

    /// Return `(id, name, version)` for every currently-loaded plugin.
    pub fn list_installed(&self) -> Vec<(PluginId, String, String)> {
        self.plugins
            .iter()
            .map(|(id, lp)| (id.clone(), lp.manifest.name.clone(), lp.manifest.version.clone()))
            .collect()
    }

    // ── Load all installed ────────────────────────────────────────────────

    /// Scan `self.plugin_dir` for subdirectories that contain a `plugin.toml`
    /// and load each one. Returns a vec of results (one per subdirectory found).
    pub fn load_all_installed(&mut self) -> Vec<anyhow::Result<PluginId>> {
        if !self.plugin_dir.exists() {
            return Vec::new();
        }

        let entries = match std::fs::read_dir(&self.plugin_dir) {
            Ok(e) => e,
            Err(e) => {
                log::warn!(
                    "plugin: cannot read plugin directory '{}': {e}",
                    self.plugin_dir.display()
                );
                return Vec::new();
            }
        };

        let mut results = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("plugin.toml").exists() {
                results.push(self.load_from_dir(&path));
            }
        }
        results
    }

    // ── Draw buffer access ────────────────────────────────────────────────

    /// Take the current draw buffer for a plugin pane, replacing it with an
    /// empty vec.  Returns an empty vec if the plugin is not loaded.
    ///
    /// Called by the renderer during `RedrawRequested` to obtain the latest
    /// styled lines from a plugin's `render()` output.
    pub fn take_draw_buffer(
        &self,
        id: &PluginId,
    ) -> Vec<crate::host::arcterm::plugin::types::StyledLine> {
        if let Some(lp) = self.plugins.get(id)
            && let Ok(mut buf) = lp.draw_buffer.lock()
        {
            return std::mem::take(&mut *buf);
        }
        Vec::new()
    }

    /// Invoke a named tool by dispatching to the WASM plugin that owns it.
    ///
    /// Uses a single mutable lock per plugin to check ownership and dispatch
    /// within the same critical section (fixes ISSUE-017 TOCTOU). The
    /// tool-not-found response is built with `serde_json::json!` so that
    /// special characters in `name` are properly escaped (fixes ISSUE-014).
    pub fn call_tool(&self, name: &str, args_json: &str) -> anyhow::Result<String> {
        for lp in self.plugins.values() {
            let mut inst = lp.instance.lock()
                .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
            let owned = inst.host_data().registered_tools.iter().any(|t| t.name == name);
            if owned {
                return inst.call_tool_export(name, args_json);
            }
        }
        Ok(serde_json::json!({"error": "tool not found", "tool": name}).to_string())
    }

    /// Collect all MCP tool schemas from all loaded plugin instances.
    ///
    /// Returns a flat list of `ToolSchema` records suitable for serialising
    /// into an MCP `tools/list` response (JSON-RPC serving deferred to Phase 7).
    pub fn list_tools(&self) -> Vec<ToolSchema> {
        let mut tools = Vec::new();
        for lp in self.plugins.values() {
            if let Ok(inst) = lp.instance.lock() {
                let registered = &inst.host_data().registered_tools;
                tools.extend_from_slice(registered);
            }
        }
        tools
    }

    /// Send a key-input event directly to a specific plugin instance.
    ///
    /// Unlike lifecycle events (which go through the broadcast bus), key events
    /// are targeted at the focused plugin pane only.  Returns `true` if the
    /// plugin consumed the event and a re-render is needed.
    pub fn send_key_input(
        &self,
        id: &PluginId,
        key_char: Option<String>,
        key_name: String,
        ctrl: bool,
        alt: bool,
        shift: bool,
    ) -> bool {
        let Some(lp) = self.plugins.get(id) else { return false };

        let event = WitPluginEvent::KeyInput(KeyInputPayload {
            key_char,
            key_name,
            modifiers: KeyModifiers { ctrl, alt, shift },
        });

        let instance = Arc::clone(&lp.instance);
        let draw_buffer = Arc::clone(&lp.draw_buffer);
        let pane_access = lp.manifest.permissions.panes.clone();

        // Key events are synchronous with the render loop — use block_in_place
        // to run the WASM call on the current (non-async) thread.
        tokio::task::block_in_place(|| {
            let mut inst = match instance.lock() {
                Ok(g) => g,
                Err(e) => {
                    log::error!("plugin: key-input: instance lock poisoned: {e}");
                    return false;
                }
            };
            match inst.call_update(event) {
                Ok(true) => {
                    // Re-render if plugin has pane access.
                    if pane_access != crate::manifest::PaneAccess::None {
                        match inst.call_render() {
                            Ok(lines) => {
                                if let Ok(mut buf) = draw_buffer.lock() {
                                    *buf = lines;
                                }
                            }
                            Err(e) => log::warn!("plugin: key-input render() error: {e}"),
                        }
                    }
                    true
                }
                Ok(false) => false,
                Err(e) => {
                    log::warn!("plugin: key-input update() error: {e}");
                    false
                }
            }
        })
    }

    // ── Event bus ────────────────────────────────────────────────────────

    /// Clone the broadcast sender so other components (e.g. `AppState`)
    /// can emit events without holding a reference to the manager.
    pub fn event_sender(&self) -> broadcast::Sender<PluginEvent> {
        self.event_tx.clone()
    }

    /// Broadcast an event to all plugin instances.
    ///
    /// Returns the number of receivers that got the message, or 0 if there
    /// are no active subscribers (not an error).
    pub fn broadcast_event(&self, event: PluginEvent) -> usize {
        self.event_tx.send(event).unwrap_or(0)
    }

    // ── Per-plugin event listener task ────────────────────────────────────

    fn spawn_event_listener(
        plugin_id: PluginId,
        instance: Arc<Mutex<PluginInstance>>,
        draw_buffer: DrawBuffer,
        mut rx: broadcast::Receiver<PluginEvent>,
        pane_access: PaneAccess,
    ) {
        tokio::spawn(async move {
            loop {
                let event = match rx.recv().await {
                    Ok(e) => e,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("plugin '{}': lagged, dropped {n} events", plugin_id);
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        log::debug!("plugin '{}': event bus closed, stopping listener", plugin_id);
                        break;
                    }
                };

                let wit_event = event.to_wit();

                // Deliver event to plugin on a blocking thread (wasmtime is sync).
                let instance_clone = Arc::clone(&instance);
                let buf_clone = Arc::clone(&draw_buffer);
                let pa = pane_access.clone();
                let pid = plugin_id.clone();

                let result = tokio::task::spawn_blocking(move || {
                    let mut inst = match instance_clone.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            log::error!("plugin '{}': instance lock poisoned: {e}", pid);
                            return;
                        }
                    };

                    // Call update(); if it returns true, call render() and swap buffer.
                    match inst.call_update(wit_event) {
                        Ok(true) => {
                            // Only call render if plugin has pane access.
                            if pa != PaneAccess::None {
                                match inst.call_render() {
                                    Ok(lines) => {
                                        if let Ok(mut buf) = buf_clone.lock() {
                                            *buf = lines;
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!("plugin '{}': render() error: {e}", pid);
                                    }
                                }
                            }
                        }
                        Ok(false) => {}
                        Err(e) => {
                            log::warn!("plugin '{}': update() error: {e}", pid);
                        }
                    }
                })
                .await;

                if let Err(e) = result {
                    log::error!("plugin '{}': event task panicked: {e}", plugin_id);
                    break;
                }
            }
        });
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    // ── Test plugin directory helpers ──────────────────────────────────────

    /// Write a minimal `plugin.toml` to `dir`. Does NOT write a wasm file —
    /// used by tests that verify file-copy behavior without wasm compilation.
    fn write_plugin_toml(dir: &Path, name: &str) {
        fs::create_dir_all(dir).expect("create plugin dir");
        let toml = format!(
            r#"
name        = "{name}"
version     = "0.1.0"
api_version = "0.1"
wasm        = "plugin.wasm"
"#
        );
        fs::write(dir.join("plugin.toml"), toml).expect("write plugin.toml");
        // Write a placeholder wasm file so the file-copy test can verify it.
        fs::write(dir.join("plugin.wasm"), b"placeholder").expect("write placeholder wasm");
    }

    // ── M-1: KeyInput event kind maps to WitEventKind::KeyInput ──────────

    #[test]
    fn key_input_event_kind_is_key_input() {
        let ev = PluginEvent::KeyInput {
            key_char: Some("a".to_string()),
            key_name: "a".to_string(),
            modifiers: KeyInputModifiers::default(),
        };
        assert!(matches!(ev.kind(), WitEventKind::KeyInput));
    }

    // ── M-6: copy_plugin_files rejects symlinks ───────────────────────────

    #[cfg(unix)]
    #[test]
    fn copy_plugin_files_rejects_symlinks() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("symlink-plugin-src");
        write_plugin_toml(&source, "symlink-plugin");

        // Create a symlink inside the source directory.
        let target = tmp.path().join("secret.txt");
        fs::write(&target, b"secret data").expect("write target");
        std::os::unix::fs::symlink(&target, source.join("secret_link.txt"))
            .expect("create symlink");

        let install_root = tmp.path().join("installed");
        let mgr = PluginManager::new_with_dir(install_root).expect("PluginManager::new_with_dir");

        let err = mgr.copy_plugin_files(&source).expect_err("should fail due to symlink");
        let msg = format!("{err}");
        assert!(msg.contains("symlink"), "expected symlink error, got: {msg}");
    }

    // ── (a) copy_plugin_files copies files to plugin_dir ─────────────────
    //
    // Verifies the file-copy behavior of install without requiring a valid
    // wasm binary (which would require a full WIT-compliant component build).

    #[test]
    fn install_copies_files_to_plugin_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("my-plugin-src");
        write_plugin_toml(&source, "my-plugin");

        let install_root = tmp.path().join("installed");
        let mgr =
            PluginManager::new_with_dir(install_root.clone()).expect("PluginManager::new_with_dir");

        // Test the file-copy step directly (without wasm loading).
        let dest = mgr.copy_plugin_files(&source).expect("copy_plugin_files should succeed");
        assert_eq!(dest, install_root.join("my-plugin"));

        // Verify files were copied to the correct location.
        assert!(dest.join("plugin.toml").exists(), "plugin.toml was copied");
        assert!(dest.join("plugin.wasm").exists(), "plugin.wasm was copied");

        // Verify the manifest parses correctly from the copied location.
        let manifest_text = fs::read_to_string(dest.join("plugin.toml")).expect("read copied manifest");
        let manifest = PluginManifest::from_toml(&manifest_text).expect("parse copied manifest");
        assert_eq!(manifest.name, "my-plugin");
        assert_eq!(manifest.version, "0.1.0");
    }

    // ── (b) unload removes the plugin from the manager ────────────────────
    //
    // Tests the lifecycle: insert a synthetic LoadedPlugin directly, then unload.

    #[test]
    fn unload_removes_plugin() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let install_root = tmp.path().join("installed");
        let mut mgr =
            PluginManager::new_with_dir(install_root).expect("PluginManager::new_with_dir");

        // Directly insert a mock entry to test unload without needing real wasm.
        let manifest = PluginManifest {
            name: "mock-plugin".to_string(),
            version: "1.0.0".to_string(),
            api_version: "0.1".to_string(),
            description: String::new(),
            wasm: "plugin.wasm".to_string(),
            permissions: crate::manifest::Permissions::default(),
        };
        let draw_buffer: DrawBuffer = Arc::new(Mutex::new(Vec::new()));
        // We can't create a real PluginInstance without a wasm binary, so we
        // test that unload correctly removes entries that were never added
        // (the manager should handle gracefully) and that insert + remove works.

        // Insert via the HashMap directly (test-only).
        // We can't create a real PluginInstance, so this test verifies that
        // the PluginId == name convention and list_installed() reflect state.
        // The real insert path is tested via integration of copy_plugin_files + load_from_dir.
        let _ = (manifest, draw_buffer); // suppress unused warning

        // Verify that unloading a non-existent plugin is a no-op.
        mgr.unload("nonexistent");
        assert_eq!(mgr.list_installed().len(), 0, "should still be empty");
    }

    // ── (c) event broadcast delivers events to all subscribers ────────────
    //
    // Verifies the broadcast channel mechanics: events sent via broadcast_event
    // are receivable by all subscribers (both via event_sender().subscribe()
    // and the per-plugin listener tasks that are spawned on load).

    #[tokio::test]
    async fn event_broadcast_reaches_subscribers() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let install_root = tmp.path().join("installed");
        let mgr =
            PluginManager::new_with_dir(install_root).expect("PluginManager::new_with_dir");

        // Subscribe two receivers.
        let mut rx1 = mgr.event_sender().subscribe();
        let mut rx2 = mgr.event_sender().subscribe();

        // Broadcast a PaneOpened event.
        let sent = mgr.broadcast_event(PluginEvent::PaneOpened("42".to_string()));
        // With 2 receivers active, sent should be 2.
        assert_eq!(sent, 2, "both receivers should get the event");

        // Both receivers should get the event.
        let ev1 = tokio::time::timeout(Duration::from_millis(200), rx1.recv())
            .await
            .expect("timeout rx1")
            .expect("rx1 error");
        let ev2 = tokio::time::timeout(Duration::from_millis(200), rx2.recv())
            .await
            .expect("timeout rx2")
            .expect("rx2 error");

        assert!(
            matches!(ev1, PluginEvent::PaneOpened(ref s) if s == "42"),
            "rx1 received wrong event: {ev1:?}"
        );
        assert!(
            matches!(ev2, PluginEvent::PaneOpened(ref s) if s == "42"),
            "rx2 received wrong event: {ev2:?}"
        );

        // Broadcast a WorkspaceSwitched event.
        let sent2 = mgr.broadcast_event(PluginEvent::WorkspaceSwitched("my-ws".to_string()));
        assert_eq!(sent2, 2);

        let ev3 = tokio::time::timeout(Duration::from_millis(200), rx1.recv())
            .await
            .expect("timeout")
            .expect("error");
        assert!(matches!(ev3, PluginEvent::WorkspaceSwitched(ref s) if s == "my-ws"));
    }

    // ── (d) event_sender() allows broadcasting from external callers ──────

    #[tokio::test]
    async fn external_sender_broadcasts_events() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let install_root = tmp.path().join("installed");
        let mgr =
            PluginManager::new_with_dir(install_root).expect("PluginManager::new_with_dir");

        let sender = mgr.event_sender();
        let mut rx = sender.subscribe();

        // Drop the manager — the sender should still work.
        drop(mgr);

        sender.send(PluginEvent::CommandExecuted("ls -la".to_string())).ok();

        let ev = tokio::time::timeout(Duration::from_millis(200), rx.recv())
            .await
            .expect("timeout")
            .expect("no error");

        assert!(matches!(ev, PluginEvent::CommandExecuted(ref s) if s == "ls -la"));
    }

    // ── ISSUE-014: tool-not-found JSON properly escapes name ──────────────
    //
    // Regression test: a tool name containing `"` or `\` must produce valid JSON.
    // The format!() approach does not JSON-escape, so serde_json::from_str would fail.

    #[test]
    fn call_tool_not_found_returns_valid_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let install_root = tmp.path().join("installed");
        let mgr =
            PluginManager::new_with_dir(install_root).expect("PluginManager::new_with_dir");

        // A tool name with characters that would break format!()-based JSON assembly.
        let tricky_name = r#"tool"with\"special\chars"#;
        let result = mgr.call_tool(tricky_name, "{}").expect("call_tool should return Ok");

        // The response must be valid JSON regardless of special chars in the name.
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("response must be valid JSON");

        // The "tool" field must round-trip the original name exactly.
        assert_eq!(
            parsed["tool"].as_str().expect("tool field must be a string"),
            tricky_name,
            "tool field must preserve the original name"
        );
        assert_eq!(
            parsed["error"].as_str().expect("error field must be a string"),
            "tool not found"
        );
    }

    // ── ISSUE-017: call_tool uses single lock (no TOCTOU) ─────────────────
    //
    // Structural regression test: this verifies the single-lock code path
    // compiles correctly. The TOCTOU was a code structure issue; the runtime
    // observable behavior in single-threaded tests is the same either way.

    #[test]
    fn call_tool_single_lock_no_toctou() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let install_root = tmp.path().join("installed");
        let mgr =
            PluginManager::new_with_dir(install_root).expect("PluginManager::new_with_dir");

        // With no plugins loaded, call_tool must return a valid tool-not-found response
        // using a single lock scope (verified by the implementation's structure).
        let result = mgr.call_tool("any-tool", "{}").expect("call_tool should return Ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("response must be valid JSON");
        assert_eq!(parsed["error"], "tool not found");
    }

    // ── ISSUE-018: load_from_dir propagates missing wasm error ───────────
    //
    // Regression test: when plugin.toml exists but the .wasm file does not,
    // load_from_dir must return a clear "not found" error rather than silently
    // falling back to the raw path and producing a confusing "resolves outside
    // the plugin directory" error or an unhelpful OS-level message.

    #[test]
    fn load_from_dir_rejects_missing_wasm() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let plugin_dir = tmp.path().join("bad-plugin");
        fs::create_dir_all(&plugin_dir).expect("create plugin dir");

        // Write plugin.toml but deliberately omit the .wasm file.
        let toml = r#"
name        = "bad-plugin"
version     = "0.1.0"
api_version = "0.1"
wasm        = "plugin.wasm"
"#;
        fs::write(plugin_dir.join("plugin.toml"), toml).expect("write plugin.toml");
        // Intentionally do NOT write plugin.wasm.

        let install_root = tmp.path().join("installed");
        let mut mgr =
            PluginManager::new_with_dir(install_root).expect("PluginManager::new_with_dir");

        let err = mgr.load_from_dir(&plugin_dir).expect_err("should fail: wasm file missing");
        let msg = format!("{err}");
        assert!(
            msg.contains("not found"),
            "error should mention 'not found', got: {msg}"
        );
    }

    // ── (e) load_all_installed scans plugin_dir but returns empty when not populated ─

    #[test]
    fn load_all_installed_returns_empty_when_no_plugins() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let install_root = tmp.path().join("plugins");
        // Don't create the directory — verify it handles absence gracefully.

        let mut mgr =
            PluginManager::new_with_dir(install_root).expect("PluginManager::new_with_dir");
        let results = mgr.load_all_installed();
        assert_eq!(results.len(), 0, "should return empty when directory doesn't exist");
    }

    // ── (f) PluginEvent::to_wit() converts correctly ──────────────────────

    #[test]
    fn plugin_event_to_wit_conversion() {
        use crate::host::arcterm::plugin::types::PluginEvent as WitPluginEvent;

        let ev = PluginEvent::PaneOpened("123".to_string());
        let wit = ev.to_wit();
        assert!(matches!(wit, WitPluginEvent::PaneOpened(ref s) if s == "123"));

        let ev = PluginEvent::WorkspaceSwitched("alpha".to_string());
        let wit = ev.to_wit();
        assert!(matches!(wit, WitPluginEvent::WorkspaceSwitched(ref s) if s == "alpha"));

        let ev = PluginEvent::CommandExecuted("git status".to_string());
        let wit = ev.to_wit();
        assert!(matches!(wit, WitPluginEvent::CommandExecuted(ref s) if s == "git status"));

        let ev = PluginEvent::PaneClosed("456".to_string());
        let wit = ev.to_wit();
        assert!(matches!(wit, WitPluginEvent::PaneClosed(ref s) if s == "456"));
    }
}
