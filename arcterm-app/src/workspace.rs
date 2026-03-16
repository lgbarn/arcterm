//! Workspace file data model, serialization, and I/O for Arcterm.
//!
//! A workspace file captures a session's pane layout, per-pane configuration,
//! window dimensions, and workspace-level environment variables as a
//! human-readable TOML file.
//!
//! The types here are pure serialization DTOs — they are intentionally
//! decoupled from the live runtime types (`PaneNode`, `PaneId`) so that
//! session-specific state (opaque `PaneId(u64)` counters) does not leak into
//! persisted files.

// Public API consumed by downstream CLI subcommands (Phase 5 Wave 2+).
#![allow(dead_code)]

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout::{PaneId, PaneNode};
use crate::tab::TabManager;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when loading or saving workspace files.
#[derive(Debug)]
pub enum WorkspaceError {
    /// An underlying I/O error (file not found, permissions, etc.).
    IoError(io::Error),
    /// The TOML content could not be parsed.
    TomlParseError(String),
    /// The workspace file's `schema_version` is not supported by this build.
    UnsupportedVersion(u32),
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkspaceError::IoError(e) => write!(f, "I/O error: {e}"),
            WorkspaceError::TomlParseError(s) => write!(f, "TOML parse error: {s}"),
            WorkspaceError::UnsupportedVersion(v) => {
                write!(
                    f,
                    "unsupported workspace schema version {v} (this build supports version 1)"
                )
            }
        }
    }
}

impl From<io::Error> for WorkspaceError {
    fn from(e: io::Error) -> Self {
        WorkspaceError::IoError(e)
    }
}

// ---------------------------------------------------------------------------
// Workspace metadata structs
// ---------------------------------------------------------------------------

/// Top-level workspace file, serialized as TOML.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceFile {
    /// Schema version — must be `1` for this implementation.
    pub schema_version: u32,

    /// Workspace identity and root directory.
    pub workspace: WorkspaceMeta,

    /// Window physical dimensions at save time (optional).
    pub window: Option<WindowState>,

    /// The pane layout tree.
    pub layout: WorkspacePaneNode,

    /// Workspace-level environment variable overrides.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub environment: HashMap<String, String>,
}

impl Default for WorkspaceFile {
    fn default() -> Self {
        WorkspaceFile {
            schema_version: 1,
            workspace: WorkspaceMeta {
                name: "default".to_string(),
                directory: None,
            },
            window: None,
            layout: WorkspacePaneNode::Leaf {
                command: None,
                directory: None,
                env: None,
            },
            environment: HashMap::new(),
        }
    }
}

/// Workspace identity: name and optional root directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceMeta {
    /// Human-readable workspace name (used for the file name and UI).
    pub name: String,
    /// Root directory for the workspace. `~` is expanded on load.
    pub directory: Option<String>,
}

/// Physical window dimensions in pixels, captured at save time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    /// Window width in physical pixels.
    pub width: u32,
    /// Window height in physical pixels.
    pub height: u32,
}

// ---------------------------------------------------------------------------
// WorkspacePaneNode — serialization DTO for the pane tree
// ---------------------------------------------------------------------------

/// A node in the serialized pane tree.
///
/// Uses `#[serde(tag = "type", rename_all = "lowercase")]` so the TOML
/// output reads `type = "leaf"`, `type = "hsplit"`, etc., rather than the
/// externally-tagged form, which hits TOML 1.0 inline-table nesting limits
/// for recursive structures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WorkspacePaneNode {
    /// A terminal leaf pane.
    Leaf {
        /// The command to run in this pane (e.g. `"nvim ."`, `"cargo watch"`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<String>,
        /// The working directory for this pane.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        directory: Option<String>,
        /// Per-pane environment overrides.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        env: Option<HashMap<String, String>>,
    },
    /// A horizontal split (left | right).
    HSplit {
        /// Fraction [0.0, 1.0] of width given to the left child.
        ratio: f32,
        /// Left child sub-tree.
        left: Box<WorkspacePaneNode>,
        /// Right child sub-tree.
        right: Box<WorkspacePaneNode>,
    },
    /// A vertical split (top / bottom).
    VSplit {
        /// Fraction [0.0, 1.0] of height given to the top child.
        ratio: f32,
        /// Top child sub-tree.
        top: Box<WorkspacePaneNode>,
        /// Bottom child sub-tree.
        bottom: Box<WorkspacePaneNode>,
    },
}

// ---------------------------------------------------------------------------
// File system helpers
// ---------------------------------------------------------------------------

/// Returns the directory where workspace TOML files are stored.
///
/// Resolves to `<config_dir>/arcterm/workspaces/`, e.g.
/// `~/.config/arcterm/workspaces/` on Linux/macOS.
pub fn workspaces_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arcterm")
        .join("workspaces")
}

// ---------------------------------------------------------------------------
// WorkspaceFile — I/O methods
// ---------------------------------------------------------------------------

impl WorkspaceFile {
    /// Serialize this workspace to TOML and write it atomically to `path`.
    ///
    /// Writes to a `.tmp` sibling first, then renames (POSIX atomic rename).
    pub fn save_to_file(&self, path: &Path) -> io::Result<()> {
        let toml_str = toml::to_string_pretty(self).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("TOML serialize error: {e}"))
        })?;

        // Ensure parent directory exists.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write to a .tmp sibling.
        let tmp_path = path.with_extension("toml.tmp");
        std::fs::write(&tmp_path, toml_str.as_bytes())?;

        // Atomic rename.
        std::fs::rename(&tmp_path, path)?;

        Ok(())
    }

    /// Load and validate a workspace file from `path`.
    ///
    /// Returns `WorkspaceError::UnsupportedVersion` if `schema_version != 1`.
    pub fn load_from_file(path: &Path) -> Result<Self, WorkspaceError> {
        let text = std::fs::read_to_string(path)?;
        let ws: WorkspaceFile =
            toml::from_str(&text).map_err(|e| WorkspaceError::TomlParseError(e.to_string()))?;

        if ws.schema_version != 1 {
            return Err(WorkspaceError::UnsupportedVersion(ws.schema_version));
        }

        Ok(ws)
    }
}

// ---------------------------------------------------------------------------
// PaneMetadata — per-pane runtime metadata for capture/restore
// ---------------------------------------------------------------------------

/// Runtime metadata for a single leaf pane, used during session capture.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneMetadata {
    /// The command running in this pane (e.g. `"nvim ."`, `None` for shell).
    pub command: Option<String>,
    /// Current working directory of this pane, captured at save time.
    pub directory: Option<String>,
    /// Per-pane environment variable overrides.
    pub env: Option<HashMap<String, String>>,
}

// ---------------------------------------------------------------------------
// Conversion: live PaneNode <-> WorkspacePaneNode
// ---------------------------------------------------------------------------

impl WorkspacePaneNode {
    /// Walk a live `PaneNode` tree and produce a `WorkspacePaneNode` DTO,
    /// looking up per-leaf metadata from `pane_metadata`.
    pub fn from_pane_tree(
        tree: &PaneNode,
        pane_metadata: &HashMap<PaneId, PaneMetadata>,
    ) -> Self {
        match tree {
            PaneNode::Leaf { pane_id } => {
                let meta = pane_metadata.get(pane_id);
                WorkspacePaneNode::Leaf {
                    command: meta.and_then(|m| m.command.clone()),
                    directory: meta.and_then(|m| m.directory.clone()),
                    env: meta.and_then(|m| m.env.clone()),
                }
            }
            // Plugin panes are serialised as plain leaves in workspace files.
            // The plugin_id is not preserved across sessions (plugins are reloaded
            // at startup independently of the workspace layout).
            PaneNode::PluginPane { pane_id, .. } => {
                let meta = pane_metadata.get(pane_id);
                WorkspacePaneNode::Leaf {
                    command: meta.and_then(|m| m.command.clone()),
                    directory: meta.and_then(|m| m.directory.clone()),
                    env: meta.and_then(|m| m.env.clone()),
                }
            }
            PaneNode::HSplit { ratio, left, right } => WorkspacePaneNode::HSplit {
                ratio: *ratio,
                left: Box::new(WorkspacePaneNode::from_pane_tree(left, pane_metadata)),
                right: Box::new(WorkspacePaneNode::from_pane_tree(right, pane_metadata)),
            },
            PaneNode::VSplit { ratio, top, bottom } => WorkspacePaneNode::VSplit {
                ratio: *ratio,
                top: Box::new(WorkspacePaneNode::from_pane_tree(top, pane_metadata)),
                bottom: Box::new(WorkspacePaneNode::from_pane_tree(bottom, pane_metadata)),
            },
        }
    }

    /// Produce a live `PaneNode` tree with freshly allocated `PaneId` values,
    /// and return the per-leaf metadata in left-to-right / top-to-bottom order.
    pub fn to_pane_tree(&self) -> (PaneNode, Vec<PaneMetadata>) {
        let mut metadata = Vec::new();
        let node = self.build_pane_node(&mut metadata);
        (node, metadata)
    }

    fn build_pane_node(&self, metadata: &mut Vec<PaneMetadata>) -> PaneNode {
        match self {
            WorkspacePaneNode::Leaf { command, directory, env } => {
                let id = PaneId::next();
                metadata.push(PaneMetadata {
                    command: command.clone(),
                    directory: directory.clone(),
                    env: env.clone(),
                });
                PaneNode::Leaf { pane_id: id }
            }
            WorkspacePaneNode::HSplit { ratio, left, right } => PaneNode::HSplit {
                ratio: *ratio,
                left: Box::new(left.build_pane_node(metadata)),
                right: Box::new(right.build_pane_node(metadata)),
            },
            WorkspacePaneNode::VSplit { ratio, top, bottom } => PaneNode::VSplit {
                ratio: *ratio,
                top: Box::new(top.build_pane_node(metadata)),
                bottom: Box::new(bottom.build_pane_node(metadata)),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// capture_session — full session snapshot
// ---------------------------------------------------------------------------

/// Capture the current session state into a `WorkspaceFile`.
///
/// Uses the active tab's layout tree. Multi-tab workspace support is deferred.
pub fn capture_session(
    tab_manager: &TabManager,
    pane_metadata: &HashMap<PaneId, PaneMetadata>,
    name: &str,
    window_size: Option<(u32, u32)>,
) -> WorkspaceFile {
    let active_tab = tab_manager.active_tab();
    let layout = WorkspacePaneNode::from_pane_tree(&active_tab.layout, pane_metadata);

    WorkspaceFile {
        schema_version: 1,
        workspace: WorkspaceMeta {
            name: name.to_string(),
            directory: None,
        },
        window: window_size.map(|(w, h)| WindowState { width: w, height: h }),
        layout,
        environment: HashMap::new(),
    }
}

// ---------------------------------------------------------------------------
// discover_workspaces — produce WorkspaceEntry vec for the switcher UI
// ---------------------------------------------------------------------------

/// Scan the workspaces directory and return a [`Vec<WorkspaceEntry>`] suitable
/// for use in the workspace switcher overlay.
///
/// Skips `.toml` files whose stem starts with `_` (auto-save/reserved files).
/// Results are sorted alphabetically by name.  Returns an empty `Vec` if the
/// directory does not exist.
pub fn discover_workspaces() -> Vec<crate::palette::WorkspaceEntry> {
    let dir = workspaces_dir();
    let read_dir = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }

        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        if stem.starts_with('_') {
            continue;
        }

        results.push(crate::palette::WorkspaceEntry { name: stem, path });
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results
}

// ---------------------------------------------------------------------------
// list_workspaces — scan workspaces directory
// ---------------------------------------------------------------------------

/// List workspace files in `workspaces_dir()`.
///
/// Returns `(name, path)` pairs for every `.toml` file whose name does NOT
/// start with `_` (underscore-prefixed files are reserved for auto-save).
///
/// The name is the file stem (e.g. `"my-project"` for `my-project.toml`).
pub fn list_workspaces() -> Vec<(String, PathBuf)> {
    let dir = workspaces_dir();
    let read_dir = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();

        // Must be a .toml file.
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }

        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        // Skip _-prefixed reserved files (auto-save).
        if stem.starts_with('_') {
            continue;
        }

        results.push((stem, path));
    }

    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // -----------------------------------------------------------------------
    // Task 1 — TOML round-trip tests
    // -----------------------------------------------------------------------

    fn leaf(command: Option<&str>, directory: Option<&str>) -> WorkspacePaneNode {
        WorkspacePaneNode::Leaf {
            command: command.map(str::to_string),
            directory: directory.map(str::to_string),
            env: None,
        }
    }

    #[test]
    fn round_trip_single_leaf() {
        let ws = WorkspaceFile {
            schema_version: 1,
            workspace: WorkspaceMeta {
                name: "test".to_string(),
                directory: Some("/home/user/test".to_string()),
            },
            window: None,
            layout: leaf(None, None),
            environment: HashMap::new(),
        };

        let toml_str = toml::to_string_pretty(&ws).expect("serialize");
        let ws2: WorkspaceFile = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(ws, ws2);
    }

    #[test]
    fn round_trip_4_pane_layout() {
        // HSplit(Leaf, VSplit(Leaf, Leaf)) — 3-level nesting
        let ws = WorkspaceFile {
            schema_version: 1,
            workspace: WorkspaceMeta {
                name: "dev".to_string(),
                directory: Some("/Users/dev/projects/myapp".to_string()),
            },
            window: Some(WindowState { width: 1920, height: 1080 }),
            layout: WorkspacePaneNode::HSplit {
                ratio: 0.6,
                left: Box::new(leaf(Some("nvim ."), Some("/Users/dev/projects/myapp"))),
                right: Box::new(WorkspacePaneNode::VSplit {
                    ratio: 0.5,
                    top: Box::new(leaf(
                        Some("cargo watch -x test"),
                        Some("/Users/dev/projects/myapp"),
                    )),
                    bottom: Box::new(leaf(None, None)),
                }),
            },
            environment: HashMap::new(),
        };

        let toml_str = toml::to_string_pretty(&ws).expect("serialize");
        let ws2: WorkspaceFile = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(ws, ws2);
    }

    #[test]
    fn round_trip_with_environment() {
        let mut env = HashMap::new();
        env.insert("KUBECONFIG".to_string(), "/home/.kube/prod".to_string());
        env.insert("RUST_LOG".to_string(), "debug".to_string());
        env.insert("NODE_ENV".to_string(), "development".to_string());

        let ws = WorkspaceFile {
            schema_version: 1,
            workspace: WorkspaceMeta { name: "env-test".to_string(), directory: None },
            window: None,
            layout: leaf(None, None),
            environment: env.clone(),
        };

        let toml_str = toml::to_string_pretty(&ws).expect("serialize");
        let ws2: WorkspaceFile = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(ws.environment, ws2.environment);
        assert_eq!(ws2.environment.len(), 3);
    }

    #[test]
    fn toml_output_is_human_readable() {
        let ws = WorkspaceFile {
            schema_version: 1,
            workspace: WorkspaceMeta {
                name: "readable".to_string(),
                directory: Some("/home/user".to_string()),
            },
            window: None,
            layout: WorkspacePaneNode::HSplit {
                ratio: 0.5,
                left: Box::new(leaf(Some("nvim ."), None)),
                right: Box::new(leaf(None, None)),
            },
            environment: HashMap::new(),
        };

        let toml_str = toml::to_string_pretty(&ws).expect("serialize");

        assert!(
            toml_str.contains("[workspace]"),
            "TOML must contain [workspace] section\n---\n{toml_str}"
        );
        assert!(
            toml_str.contains("[layout]"),
            "TOML must contain [layout] section\n---\n{toml_str}"
        );
        assert!(
            toml_str.contains("command = "),
            "TOML must contain plain-text 'command = ' key\n---\n{toml_str}"
        );
    }

    #[test]
    fn schema_version_mismatch_returns_error() {
        let toml_str = r#"
schema_version = 99

[workspace]
name = "future-workspace"

[layout]
type = "leaf"
"#;
        let tmp = tempfile_for_test("schema_mismatch.toml", toml_str);
        let result = WorkspaceFile::load_from_file(&tmp);
        match result {
            Err(WorkspaceError::UnsupportedVersion(99)) => {}
            other => panic!("expected UnsupportedVersion(99), got {other:?}"),
        }
        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn default_workspace_file_round_trips() {
        let ws = WorkspaceFile::default();
        let toml_str = toml::to_string_pretty(&ws).expect("serialize");
        let ws2: WorkspaceFile = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(ws, ws2);
        assert_eq!(ws2.schema_version, 1);
    }

    #[test]
    fn workspaces_dir_contains_arcterm() {
        let dir = workspaces_dir();
        let s = dir.to_string_lossy();
        assert!(
            s.contains("arcterm"),
            "workspaces_dir() must contain 'arcterm': {s}"
        );
        assert!(
            s.ends_with("workspaces"),
            "workspaces_dir() must end with 'workspaces': {s}"
        );
    }

    // -----------------------------------------------------------------------
    // Task 2 — Conversion function tests
    // -----------------------------------------------------------------------

    #[test]
    fn from_pane_tree_single_leaf() {
        let id = PaneId::next();
        let tree = PaneNode::Leaf { pane_id: id };
        let mut meta = HashMap::new();
        meta.insert(
            id,
            PaneMetadata {
                command: Some("zsh".to_string()),
                directory: Some("/home".to_string()),
                env: None,
            },
        );

        let ws_node = WorkspacePaneNode::from_pane_tree(&tree, &meta);
        match ws_node {
            WorkspacePaneNode::Leaf { command, directory, .. } => {
                assert_eq!(command.as_deref(), Some("zsh"));
                assert_eq!(directory.as_deref(), Some("/home"));
            }
            other => panic!("expected Leaf, got {other:?}"),
        }
    }

    #[test]
    fn from_pane_tree_nested() {
        // Build HSplit(Leaf_a, VSplit(Leaf_b, Leaf_c))
        let a = PaneId::next();
        let b = PaneId::next();
        let c = PaneId::next();
        let tree = PaneNode::HSplit {
            ratio: 0.6,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::VSplit {
                ratio: 0.4,
                top: Box::new(PaneNode::Leaf { pane_id: b }),
                bottom: Box::new(PaneNode::Leaf { pane_id: c }),
            }),
        };

        let mut meta = HashMap::new();
        meta.insert(
            a,
            PaneMetadata { command: Some("nvim".to_string()), directory: None, env: None },
        );
        meta.insert(
            b,
            PaneMetadata {
                command: Some("cargo test".to_string()),
                directory: None,
                env: None,
            },
        );
        meta.insert(c, PaneMetadata { command: None, directory: None, env: None });

        let ws_node = WorkspacePaneNode::from_pane_tree(&tree, &meta);

        // Verify top-level is HSplit with correct ratio.
        match &ws_node {
            WorkspacePaneNode::HSplit { ratio, left, right } => {
                assert!((ratio - 0.6).abs() < 1e-5, "ratio should be 0.6");
                assert!(
                    matches!(left.as_ref(), WorkspacePaneNode::Leaf { command, .. } if command.as_deref() == Some("nvim"))
                );
                match right.as_ref() {
                    WorkspacePaneNode::VSplit { ratio: r2, top, .. } => {
                        assert!((r2 - 0.4).abs() < 1e-5);
                        assert!(
                            matches!(top.as_ref(), WorkspacePaneNode::Leaf { command, .. } if command.as_deref() == Some("cargo test"))
                        );
                    }
                    other => panic!("expected VSplit on right, got {other:?}"),
                }
            }
            other => panic!("expected HSplit at root, got {other:?}"),
        }
    }

    #[test]
    fn to_pane_tree_assigns_fresh_ids() {
        let ws_node = WorkspacePaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(leaf(Some("nvim"), None)),
            right: Box::new(leaf(None, None)),
        };

        let (tree, _meta) = ws_node.to_pane_tree();
        let ids = tree.all_pane_ids();
        assert_eq!(ids.len(), 2, "two leaves should produce two PaneIds");

        // All IDs must be unique and nonzero.
        for id in &ids {
            assert!(id.0 > 0, "PaneId must be nonzero, got {:?}", id);
        }
        let mut seen = std::collections::HashSet::new();
        for id in &ids {
            assert!(seen.insert(id.0), "PaneIds must be unique, duplicate: {:?}", id);
        }
    }

    #[test]
    fn to_pane_tree_preserves_ratios() {
        let ws_node = WorkspacePaneNode::HSplit {
            ratio: 0.7,
            left: Box::new(leaf(None, None)),
            right: Box::new(leaf(None, None)),
        };

        let (tree, _meta) = ws_node.to_pane_tree();
        match tree {
            PaneNode::HSplit { ratio, .. } => {
                assert!((ratio - 0.7).abs() < 1e-5, "ratio must be preserved, got {ratio}");
            }
            other => panic!("expected HSplit, got {other:?}"),
        }
    }

    #[test]
    fn round_trip_pane_tree() {
        // Build an original PaneNode tree.
        let a = PaneId::next();
        let b = PaneId::next();
        let c = PaneId::next();
        let original = PaneNode::HSplit {
            ratio: 0.55,
            left: Box::new(PaneNode::Leaf { pane_id: a }),
            right: Box::new(PaneNode::VSplit {
                ratio: 0.3,
                top: Box::new(PaneNode::Leaf { pane_id: b }),
                bottom: Box::new(PaneNode::Leaf { pane_id: c }),
            }),
        };

        let meta = HashMap::new(); // no metadata — all Leafs get None fields

        // Convert to DTO.
        let ws_node = WorkspacePaneNode::from_pane_tree(&original, &meta);

        // Convert back to live tree.
        let (restored, restored_meta) = ws_node.to_pane_tree();

        // Structure must match (ratios), PaneId values will differ.
        match (&original, &restored) {
            (
                PaneNode::HSplit { ratio: r1, right: orig_right, .. },
                PaneNode::HSplit { ratio: r2, right: rest_right, .. },
            ) => {
                assert!((r1 - r2).abs() < 1e-5, "HSplit ratio must be preserved");
                match (orig_right.as_ref(), rest_right.as_ref()) {
                    (
                        PaneNode::VSplit { ratio: vr1, .. },
                        PaneNode::VSplit { ratio: vr2, .. },
                    ) => {
                        assert!((vr1 - vr2).abs() < 1e-5, "VSplit ratio must be preserved");
                    }
                    other => panic!("right child structure mismatch: {other:?}"),
                }
            }
            other => panic!("root structure mismatch: {other:?}"),
        }

        // 3 leaves in the original → 3 entries in metadata.
        assert_eq!(restored_meta.len(), 3);
    }

    #[test]
    fn capture_session_produces_valid_workspace_file() {
        let initial_id = PaneId::next();
        let tab_manager = TabManager::new(initial_id);

        let meta = HashMap::new();
        let ws = capture_session(&tab_manager, &meta, "my-project", Some((1920, 1080)));

        assert_eq!(ws.schema_version, 1);
        assert_eq!(ws.workspace.name, "my-project");
        assert_eq!(ws.window, Some(WindowState { width: 1920, height: 1080 }));
        assert!(matches!(ws.layout, WorkspacePaneNode::Leaf { .. }));
    }

    // -----------------------------------------------------------------------
    // PLAN-3.2 Task 2 — Performance benchmark and edge-case tests
    // -----------------------------------------------------------------------

    /// Parse a 4-pane workspace TOML string and assert that the parse
    /// completes in under 1 millisecond.
    ///
    /// Layout: HSplit(Leaf, VSplit(Leaf, VSplit(Leaf, Leaf)))
    #[test]
    fn workspace_toml_parse_under_1ms() {
        use std::time::Instant;

        let toml_str = r#"
schema_version = 1

[workspace]
name = "four-pane-perf"
directory = "/tmp"

[window]
width = 1920
height = 1080

[layout]
type = "hsplit"
ratio = 0.5

[layout.left]
type = "leaf"
directory = "/tmp"

[layout.right]
type = "vsplit"
ratio = 0.5

[layout.right.top]
type = "leaf"
directory = "/tmp"

[layout.right.bottom]
type = "vsplit"
ratio = 0.5

[layout.right.bottom.top]
type = "leaf"
directory = "/tmp"

[layout.right.bottom.bottom]
type = "leaf"
directory = "/tmp"
"#;

        let start = Instant::now();
        let ws: WorkspaceFile = toml::from_str(toml_str).expect("parse must succeed");
        let elapsed = start.elapsed();

        // Verify the structure parsed correctly.
        assert_eq!(ws.workspace.name, "four-pane-perf");
        assert!(
            matches!(ws.layout, WorkspacePaneNode::HSplit { .. }),
            "root must be HSplit"
        );

        // Parse must complete under 1ms.
        assert!(
            elapsed.as_millis() < 1,
            "TOML parse took {}us, must be < 1ms",
            elapsed.as_micros()
        );
    }

    /// A workspace with a single Leaf layout serializes and deserializes
    /// correctly, producing exactly one pane node on restore.
    #[test]
    fn workspace_file_with_no_panes_defaults_to_single_leaf() {
        let ws = WorkspaceFile {
            schema_version: 1,
            workspace: WorkspaceMeta {
                name: "single-leaf".to_string(),
                directory: None,
            },
            window: None,
            layout: WorkspacePaneNode::Leaf {
                command: None,
                directory: None,
                env: None,
            },
            environment: HashMap::new(),
        };

        let toml_str = toml::to_string_pretty(&ws).expect("serialize");
        let ws2: WorkspaceFile = toml::from_str(&toml_str).expect("deserialize");

        // The restored workspace must have exactly one pane (a Leaf).
        let (tree, metadata) = ws2.layout.to_pane_tree();
        let ids = tree.all_pane_ids();
        assert_eq!(ids.len(), 1, "single Leaf must restore to exactly one pane");
        assert_eq!(metadata.len(), 1, "single Leaf must produce exactly one PaneMetadata");
        assert!(
            matches!(tree, crate::layout::PaneNode::Leaf { .. }),
            "restored tree must be a Leaf"
        );
    }

    /// A workspace with a tilde in the directory field preserves the tilde
    /// as a literal string — expansion is the caller's responsibility at
    /// restore time.
    #[test]
    fn workspace_with_tilde_in_directory() {
        let ws = WorkspaceFile {
            schema_version: 1,
            workspace: WorkspaceMeta {
                name: "tilde-test".to_string(),
                directory: Some("~/projects/test".to_string()),
            },
            window: None,
            layout: WorkspacePaneNode::Leaf {
                command: None,
                directory: Some("~/projects/test".to_string()),
                env: None,
            },
            environment: HashMap::new(),
        };

        let toml_str = toml::to_string_pretty(&ws).expect("serialize");
        let ws2: WorkspaceFile = toml::from_str(&toml_str).expect("deserialize");

        // The tilde must be preserved as a literal string after round-trip.
        assert_eq!(
            ws2.workspace.directory.as_deref(),
            Some("~/projects/test"),
            "workspace directory tilde must round-trip as a literal string"
        );
        match &ws2.layout {
            WorkspacePaneNode::Leaf { directory, .. } => {
                assert_eq!(
                    directory.as_deref(),
                    Some("~/projects/test"),
                    "pane directory tilde must round-trip as a literal string"
                );
            }
            other => panic!("expected Leaf layout, got {other:?}"),
        }
    }

    /// A workspace with an empty environment HashMap serializes and deserializes
    /// correctly — the empty map must round-trip (not become a missing field).
    ///
    /// Note: `#[serde(skip_serializing_if = "HashMap::is_empty")]` means the
    /// field is omitted when empty during serialization; on deserialization the
    /// `#[serde(default)]` attribute restores it as an empty map. This test
    /// verifies the semantic round-trip (empty-in, empty-out) holds.
    #[test]
    fn workspace_with_empty_environment() {
        let ws = WorkspaceFile {
            schema_version: 1,
            workspace: WorkspaceMeta {
                name: "empty-env".to_string(),
                directory: None,
            },
            window: None,
            layout: WorkspacePaneNode::Leaf {
                command: None,
                directory: None,
                env: None,
            },
            environment: HashMap::new(),
        };

        let toml_str = toml::to_string_pretty(&ws).expect("serialize");
        let ws2: WorkspaceFile = toml::from_str(&toml_str).expect("deserialize");

        // An empty environment must round-trip as an empty map (not None or missing).
        assert_eq!(
            ws2.environment.len(),
            0,
            "empty environment must deserialize as an empty HashMap, got {:?}",
            ws2.environment
        );
    }

    /// A deeply nested tree (4 levels, 8 leaves) serializes to TOML and
    /// deserializes back to an equal value. Stress-tests the serde enum
    /// representation for recursive WorkspacePaneNode structures.
    #[test]
    fn workspace_large_tree_round_trips() {
        // Build a tree with 4 levels of nesting and 8 leaves.
        fn make_leaf() -> WorkspacePaneNode {
            WorkspacePaneNode::Leaf {
                command: Some("zsh".to_string()),
                directory: Some("/tmp".to_string()),
                env: None,
            }
        }

        let tree = WorkspacePaneNode::HSplit {
            ratio: 0.5,
            left: Box::new(WorkspacePaneNode::VSplit {
                ratio: 0.5,
                top: Box::new(make_leaf()),
                bottom: Box::new(make_leaf()),
            }),
            right: Box::new(WorkspacePaneNode::VSplit {
                ratio: 0.5,
                top: Box::new(WorkspacePaneNode::HSplit {
                    ratio: 0.5,
                    left: Box::new(make_leaf()),
                    right: Box::new(make_leaf()),
                }),
                bottom: Box::new(WorkspacePaneNode::HSplit {
                    ratio: 0.5,
                    left: Box::new(WorkspacePaneNode::VSplit {
                        ratio: 0.5,
                        top: Box::new(make_leaf()),
                        bottom: Box::new(make_leaf()),
                    }),
                    right: Box::new(WorkspacePaneNode::VSplit {
                        ratio: 0.5,
                        top: Box::new(make_leaf()),
                        bottom: Box::new(make_leaf()),
                    }),
                }),
            }),
        };

        let ws = WorkspaceFile {
            schema_version: 1,
            workspace: WorkspaceMeta {
                name: "large-tree".to_string(),
                directory: None,
            },
            window: None,
            layout: tree.clone(),
            environment: HashMap::new(),
        };

        let toml_str = toml::to_string_pretty(&ws).expect("serialize large tree");
        let ws2: WorkspaceFile = toml::from_str(&toml_str).expect("deserialize large tree");

        // The full layout tree must survive the TOML round-trip intact.
        assert_eq!(
            ws.layout, ws2.layout,
            "large tree must survive TOML round-trip with all nodes equal"
        );

        // Verify all 8 leaves are recoverable via to_pane_tree.
        let (_, metadata) = ws2.layout.to_pane_tree();
        assert_eq!(
            metadata.len(),
            8,
            "8-leaf tree must produce 8 PaneMetadata entries, got {}",
            metadata.len()
        );
    }

    // -----------------------------------------------------------------------
    // Task 3 — list_workspaces tests
    // -----------------------------------------------------------------------

    #[test]
    fn list_finds_toml_files() {
        let dir = tempdir_for_test("list_toml");
        // Create two workspace files.
        std::fs::write(dir.join("alpha.toml"), b"").ok();
        std::fs::write(dir.join("beta.toml"), b"").ok();
        // Non-toml file must be ignored.
        std::fs::write(dir.join("notes.txt"), b"").ok();

        let results = list_workspaces_in(&dir);
        let names: Vec<&str> = results.iter().map(|(n, _)| n.as_str()).collect();

        assert!(names.contains(&"alpha"), "alpha.toml must appear in listing");
        assert!(names.contains(&"beta"), "beta.toml must appear in listing");
        assert!(!names.contains(&"notes"), "notes.txt must be filtered out");

        cleanup_tempdir(&dir);
    }

    #[test]
    fn list_ignores_underscore_prefixed_files() {
        let dir = tempdir_for_test("list_underscore");
        std::fs::write(dir.join("my-workspace.toml"), b"").ok();
        std::fs::write(dir.join("_autosave.toml"), b"").ok();
        std::fs::write(dir.join("_session.toml"), b"").ok();

        let results = list_workspaces_in(&dir);
        let names: Vec<&str> = results.iter().map(|(n, _)| n.as_str()).collect();

        assert!(names.contains(&"my-workspace"), "user workspace must appear");
        assert!(!names.contains(&"_autosave"), "_autosave must be filtered");
        assert!(!names.contains(&"_session"), "_session must be filtered");

        cleanup_tempdir(&dir);
    }

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Write `content` to a temp file and return its path.
    fn tempfile_for_test(name: &str, content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("arcterm_test_{name}"));
        std::fs::write(&path, content.as_bytes()).expect("write temp file");
        path
    }

    /// Create a temporary directory for list_workspaces tests.
    fn tempdir_for_test(suffix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("arcterm_ws_test_{suffix}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn cleanup_tempdir(dir: &Path) {
        std::fs::remove_dir_all(dir).ok();
    }

    /// Variant of `list_workspaces` that operates on a given directory (for
    /// hermetic testing without touching the real config dir).
    fn list_workspaces_in(dir: &Path) -> Vec<(String, PathBuf)> {
        let read_dir = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::new();
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            if stem.starts_with('_') {
                continue;
            }
            results.push((stem, path));
        }
        results.sort_by(|a, b| a.0.cmp(&b.0));
        results
    }

    // -----------------------------------------------------------------------
    // PLAN-2.2 Task 3 — auto-save path and filtering tests
    // -----------------------------------------------------------------------

    /// `_last_session.toml` must be filtered out of the workspace listing.
    /// Only user-named workspaces (no leading `_`) should appear.
    #[test]
    fn list_workspaces_skips_underscore_files() {
        let dir = tempdir_for_test("plan22_skip_underscore");
        // Write the auto-save file and one user workspace.
        std::fs::write(dir.join("_last_session.toml"), b"").ok();
        std::fs::write(dir.join("my-project.toml"), b"").ok();

        let results = list_workspaces_in(&dir);
        let names: Vec<&str> = results.iter().map(|(n, _)| n.as_str()).collect();

        assert!(
            names.contains(&"my-project"),
            "my-project.toml must appear in listing, got: {names:?}"
        );
        assert!(
            !names.contains(&"_last_session"),
            "_last_session.toml must be excluded from listing, got: {names:?}"
        );

        cleanup_tempdir(&dir);
    }

    /// The auto-save path must be inside the arcterm workspaces directory
    /// and use the reserved `_last_session.toml` filename.
    #[test]
    fn last_session_path_is_in_workspaces_dir() {
        let path = workspaces_dir().join("_last_session.toml");
        let s = path.to_string_lossy();

        assert!(
            s.contains("arcterm"),
            "_last_session path must contain 'arcterm': {s}"
        );
        assert!(
            s.contains("workspaces"),
            "_last_session path must contain 'workspaces': {s}"
        );
        assert!(
            s.ends_with("_last_session.toml"),
            "_last_session path must end with '_last_session.toml': {s}"
        );
    }

    /// `WorkspaceFile::save_to_file` must create parent directories if they
    /// do not exist (the workspaces dir may not exist on first run).
    #[test]
    fn save_to_file_creates_parent_dirs() {
        let base = tempdir_for_test("plan22_parent_dirs");
        // Nested directory that does not yet exist.
        let nested = base.join("a").join("b").join("c");
        let target = nested.join("session.toml");

        let ws = WorkspaceFile::default();
        ws.save_to_file(&target).expect("save_to_file must create parent dirs");

        assert!(
            target.exists(),
            "workspace file must exist at {target:?} after save"
        );

        cleanup_tempdir(&base);
    }

    // -----------------------------------------------------------------------
    // Task 3 (PLAN-3.1) — discover_workspaces tests
    // -----------------------------------------------------------------------

    /// Helper: run discover_workspaces logic against a custom directory.
    fn discover_workspaces_in(dir: &Path) -> Vec<crate::palette::WorkspaceEntry> {
        let read_dir = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::new();
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            if stem.starts_with('_') {
                continue;
            }
            results.push(crate::palette::WorkspaceEntry { name: stem, path });
        }
        results.sort_by(|a, b| a.name.cmp(&b.name));
        results
    }

    #[test]
    fn discover_finds_toml_files_as_entries() {
        let dir = tempdir_for_test("discover_toml");
        std::fs::write(dir.join("alpha.toml"), b"").ok();
        std::fs::write(dir.join("beta.toml"), b"").ok();
        std::fs::write(dir.join("notes.txt"), b"").ok();

        let results = discover_workspaces_in(&dir);
        let names: Vec<&str> = results.iter().map(|e| e.name.as_str()).collect();

        assert!(names.contains(&"alpha"), "alpha.toml must appear");
        assert!(names.contains(&"beta"), "beta.toml must appear");
        assert!(!names.contains(&"notes"), "notes.txt must be filtered out");

        cleanup_tempdir(&dir);
    }

    #[test]
    fn discover_ignores_underscore_prefixed_files() {
        let dir = tempdir_for_test("discover_underscore");
        std::fs::write(dir.join("my-workspace.toml"), b"").ok();
        std::fs::write(dir.join("_autosave.toml"), b"").ok();
        std::fs::write(dir.join("_session.toml"), b"").ok();

        let results = discover_workspaces_in(&dir);
        let names: Vec<&str> = results.iter().map(|e| e.name.as_str()).collect();

        assert!(names.contains(&"my-workspace"));
        assert!(!names.contains(&"_autosave"), "_autosave must be filtered");
        assert!(!names.contains(&"_session"), "_session must be filtered");

        cleanup_tempdir(&dir);
    }

    #[test]
    fn discover_returns_empty_for_nonexistent_directory() {
        let dir = std::path::PathBuf::from("/tmp/arcterm_nonexistent_dir_xyz_12345");
        let results = discover_workspaces_in(&dir);
        assert!(results.is_empty(), "nonexistent dir must return empty vec");
    }

    #[test]
    fn discover_sorts_alphabetically() {
        let dir = tempdir_for_test("discover_sort");
        std::fs::write(dir.join("zebra.toml"), b"").ok();
        std::fs::write(dir.join("alpha.toml"), b"").ok();
        std::fs::write(dir.join("mango.toml"), b"").ok();

        let results = discover_workspaces_in(&dir);
        let names: Vec<&str> = results.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mango", "zebra"], "must be sorted alphabetically");

        cleanup_tempdir(&dir);
    }

    #[test]
    fn discover_entry_path_points_to_toml_file() {
        let dir = tempdir_for_test("discover_path");
        std::fs::write(dir.join("myws.toml"), b"").ok();

        let results = discover_workspaces_in(&dir);
        assert_eq!(results.len(), 1);
        assert!(results[0].path.to_string_lossy().ends_with("myws.toml"));

        cleanup_tempdir(&dir);
    }
}
