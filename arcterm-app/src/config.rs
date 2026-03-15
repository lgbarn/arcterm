//! TOML configuration for Arcterm.
//!
//! Reads `~/.config/arcterm/config.toml` on startup; returns compiled-in
//! defaults when the file is absent or invalid.
//!
//! Hot-reload is available via [`watch_config`], which spawns a background
//! thread that sends a fresh [`ArctermConfig`] whenever the file changes.

use std::path::PathBuf;
use std::sync::mpsc;

use notify::{EventKind, RecursiveMode, Watcher};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Full Arcterm configuration, sourced from `config.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ArctermConfig {
    /// Font family name (empty string means system default monospace).
    pub font_family: String,
    /// Font size in logical pixels / points.
    pub font_size: f32,
    /// Line-height as a multiple of the font size.
    pub line_height_ratio: f32,
    /// Override the shell to launch. `None` means auto-detect via `$SHELL`.
    pub shell: Option<String>,
    /// Maximum number of scrollback lines kept in memory.
    pub scrollback_lines: usize,
    /// Name of the built-in colour scheme to apply.
    pub color_scheme: String,
    /// Cursor shape: "block", "underline", or "beam".
    pub cursor_style: String,
    /// Whether the cursor should blink.
    pub cursor_blink: bool,
    /// Window opacity: 0.0 = fully transparent, 1.0 = fully opaque.
    pub window_opacity: f32,
    /// Inner padding (logical pixels) applied to all four sides.
    pub padding: u32,
    /// Optional overrides for individual palette colours.
    pub colors: ColorOverrides,
    /// Keybinding overrides.
    pub keybindings: KeybindingConfig,
    /// Multiplexer settings (leader key, tab bar, pane borders, navigation).
    #[serde(default)]
    #[allow(dead_code)] // read by Wave-2 keymap and tab-bar rendering
    pub multiplexer: MultiplexerConfig,
}

impl Default for ArctermConfig {
    fn default() -> Self {
        Self {
            font_family: String::new(),
            font_size: 14.0,
            line_height_ratio: 1.4,
            shell: None,
            scrollback_lines: 10_000,
            color_scheme: "catppuccin-mocha".to_string(),
            cursor_style: "block".to_string(),
            cursor_blink: false,
            window_opacity: 1.0,
            padding: 4,
            colors: ColorOverrides::default(),
            keybindings: KeybindingConfig::default(),
            multiplexer: MultiplexerConfig::default(),
        }
    }
}

/// Optional per-slot colour overrides (hex strings, e.g. `"#ff5555"`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ColorOverrides {
    // ANSI 0-7 (normal)
    pub black: Option<String>,
    pub red: Option<String>,
    pub green: Option<String>,
    pub yellow: Option<String>,
    pub blue: Option<String>,
    pub magenta: Option<String>,
    pub cyan: Option<String>,
    pub white: Option<String>,
    // ANSI 8-15 (bright)
    pub bright_black: Option<String>,
    pub bright_red: Option<String>,
    pub bright_green: Option<String>,
    pub bright_yellow: Option<String>,
    pub bright_blue: Option<String>,
    pub bright_magenta: Option<String>,
    pub bright_cyan: Option<String>,
    pub bright_white: Option<String>,
    // Special slots
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub cursor: Option<String>,
}

/// Multiplexer configuration (leader key, tab bar, pane borders, navigation).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MultiplexerConfig {
    /// Leader key chord used to prefix all multiplexer commands (default: "Ctrl+a").
    pub leader_key: String,
    /// How long (ms) to wait for a command key after the leader (default: 500).
    pub leader_timeout_ms: u64,
    /// Whether the tab bar is shown at the top of the window (default: true).
    pub show_tab_bar: bool,
    /// Width of pane divider borders in logical pixels (default: 1.0).
    pub border_width: f32,
    /// Whether Ctrl+h/j/k/l navigate between panes (default: true).
    pub pane_navigation: bool,
}

impl Default for MultiplexerConfig {
    fn default() -> Self {
        Self {
            leader_key: "Ctrl+a".to_string(),
            leader_timeout_ms: 500,
            show_tab_bar: true,
            border_width: 1.0,
            pane_navigation: true,
        }
    }
}

/// Keybinding configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct KeybindingConfig {
    /// Key combination string for copy (default: "Super+C").
    pub copy: String,
    /// Key combination string for paste (default: "Super+V").
    pub paste: String,
}

impl Default for KeybindingConfig {
    fn default() -> Self {
        Self {
            copy: "Super+C".to_string(),
            paste: "Super+V".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

impl ArctermConfig {
    /// Return the canonical path to the config file.
    ///
    /// Resolves to `<config_dir>/arcterm/config.toml` using the platform's
    /// standard configuration directory (e.g. `~/.config/arcterm/config.toml`
    /// on Linux/macOS).
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("arcterm")
            .join("config.toml")
    }

    /// Load configuration from `config_path()`.
    ///
    /// Returns compiled-in defaults when:
    /// - The file does not exist.
    /// - The file cannot be read.
    /// - The file contains invalid TOML.
    /// - A field has an unexpected type (TOML type mismatch).
    ///
    /// Individual fields that are absent from the file also fall back to their
    /// defaults via `#[serde(default)]`.
    pub fn load() -> Self {
        let path = Self::config_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("config: cannot read {}: {e}", path.display());
                }
                return Self::default();
            }
        };

        if text.trim().is_empty() {
            return Self::default();
        }

        match toml::from_str::<Self>(&text) {
            Ok(cfg) => {
                log::info!("config: loaded from {}", path.display());
                cfg
            }
            Err(e) => {
                log::warn!("config: invalid TOML in {}: {e}", path.display());
                Self::default()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Hot-reload
// ---------------------------------------------------------------------------

/// Start watching the config file for changes.
///
/// Spawns a background OS thread that:
/// 1. Watches the parent directory of `config_path()` using the platform's
///    native file-system event API (`notify::recommended_watcher`).
/// 2. On any modify event whose path matches `config.toml`, re-parses the
///    file and sends the new `ArctermConfig` via the returned `mpsc::Receiver`.
///
/// The caller should poll the receiver in its event loop (e.g. in
/// `about_to_wait`) using `try_recv()`.
///
/// If the config directory does not exist yet the watcher will still start;
/// write events on the directory once it is created will be noticed.
///
/// # Errors
///
/// Returns `None` (and logs a warning) if the file-system watcher cannot be
/// created.  All subsequent errors (bad TOML on reload, channel closed) are
/// logged and silently discarded so the app continues running with the last
/// known-good configuration.
pub fn watch_config() -> Option<mpsc::Receiver<ArctermConfig>> {
    let config_path = ArctermConfig::config_path();

    // The directory to watch (parent of config.toml).
    let watch_dir = config_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    // Channel for raw notify events.
    let (event_tx, event_rx) = mpsc::channel::<notify::Result<notify::Event>>();

    // Channel for parsed configs delivered to the app.
    let (cfg_tx, cfg_rx) = mpsc::channel::<ArctermConfig>();

    // Build the watcher; pass the mpsc sender as the EventHandler.
    let mut watcher = match notify::recommended_watcher(event_tx) {
        Ok(w) => w,
        Err(e) => {
            log::warn!("config: cannot create file watcher: {e}");
            return None;
        }
    };

    // Watch the directory (non-recursively; we only care about one file).
    if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
        log::warn!("config: cannot watch {}: {e}", watch_dir.display());
        // Non-fatal: caller still gets a receiver (it will just never fire).
    }

    // Spawn a background thread that forwards parsed configs to the app.
    std::thread::Builder::new()
        .name("config-watcher".to_string())
        .spawn(move || {
            // Keep watcher alive for the lifetime of the thread.
            let _watcher = watcher;

            for result in event_rx {
                let event = match result {
                    Ok(e) => e,
                    Err(e) => {
                        log::warn!("config: watcher error: {e}");
                        continue;
                    }
                };

                // Only react to modify events.
                if !matches!(event.kind, EventKind::Modify(_)) {
                    continue;
                }

                // Check that at least one path in the event matches config.toml.
                let relevant = event.paths.iter().any(|p| {
                    p.file_name()
                        .map(|n| n == "config.toml")
                        .unwrap_or(false)
                });

                if !relevant {
                    continue;
                }

                log::info!("config: detected change, reloading...");
                let new_cfg = ArctermConfig::load();

                if cfg_tx.send(new_cfg).is_err() {
                    // Receiver dropped -- app has exited; stop the thread.
                    break;
                }
            }
        })
        .map_err(|e| log::warn!("config: cannot spawn watcher thread: {e}"))
        .ok();

    Some(cfg_rx)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Default values --------------------------------------------------------

    #[test]
    fn defaults_are_sensible() {
        let cfg = ArctermConfig::default();
        assert_eq!(cfg.font_size, 14.0, "default font_size");
        assert_eq!(cfg.line_height_ratio, 1.4, "default line_height_ratio");
        assert_eq!(cfg.scrollback_lines, 10_000, "default scrollback_lines");
        assert_eq!(cfg.color_scheme, "catppuccin-mocha", "default color_scheme");
        assert_eq!(cfg.cursor_style, "block", "default cursor_style");
        assert!(!cfg.cursor_blink, "default cursor_blink is false");
        assert_eq!(cfg.window_opacity, 1.0, "default window_opacity");
        assert_eq!(cfg.padding, 4, "default padding");
        assert!(cfg.shell.is_none(), "default shell is None");
        assert!(cfg.font_family.is_empty(), "default font_family is empty");
        assert_eq!(cfg.keybindings.copy, "Super+C", "default copy keybinding");
        assert_eq!(cfg.keybindings.paste, "Super+V", "default paste keybinding");
    }

    // -- TOML parsing overrides fields -----------------------------------------

    #[test]
    fn toml_overrides_fields() {
        let toml = r##"
            font_size = 18.0
            line_height_ratio = 1.6
            scrollback_lines = 50000
            color_scheme = "dracula"
            cursor_style = "beam"
            cursor_blink = true
            window_opacity = 0.9
            padding = 8
            shell = "/usr/bin/fish"

            [colors]
            red = "#ff5555"
            foreground = "#f8f8f2"

            [keybindings]
            copy = "Ctrl+Shift+C"
        "##;

        let cfg: ArctermConfig = toml::from_str(toml).expect("valid TOML must parse");

        assert_eq!(cfg.font_size, 18.0);
        assert_eq!(cfg.line_height_ratio, 1.6);
        assert_eq!(cfg.scrollback_lines, 50_000);
        assert_eq!(cfg.color_scheme, "dracula");
        assert_eq!(cfg.cursor_style, "beam");
        assert!(cfg.cursor_blink);
        assert!((cfg.window_opacity - 0.9).abs() < 1e-5);
        assert_eq!(cfg.padding, 8);
        assert_eq!(cfg.shell.as_deref(), Some("/usr/bin/fish"));
        assert_eq!(cfg.colors.red.as_deref(), Some("#ff5555"));
        assert_eq!(cfg.colors.foreground.as_deref(), Some("#f8f8f2"));
        assert_eq!(cfg.keybindings.copy, "Ctrl+Shift+C");
        // Un-overridden keybinding still has default.
        assert_eq!(cfg.keybindings.paste, "Super+V");
    }

    // -- Empty / whitespace-only string returns defaults -----------------------

    #[test]
    fn empty_toml_returns_defaults() {
        // Simulate what ArctermConfig::load() does with empty text.
        let text = "   \n   ";
        let cfg = if text.trim().is_empty() {
            ArctermConfig::default()
        } else {
            toml::from_str(text).unwrap_or_default()
        };

        assert_eq!(cfg.font_size, 14.0);
        assert_eq!(cfg.scrollback_lines, 10_000);
    }

    // -- Invalid TOML returns defaults -----------------------------------------

    #[test]
    fn invalid_toml_returns_defaults() {
        let bad_toml = "this is not [valid toml !!!";
        let cfg: ArctermConfig = toml::from_str(bad_toml).unwrap_or_default();
        // Should fall back to defaults on parse error.
        assert_eq!(cfg.font_size, 14.0);
        assert_eq!(cfg.scrollback_lines, 10_000);
    }

    // -- Partial TOML leaves unset fields at defaults --------------------------

    #[test]
    fn partial_toml_leaves_defaults() {
        let toml = r#"font_size = 16.0"#;
        let cfg: ArctermConfig = toml::from_str(toml).expect("valid TOML");
        assert_eq!(cfg.font_size, 16.0, "overridden field");
        assert_eq!(cfg.scrollback_lines, 10_000, "unset field keeps default");
        assert_eq!(cfg.color_scheme, "catppuccin-mocha", "unset field keeps default");
    }

    // -- config_path() returns a non-empty path --------------------------------

    #[test]
    fn config_path_is_reasonable() {
        let path = ArctermConfig::config_path();
        assert!(
            path.to_string_lossy().contains("arcterm"),
            "path should contain 'arcterm': {}",
            path.display()
        );
        assert!(
            path.to_string_lossy().ends_with("config.toml"),
            "path should end with config.toml: {}",
            path.display()
        );
    }

    // -- MultiplexerConfig defaults --------------------------------------------

    #[test]
    fn multiplexer_defaults_are_correct() {
        let cfg = MultiplexerConfig::default();
        assert_eq!(cfg.leader_key, "Ctrl+a", "default leader_key");
        assert_eq!(cfg.leader_timeout_ms, 500, "default leader_timeout_ms");
        assert!(cfg.show_tab_bar, "default show_tab_bar is true");
        assert!((cfg.border_width - 1.0).abs() < 1e-5, "default border_width");
        assert!(cfg.pane_navigation, "default pane_navigation is true");
    }

    #[test]
    fn arcterm_config_has_multiplexer_with_defaults() {
        let cfg = ArctermConfig::default();
        assert_eq!(cfg.multiplexer.leader_key, "Ctrl+a");
        assert_eq!(cfg.multiplexer.leader_timeout_ms, 500);
        assert!(cfg.multiplexer.show_tab_bar);
        assert!((cfg.multiplexer.border_width - 1.0).abs() < 1e-5);
        assert!(cfg.multiplexer.pane_navigation);
    }

    // -- MultiplexerConfig TOML overrides --------------------------------------

    #[test]
    fn multiplexer_toml_overrides_fields() {
        let toml = r#"
            [multiplexer]
            leader_key = "Ctrl+b"
            leader_timeout_ms = 1000
            show_tab_bar = false
            border_width = 2.5
            pane_navigation = false
        "#;

        let cfg: ArctermConfig = toml::from_str(toml).expect("valid TOML must parse");
        assert_eq!(cfg.multiplexer.leader_key, "Ctrl+b");
        assert_eq!(cfg.multiplexer.leader_timeout_ms, 1000);
        assert!(!cfg.multiplexer.show_tab_bar);
        assert!((cfg.multiplexer.border_width - 2.5).abs() < 1e-5);
        assert!(!cfg.multiplexer.pane_navigation);
    }

    #[test]
    fn omitting_multiplexer_section_uses_defaults() {
        let toml = r#"font_size = 16.0"#;
        let cfg: ArctermConfig = toml::from_str(toml).expect("valid TOML");
        assert_eq!(cfg.multiplexer.leader_key, "Ctrl+a");
        assert_eq!(cfg.multiplexer.leader_timeout_ms, 500);
        assert!(cfg.multiplexer.show_tab_bar);
    }

    #[test]
    fn partial_multiplexer_section_leaves_defaults() {
        let toml = r#"
            [multiplexer]
            leader_key = "Ctrl+Space"
        "#;
        let cfg: ArctermConfig = toml::from_str(toml).expect("valid TOML");
        assert_eq!(cfg.multiplexer.leader_key, "Ctrl+Space");
        // Unset fields must keep defaults.
        assert_eq!(cfg.multiplexer.leader_timeout_ms, 500);
        assert!(cfg.multiplexer.show_tab_bar);
        assert!((cfg.multiplexer.border_width - 1.0).abs() < 1e-5);
        assert!(cfg.multiplexer.pane_navigation);
    }
}
