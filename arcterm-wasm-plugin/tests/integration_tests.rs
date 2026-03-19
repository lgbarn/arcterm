//! Integration tests for the WASM plugin system.
//!
//! These tests verify the complete plugin loading pipeline,
//! capability enforcement, and lifecycle management.

use arcterm_wasm_plugin::capability::{
    Capability, CapabilityOperation, CapabilityResource, CapabilitySet,
};
use arcterm_wasm_plugin::config::WasmPluginConfig;
use arcterm_wasm_plugin::lifecycle::{PluginManager, PluginState};

// ── Plugin Manager Tests ──────────────────────────────────────────────────────

#[test]
fn test_plugin_manager_loads_enabled_plugins() {
    let mut manager = PluginManager::new();
    let configs = vec![WasmPluginConfig {
        name: "test-plugin".to_string(),
        path: "/nonexistent/path.wasm".to_string(),
        capabilities: vec!["terminal:read".to_string()],
        enabled: true,
        ..Default::default()
    }];
    manager.load_all(configs);
    // Should have 1 plugin (failed because file doesn't exist, but it tried)
    assert_eq!(manager.plugins().len(), 1);
    assert!(matches!(
        manager.plugins()[0].state,
        PluginState::Failed(_)
    ));
}

#[test]
fn test_plugin_manager_skips_disabled_plugins() {
    let mut manager = PluginManager::new();
    let configs = vec![WasmPluginConfig {
        name: "disabled-plugin".to_string(),
        path: "/nonexistent/path.wasm".to_string(),
        enabled: false,
        ..Default::default()
    }];
    manager.load_all(configs);
    // Disabled plugins are skipped entirely
    assert_eq!(manager.plugins().len(), 0);
}

#[test]
fn test_plugin_manager_isolates_failures() {
    let mut manager = PluginManager::new();
    let configs = vec![
        WasmPluginConfig {
            name: "plugin-a".to_string(),
            path: "/nonexistent/a.wasm".to_string(),
            enabled: true,
            ..Default::default()
        },
        WasmPluginConfig {
            name: "plugin-b".to_string(),
            path: "/nonexistent/b.wasm".to_string(),
            enabled: true,
            ..Default::default()
        },
    ];
    manager.load_all(configs);
    // Both should be attempted, both fail (no files), but manager handles it
    assert_eq!(manager.plugins().len(), 2);
    for plugin in manager.plugins() {
        assert!(matches!(plugin.state, PluginState::Failed(_)));
    }
}

#[test]
fn test_plugin_manager_shutdown() {
    let mut manager = PluginManager::new();
    // No plugins loaded — shutdown should be a no-op
    manager.shutdown_all();
    assert_eq!(manager.running_count(), 0);
}

// ── Capability Enforcement Tests ──────────────────────────────────────────────

#[test]
fn test_capability_set_terminal_read_always_granted() {
    let set = CapabilitySet::new(vec![]);
    let required = Capability {
        resource: CapabilityResource::Terminal,
        operation: CapabilityOperation::Read,
        target: None,
    };
    assert!(set.check(&required).is_ok());
}

#[test]
fn test_capability_set_terminal_write_denied_by_default() {
    let set = CapabilitySet::new(vec![]);
    let required = Capability {
        resource: CapabilityResource::Terminal,
        operation: CapabilityOperation::Write,
        target: None,
    };
    assert!(set.check(&required).is_err());
}

#[test]
fn test_capability_set_fs_path_prefix_enforcement() {
    let caps = vec![Capability::parse("fs:read:/home/user/project").unwrap()];
    let set = CapabilitySet::new(caps);

    // Within granted path
    let allowed = Capability {
        resource: CapabilityResource::Filesystem,
        operation: CapabilityOperation::Read,
        target: Some("/home/user/project/src/main.rs".to_string()),
    };
    assert!(set.check(&allowed).is_ok());

    // Outside granted path
    let denied = Capability {
        resource: CapabilityResource::Filesystem,
        operation: CapabilityOperation::Read,
        target: Some("/etc/shadow".to_string()),
    };
    assert!(set.check(&denied).is_err());

    // Write not granted (only read)
    let write_denied = Capability {
        resource: CapabilityResource::Filesystem,
        operation: CapabilityOperation::Write,
        target: Some("/home/user/project/file.txt".to_string()),
    };
    assert!(set.check(&write_denied).is_err());
}

#[test]
fn test_capability_set_network_enforcement() {
    let caps = vec![Capability::parse("net:connect:api.example.com:443").unwrap()];
    let set = CapabilitySet::new(caps);

    // Matching host:port
    let allowed = Capability {
        resource: CapabilityResource::Network,
        operation: CapabilityOperation::Connect,
        target: Some("api.example.com:443".to_string()),
    };
    assert!(set.check(&allowed).is_ok());

    // Different host
    let denied = Capability {
        resource: CapabilityResource::Network,
        operation: CapabilityOperation::Connect,
        target: Some("evil.com:443".to_string()),
    };
    assert!(set.check(&denied).is_err());
}

// ── Config Parsing Tests ──────────────────────────────────────────────────────

#[test]
fn test_config_parses_valid_capabilities() {
    let config = WasmPluginConfig {
        name: "test".to_string(),
        capabilities: vec![
            "terminal:read".to_string(),
            "fs:read:/tmp".to_string(),
            "net:connect:localhost:8080".to_string(),
        ],
        ..Default::default()
    };
    let caps = config.parse_capabilities();
    assert_eq!(caps.len(), 3);
}

#[test]
fn test_config_skips_invalid_capabilities() {
    let config = WasmPluginConfig {
        name: "test".to_string(),
        capabilities: vec![
            "terminal:read".to_string(),
            "invalid".to_string(),           // bad format
            "fs:read".to_string(),            // missing path
            "net:connect:host:443".to_string(), // valid
        ],
        ..Default::default()
    };
    let caps = config.parse_capabilities();
    // Only terminal:read and net:connect should parse
    assert_eq!(caps.len(), 2);
}

// ── Loader Tests ──────────────────────────────────────────────────────────────

#[test]
fn test_loader_rejects_nonexistent_file() {
    let engine = arcterm_wasm_plugin::loader::create_engine().unwrap();
    let config = WasmPluginConfig {
        name: "missing".to_string(),
        path: "/definitely/not/a/real/path.wasm".to_string(),
        ..Default::default()
    };
    let result = arcterm_wasm_plugin::loader::load_plugin(&engine, &config);
    assert!(result.is_err());
    assert!(format!("{:#}", result.err().unwrap()).contains("failed to read"));
}

#[test]
fn test_loader_rejects_invalid_wasm() {
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(b"this is not wasm").unwrap();

    let engine = arcterm_wasm_plugin::loader::create_engine().unwrap();
    let config = WasmPluginConfig {
        name: "invalid".to_string(),
        path: tmp.path().to_str().unwrap().to_string(),
        ..Default::default()
    };
    let result = arcterm_wasm_plugin::loader::load_plugin(&engine, &config);
    assert!(result.is_err());
}
