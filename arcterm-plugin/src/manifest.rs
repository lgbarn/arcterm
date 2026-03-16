//! Plugin manifest parsing and permission enforcement.
//!
//! Reads `plugin.toml` from a plugin directory, validates it, and builds
//! a sandboxed `WasiCtx` based on the declared permissions.

use serde::Deserialize;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};

// ──────────────────────────────────────────────────────────────────
// Manifest types
// ──────────────────────────────────────────────────────────────────

/// The only API version this runtime accepts.
pub const SUPPORTED_API_VERSION: &str = "0.1";

/// Access level a plugin may have to terminal panes.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PaneAccess {
    /// No access to pane output or rendering.
    #[default]
    None,
    /// Can read pane content but not write.
    Read,
    /// Can read and write pane content.
    Write,
}

/// Permission block declared in `plugin.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Permissions {
    /// Host filesystem paths the plugin may read/write.
    ///
    /// Each entry is preopened as a directory in the WASI context.
    /// Default: empty (no filesystem access).
    pub filesystem: Vec<String>,

    /// Whether the plugin may open network sockets.
    ///
    /// Default: false (no network access).
    pub network: bool,

    /// Pane access level granted to the plugin.
    ///
    /// Default: `PaneAccess::None`.
    pub panes: PaneAccess,

    /// Whether the plugin may register MCP tools.
    ///
    /// Default: false.
    pub ai: bool,
}

/// Parsed contents of a `plugin.toml` file.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    /// Human-readable plugin name (must be non-empty).
    pub name: String,

    /// SemVer-style plugin version string (e.g. `"1.0.0"`).
    pub version: String,

    /// API version this plugin targets (must equal `"0.1"`).
    pub api_version: String,

    /// Optional human-readable description.
    #[serde(default)]
    pub description: String,

    /// Relative path to the compiled `.wasm` file inside the plugin directory.
    pub wasm: String,

    /// Permission declarations.
    #[serde(default)]
    pub permissions: Permissions,
}

impl PluginManifest {
    /// Parse a `plugin.toml` from its raw TOML text.
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Validate a parsed manifest for well-formedness.
    ///
    /// Checks:
    /// - `name` is non-empty.
    /// - `name` does not contain path traversal characters (`/`, `\`, `..`)
    ///   and does not start with `.` or represent an absolute path.
    /// - `wasm` path is non-empty.
    /// - `api_version` equals `"0.1"`.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("plugin name must not be empty".to_string());
        }

        // Reject plugin names that could escape the install directory.
        // Names must be simple identifiers: no path separators, no parent
        // directory components, no leading dots, and not absolute paths.
        if self.name.contains('/') || self.name.contains('\\') {
            return Err(format!(
                "plugin name '{}' must not contain path separators ('/' or '\\')",
                self.name
            ));
        }
        if self.name.contains("..") {
            return Err(format!(
                "plugin name '{}' must not contain '..'",
                self.name
            ));
        }
        if self.name.starts_with('.') {
            return Err(format!(
                "plugin name '{}' must not start with '.'",
                self.name
            ));
        }
        // Reject absolute paths (Unix '/' prefix or Windows drive letters like "C:").
        {
            let p = std::path::Path::new(&self.name);
            if p.is_absolute() {
                return Err(format!(
                    "plugin name '{}' must not be an absolute path",
                    self.name
                ));
            }
        }

        if self.wasm.trim().is_empty() {
            return Err("plugin wasm path must not be empty".to_string());
        }
        if self.wasm.contains("..") {
            return Err(format!("plugin wasm path '{}' must not contain '..'", self.wasm));
        }
        if self.wasm.starts_with('/') || self.wasm.starts_with('\\') {
            return Err(format!("plugin wasm path '{}' must not be an absolute path", self.wasm));
        }
        if self.wasm.contains('\\') {
            return Err(format!("plugin wasm path '{}' must not contain backslashes", self.wasm));
        }
        if self.api_version != SUPPORTED_API_VERSION {
            return Err(format!(
                "unsupported api_version '{}': this runtime requires '{}'",
                self.api_version, SUPPORTED_API_VERSION,
            ));
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────
// WASI context builder
// ──────────────────────────────────────────────────────────────────

/// Build a sandboxed [`WasiCtx`] from the plugin's declared permissions.
///
/// - Each path in `permissions.filesystem` is preopened as a read/write directory.
///   Paths that do not exist on the host are silently skipped (the plugin will
///   simply not see them rather than failing to load).
/// - `network = false` (the default) means no socket preopening — WASI's
///   capability model means the plugin cannot open any sockets at all.
/// - `panes` and `ai` are enforced at the host-function level in `host.rs`,
///   not here.
pub fn build_wasi_ctx(permissions: &Permissions) -> WasiCtx {
    use wasmtime_wasi::{DirPerms, FilePerms};

    let mut builder = WasiCtxBuilder::new();
    // Do NOT call builder.inherit_stdio() — plugins communicate exclusively
    // via the WIT interface (render-text, log, etc.).  Exposing raw host
    // stdin/stdout/stderr to every plugin, regardless of permissions, would
    // be a sandbox escape.

    for path_str in &permissions.filesystem {
        let path = std::path::Path::new(path_str);
        if path.exists() {
            match builder.preopened_dir(path_str, path_str, DirPerms::all(), FilePerms::all()) {
                Ok(_) => {}
                Err(e) => {
                    log::warn!(
                        "plugin: cannot preopen '{}' for plugin sandbox: {e}",
                        path.display()
                    );
                }
            }
        } else {
            log::debug!(
                "plugin: skipping non-existent filesystem path '{}'",
                path.display()
            );
        }
    }

    // network = false → no socket preopening (default WasiCtxBuilder behaviour).
    // network = true → inherit the host's network access via the WASI socket API.
    if permissions.network {
        builder.inherit_network();
    }

    builder.build()
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── (a) Parse a full manifest with all permission fields ──────────────

    #[test]
    fn manifest_parse_full() {
        let toml = r#"
            name        = "my-plugin"
            version     = "1.2.3"
            api_version = "0.1"
            description = "A test plugin."
            wasm        = "plugin.wasm"

            [permissions]
            filesystem = ["/tmp", "/home/user"]
            network    = true
            panes      = "write"
            ai         = true
        "#;

        let manifest = PluginManifest::from_toml(toml).expect("valid TOML");

        assert_eq!(manifest.name, "my-plugin");
        assert_eq!(manifest.version, "1.2.3");
        assert_eq!(manifest.api_version, "0.1");
        assert_eq!(manifest.description, "A test plugin.");
        assert_eq!(manifest.wasm, "plugin.wasm");

        let p = &manifest.permissions;
        assert_eq!(p.filesystem, vec!["/tmp", "/home/user"]);
        assert!(p.network);
        assert_eq!(p.panes, PaneAccess::Write);
        assert!(p.ai);
    }

    // ── (b) Minimal manifest — permissions section absent ─────────────────

    #[test]
    fn manifest_parse_minimal_defaults() {
        let toml = r#"
            name        = "bare"
            version     = "0.1.0"
            api_version = "0.1"
            wasm        = "bare.wasm"
        "#;

        let manifest = PluginManifest::from_toml(toml).expect("valid TOML");

        assert_eq!(manifest.name, "bare");
        assert_eq!(manifest.description, "");

        let p = &manifest.permissions;
        assert!(p.filesystem.is_empty(), "filesystem should default to empty");
        assert!(!p.network, "network should default to false");
        assert_eq!(p.panes, PaneAccess::None, "panes should default to None");
        assert!(!p.ai, "ai should default to false");
    }

    // ── (c) validate() rejects api_version "0.2" ─────────────────────────

    #[test]
    fn validate_rejects_wrong_api_version() {
        let toml = r#"
            name        = "future-plugin"
            version     = "1.0.0"
            api_version = "0.2"
            wasm        = "future.wasm"
        "#;
        let manifest = PluginManifest::from_toml(toml).expect("parses fine");
        let err = manifest.validate().expect_err("should fail validation");
        assert!(
            err.contains("0.2"),
            "error should mention the bad api_version: {err}"
        );
    }

    // ── (d) validate() rejects empty name ────────────────────────────────

    #[test]
    fn validate_rejects_empty_name() {
        let toml = r#"
            name        = ""
            version     = "1.0.0"
            api_version = "0.1"
            wasm        = "plugin.wasm"
        "#;
        let manifest = PluginManifest::from_toml(toml).expect("parses fine");
        let err = manifest.validate().expect_err("should fail validation");
        assert!(
            err.contains("name"),
            "error should mention 'name': {err}"
        );
    }

    // ── (e) build_wasi_ctx with empty filesystem list ────────────────────

    #[test]
    fn build_wasi_ctx_empty_filesystem() {
        let permissions = Permissions {
            filesystem: vec![],
            network: false,
            panes: PaneAccess::None,
            ai: false,
        };

        // Should not panic; just builds a minimal context.
        let _ctx = build_wasi_ctx(&permissions);
    }

    // ── (f) build_wasi_ctx skips non-existent paths gracefully ───────────

    #[test]
    fn build_wasi_ctx_skips_missing_paths() {
        let permissions = Permissions {
            filesystem: vec!["/nonexistent/path/that/should/not/exist".to_string()],
            network: false,
            panes: PaneAccess::None,
            ai: false,
        };

        // Should not panic even with invalid paths.
        let _ctx = build_wasi_ctx(&permissions);
    }

    // ── Security: C1 — path traversal via plugin name ─────────────────────

    fn make_manifest(name: &str) -> PluginManifest {
        PluginManifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            api_version: "0.1".to_string(),
            description: String::new(),
            wasm: "plugin.wasm".to_string(),
            permissions: Permissions::default(),
        }
    }

    #[test]
    fn validate_rejects_name_with_forward_slash() {
        let err = make_manifest("../../.config").validate().expect_err("should reject");
        assert!(err.contains("path separator") || err.contains(".."), "{err}");
    }

    #[test]
    fn validate_rejects_name_with_double_dot() {
        let err = make_manifest("foo..bar").validate().expect_err("should reject ..");
        assert!(err.contains(".."), "{err}");
    }

    #[test]
    fn validate_rejects_name_starting_with_dot() {
        let err = make_manifest(".hidden").validate().expect_err("should reject leading dot");
        assert!(err.contains("'.'"), "{err}");
    }

    #[test]
    fn validate_rejects_name_with_backslash() {
        let err = make_manifest("foo\\bar").validate().expect_err("should reject backslash");
        assert!(err.contains("path separator"), "{err}");
    }

    #[test]
    fn validate_accepts_safe_name() {
        // A plain alphanumeric name with hyphens is always valid.
        make_manifest("my-plugin-v2").validate().expect("safe name must pass");
    }

    fn make_manifest_wasm(wasm: &str) -> PluginManifest {
        PluginManifest {
            name: "my-plugin".to_string(),
            version: "0.1.0".to_string(),
            api_version: "0.1".to_string(),
            description: String::new(),
            wasm: wasm.to_string(),
            permissions: Permissions::default(),
        }
    }

    #[test]
    fn validate_wasm_rejects_path_traversal() {
        let err = make_manifest_wasm("../../evil.wasm").validate().expect_err("should reject ..");
        assert!(err.contains(".."), "{err}");
    }

    #[test]
    fn validate_wasm_rejects_absolute_unix() {
        let err = make_manifest_wasm("/etc/evil.wasm").validate().expect_err("should reject absolute path");
        assert!(err.contains("absolute"), "{err}");
    }

    #[test]
    fn validate_wasm_rejects_backslash() {
        let err = make_manifest_wasm("..\\evil.wasm").validate().expect_err("should reject backslash");
        assert!(err.contains("..") || err.contains("backslash"), "{err}");
    }
}
