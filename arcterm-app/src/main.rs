//! arcterm-app — application entry point.
//!
//! # Manual Test Checklist (Task 3 acceptance criteria)
//!
//! Run `cargo run --package arcterm-app` and verify each item:
//!
//! 1. `ls --color` — coloured directory listing renders correctly with ANSI colours.
//! 2. `vim` — full-screen editor launches, redraws on resize, and exits cleanly.
//! 3. `top` — live updating display renders without corruption.
//! 4. `htop` — same as top; mouse input is not required for pass.
//! 5. Window resize — drag the window edge; the shell prompt reflows to the new width.
//! 6. Ctrl+C — sends SIGINT; running process terminates and returns to shell prompt.
//! 7. `echo -e "\033[31mred\033[0m"` — red text appears, then colour resets.
//! 8. Rapid output (`cat /dev/urandom | head -c 1M | base64`) — no hang or crash.
//!
//! # Phase 5 Integration Test Checklist (PLAN-3.2 Task 3)
//!
//! These five scenarios map to the five Phase 5 success criteria (SC-1 through SC-5).
//! Run them in order after a clean build (`cargo build --release -p arcterm-app`).
//!
//! ## SC-1: `arcterm open <workspace>` reads TOML and restores layout
//!
//! 1. Create `~/.config/arcterm/workspaces/test-project.toml` with content:
//!    ```toml
//!    schema_version = 1
//!    [workspace]
//!    name = "test-project"
//!    directory = "/tmp"
//!    [layout]
//!    type = "hsplit"
//!    ratio = 0.5
//!    [layout.left]
//!    type = "leaf"
//!    command = "echo 'left pane'"
//!    directory = "/tmp"
//!    [layout.right]
//!    type = "leaf"
//!    command = "echo 'right pane'"
//!    directory = "/tmp"
//!    ```
//! 2. Run `arcterm-app open test-project`.
//! 3. Verify: window opens with two side-by-side panes. Left pane shows "left pane"
//!    output. Right pane shows "right pane" output. Both panes' shells are in `/tmp`.
//!
//! Pass criteria: correct 2-pane layout, correct output in each pane.
//!
//! ## SC-2: Session persistence survives exit and reopen
//!
//! 1. Open arcterm with default launch. Split into 2 panes (Ctrl+a, n).
//!    `cd /tmp` in one pane.
//! 2. Close the window (Cmd+Q or click close button).
//! 3. Reopen arcterm with default launch (no arguments).
//! 4. Verify: the 2-pane layout is restored. One pane's CWD is `/tmp`.
//!
//! Pass criteria: layout matches the session at close time. CWD is restored.
//!
//! ## SC-3: Leader+w opens fuzzy workspace switcher
//!
//! 1. Create 3 workspace files in `~/.config/arcterm/workspaces/`:
//!    `alpha.toml`, `beta.toml`, `gamma.toml` (contents: minimal single-leaf TOML).
//! 2. Open arcterm. Press Ctrl+a then w.
//! 3. Verify: dim overlay appears with a search box and three workspace names listed.
//! 4. Type "al" — only "alpha" remains visible.
//! 5. Press Enter — alpha workspace loads.
//! 6. Reopen switcher (Ctrl+a, w), press Escape — overlay dismisses, current session
//!    is unchanged.
//!
//! Pass criteria: overlay appears with correct entries, filter narrows list, Enter
//! opens workspace, Escape dismisses without side-effects.
//!
//! ## SC-4: Workspace TOML files are human-readable and git-committable
//!
//! 1. Inside arcterm, press Ctrl+a then s to save the current session.
//! 2. Open `~/.config/arcterm/workspaces/session-*.toml` in a text editor or
//!    `cat` it in a terminal.
//! 3. Verify: the file is valid TOML with clear `[workspace]`, `[layout]`,
//!    and (when non-empty) `[environment]` sections. No binary data. The file
//!    can be `git add`-ed and `git diff`-ed cleanly.
//!
//! Pass criteria: file is human-readable TOML, committed cleanly to git.
//!
//! ## SC-5: Workspace restore under 500ms for 4-pane layout
//!
//! 1. Create `~/.config/arcterm/workspaces/four-pane-test.toml` with 4 panes:
//!    ```toml
//!    schema_version = 1
//!    [workspace]
//!    name = "four-pane-test"
//!    [layout]
//!    type = "hsplit"
//!    ratio = 0.5
//!    [layout.left]
//!    type = "vsplit"
//!    ratio = 0.5
//!    [layout.left.top]
//!    type = "leaf"
//!    [layout.left.bottom]
//!    type = "leaf"
//!    [layout.right]
//!    type = "vsplit"
//!    ratio = 0.5
//!    [layout.right.top]
//!    type = "leaf"
//!    [layout.right.bottom]
//!    type = "leaf"
//!    ```
//! 2. Run `RUST_LOG=info arcterm-app open four-pane-test` and observe the log output.
//! 3. Measure time from process start to first frame render using wall-clock
//!    observation or the `latency-trace` feature flag.
//! 4. Verify: restore completes and all 4 panes are interactive in under 500ms.
//!
//! Pass criteria: 4-pane workspace is fully interactive within 500ms of launch.
//!
//! # Phase 7 Integration Test Checklist (PLAN-7.3 Task 3)
//!
//! These six scenarios map to the six Phase 7 success criteria (SC-1 through SC-6).
//! Run with `RUST_LOG=info cargo run --package arcterm-app`.
//!
//! ## SC-1: AI agent detection
//!
//! 1. Split the window into two panes (Leader+n).
//! 2. In the second pane, run `claude` or `codex` (or any known AI binary).
//! 3. In the first pane, press Ctrl+Shift+h (or the configured NavigatePane key)
//!    to move focus to the AI pane.
//! 4. Observe the log output: expect `INFO AI agent detected in pane PaneId(N): ClaudeCode`.
//!
//! Pass criteria: detection log appears at most 5 seconds after focusing the pane.
//!
//! ## SC-2: Cross-pane context query
//!
//! 1. In an AI pane session, send the OSC sequence:
//!    `printf '\033]7770;context/query\007'`
//! 2. Observe the PTY input to the AI pane; an OSC 7770 JSON response should
//!    arrive containing `[{"pane_id":...,"cwd":...,"last_command":...}]`.
//!
//! Pass criteria: the response JSON lists all sibling panes, excluding the querying pane.
//!
//! ## SC-3: MCP tool discovery
//!
//! 1. Install a test plugin (`arcterm plugin install <path>`).
//! 2. From an AI pane, send: `printf '\033]7770;tools/list\007'`
//! 3. Verify a base64-encoded JSON array of tool descriptors is written back to
//!    the pane's PTY input as `ESC ] 7770 ; tools/response ; <b64> BEL`.
//!
//! Pass criteria: tool descriptors include `name`, `description`, `inputSchema`.
//!
//! ## SC-4: Leader+p plan strip
//!
//! 1. Ensure `.shipyard/` files exist in the working directory.
//! 2. Press Leader+p.
//! 3. Verify the plan strip appears at the bottom of the window showing plan summaries.
//! 4. Press Leader+p again; the expanded overlay opens showing task detail.
//! 5. Press Leader+p a third time; the overlay closes (strip remains).
//!
//! Pass criteria: strip renders without crash; overlay toggles correctly.
//!
//! ## SC-5: Leader+a jump and no-op safety
//!
//! 1. Open a session with no AI pane. Press Leader+a.
//!    Verify: nothing happens (no crash, no focus change).
//! 2. Start an AI agent in one pane. Press Leader+a from a shell pane.
//!    Verify: focus jumps to the AI pane.
//!
//! Pass criteria: no crash when no AI pane exists; correct pane focused when one does.
//!
//! ## SC-6: Error bridging
//!
//! 1. Open a session with an AI pane (Leader+n, start claude in new pane).
//! 2. In a shell pane, run a failing command: `false` or `ls /nonexistent`.
//! 3. Press Leader+a to jump to the AI pane.
//! 4. Observe the AI pane's PTY: an OSC 7770 error block should be injected
//!    with `type=error`, `exit_code=1`, and the last output lines.
//!
//! Pass criteria: structured error block appears in the AI pane PTY input on Leader+a.

mod ai_detect;
mod colors;
mod config;
mod context;
mod detect;
mod input;
mod keymap;
mod kitty_types;
mod layout;
mod neovim;
mod osc7770;
mod overlay;
mod palette;
mod plan;
mod prefilter;
mod proc;
mod search;
mod selection;
mod tab;
mod terminal;
mod menu;
mod workspace;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::event_loop::ControlFlow;

#[cfg(feature = "latency-trace")]
use std::time::Instant as TraceInstant;

use arcterm_render::{
    ContentType, HighlightEngine, OverlayQuad, PaneRenderInfo, PluginPaneRenderInfo, PluginStyledLine,
    RenderPalette, RenderSnapshot, Renderer, StructuredBlock,
};
use keymap::{KeyAction, KeymapHandler};
use palette::{PaletteState, WorkspaceSwitcherState};
use layout::{Axis, Direction, PaneId, PaneNode, PixelRect};
use selection::{generate_selection_quads, pixel_to_cell, Clipboard, Selection, SelectionMode, SelectionQuad};
use tab::TabManager;
use detect::AutoDetector;
use terminal::{PendingImage, Terminal};
use tokio::sync::mpsc;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::ModifiersState,
    window::{Window, WindowId},
};

// ---------------------------------------------------------------------------
// CLI structs (clap 4 derive)
// ---------------------------------------------------------------------------

#[derive(clap::Parser)]
#[command(name = "arcterm", about = "GPU-rendered AI terminal emulator")]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(clap::Subcommand)]
enum CliCommand {
    /// Open a named workspace
    Open {
        /// Workspace name (without .toml extension)
        name: String,
    },
    /// Save current session as a workspace
    Save {
        /// Workspace name
        name: String,
    },
    /// List available workspaces
    List,
    /// Manage plugins
    Plugin {
        #[command(subcommand)]
        subcommand: PluginSubcommand,
    },
    /// Manage configuration
    Config {
        #[command(subcommand)]
        subcommand: ConfigSubcommand,
    },
}

#[derive(clap::Subcommand)]
enum ConfigSubcommand {
    /// Print the fully resolved configuration (base + accepted overlays) as TOML
    Flatten,
}

#[derive(clap::Subcommand)]
enum PluginSubcommand {
    /// Install a plugin from a directory containing plugin.toml
    Install {
        /// Path to the plugin directory
        path: std::path::PathBuf,
    },
    /// List installed plugins
    List,
    /// Remove an installed plugin by name
    Remove {
        /// Plugin name (as declared in plugin.toml)
        name: String,
    },
    /// Load a plugin directly from a directory (for development, no copy)
    Dev {
        /// Path to the plugin directory
        path: std::path::PathBuf,
    },
}

/// Maximum gap between clicks to be counted as a multi-click (in ms).
const MULTI_CLICK_INTERVAL_MS: u64 = 400;

/// Lines scrolled per mouse-wheel tick.
const SCROLL_LINES_PER_TICK: usize = 3;

/// Width of pane borders in physical pixels (logical border_width * scale).
const BORDER_PX: f32 = 2.0;

/// Border color for unfocused pane borders (dark grey, RGBA).
const BORDER_COLOR_NORMAL: [u8; 4] = [60, 60, 60, 255];

/// Border color for the split containing the focused pane (accent, RGBA).
const BORDER_COLOR_FOCUS: [u8; 4] = [130, 100, 200, 255];

/// Amount to adjust the split ratio per resize keypress.
const RESIZE_DELTA: f32 = 0.05;

/// Border drag threshold in physical pixels — within this distance of a border
/// edge the cursor is treated as "on the border" for drag-to-resize.
const BORDER_DRAG_THRESHOLD: f32 = 4.0;

// ---------------------------------------------------------------------------
// Startup helpers
// ---------------------------------------------------------------------------

/// Count the number of leaf nodes in a `WorkspacePaneNode` tree.
fn count_leaves(node: &workspace::WorkspacePaneNode) -> usize {
    match node {
        workspace::WorkspacePaneNode::Leaf { .. } => 1,
        workspace::WorkspacePaneNode::HSplit { left, right, .. } => {
            count_leaves(left) + count_leaves(right)
        }
        workspace::WorkspacePaneNode::VSplit { top, bottom, .. } => {
            count_leaves(top) + count_leaves(bottom)
        }
    }
}

/// Collections returned by pane-spawn helpers, consumed by `AppState` init.
type PaneBundle = (
    HashMap<PaneId, Terminal>,
    HashMap<PaneId, mpsc::Receiver<PendingImage>>,
    TabManager,
    Vec<PaneNode>,
    HashMap<PaneId, AutoDetector>,
    HashMap<PaneId, Vec<StructuredBlock>>,
);

/// Spawn a single default pane and return the collections needed by `AppState`.
///
/// This is the normal (no workspace) startup path. Extracted into a function
/// so that workspace restore can fall back to it when the session file is
/// empty or invalid.
fn spawn_default_pane(cfg: &config::ArctermConfig, initial_size: (usize, usize)) -> PaneBundle {
    let first_id = PaneId::next();
    let (rows, cols) = initial_size;
    let (terminal, image_rx) =
        Terminal::new(cols, rows, 8, 16, cfg.shell.clone(), None)
            .unwrap_or_else(|e| {
                log::error!("Failed to create PTY session: {e}");
                std::process::exit(1);
            });

    let mut panes = HashMap::new();
    panes.insert(first_id, terminal);

    let mut image_channels = HashMap::new();
    image_channels.insert(first_id, image_rx);

    let tab_manager = TabManager::new(first_id);
    let tab_layouts = vec![PaneNode::Leaf { pane_id: first_id }];

    let mut auto_detectors = HashMap::new();
    auto_detectors.insert(first_id, AutoDetector::new());
    let mut structured_blocks_map: HashMap<PaneId, Vec<StructuredBlock>> = HashMap::new();
    structured_blocks_map.insert(first_id, Vec::new());

    (panes, image_channels, tab_manager, tab_layouts, auto_detectors, structured_blocks_map)
}

fn main() {
    env_logger::init();

    let cli = <Cli as clap::Parser>::parse();

    // Handle non-GUI subcommands before touching the event loop.
    // `dev_plugin` carries the path for `arcterm plugin dev <path>` so the
    // GUI startup can load the plugin without copying it.
    let mut dev_plugin: Option<std::path::PathBuf> = None;

    let initial_workspace: Option<workspace::WorkspaceFile> = match cli.command {
        Some(CliCommand::List) => {
            let workspaces = workspace::list_workspaces();
            if workspaces.is_empty() {
                println!("No workspaces found.");
            } else {
                for (name, _) in workspaces {
                    println!("{name}");
                }
            }
            return;
        }
        Some(CliCommand::Save { .. }) => {
            eprintln!(
                "Save command requires a running arcterm session. \
                 Use Leader+s from within arcterm."
            );
            return;
        }
        Some(CliCommand::Open { name }) => {
            let path = workspace::workspaces_dir().join(format!("{name}.toml"));
            match workspace::WorkspaceFile::load_from_file(&path) {
                Ok(ws) => Some(ws),
                Err(e) => {
                    eprintln!("arcterm: failed to open workspace '{name}': {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(CliCommand::Plugin { subcommand }) => {
            match subcommand {
                PluginSubcommand::Install { path } => {
                    let mut mgr = arcterm_plugin::manager::PluginManager::new()
                        .unwrap_or_else(|e| {
                            eprintln!("arcterm: failed to initialize plugin manager: {e}");
                            std::process::exit(1);
                        });
                    match mgr.install(&path) {
                        Ok(id) => println!("Plugin installed with id {id}"),
                        Err(e) => {
                            eprintln!("arcterm: plugin install failed: {e}");
                            std::process::exit(1);
                        }
                    }
                    return;
                }
                PluginSubcommand::List => {
                    let mgr = arcterm_plugin::manager::PluginManager::new()
                        .unwrap_or_else(|e| {
                            eprintln!("arcterm: failed to initialize plugin manager: {e}");
                            std::process::exit(1);
                        });
                    let plugins = mgr.list_installed();
                    if plugins.is_empty() {
                        println!("No plugins installed.");
                    } else {
                        let header = format!("{:<20} {:<10} ID", "NAME", "VERSION");
                        println!("{header}");
                        for (id, name, version) in plugins {
                            println!("{name:<20} {version:<10} {id}");
                        }
                    }
                    return;
                }
                PluginSubcommand::Remove { name } => {
                    let plugin_dir = dirs::config_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                        .join("arcterm")
                        .join("plugins")
                        .join(&name);
                    if plugin_dir.exists() {
                        match std::fs::remove_dir_all(&plugin_dir) {
                            Ok(_) => println!("Plugin '{name}' removed."),
                            Err(e) => {
                                eprintln!("arcterm: failed to remove plugin '{name}': {e}");
                                std::process::exit(1);
                            }
                        }
                    } else {
                        eprintln!("arcterm: plugin '{name}' not found.");
                        std::process::exit(1);
                    }
                    return;
                }
                PluginSubcommand::Dev { path } => {
                    dev_plugin = Some(path);
                    None
                }
            }
        }
        Some(CliCommand::Config { subcommand }) => {
            match subcommand {
                ConfigSubcommand::Flatten => {
                    match config::ArctermConfig::flatten_to_string() {
                        Ok(s) => {
                            print!("{s}");
                        }
                        Err(e) => {
                            eprintln!("arcterm: config flatten failed: {e}");
                            std::process::exit(1);
                        }
                    }
                    return;
                }
            }
        }
        None => {
            // Auto-restore from last session if it exists.
            let session_path = workspace::workspaces_dir().join("_last_session.toml");
            if session_path.exists() {
                match workspace::WorkspaceFile::load_from_file(&session_path) {
                    Ok(ws) => {
                        log::info!("Restoring last session from {}", session_path.display());
                        // Delete the file so it is consumed exactly once.
                        if let Err(e) = std::fs::remove_file(&session_path) {
                            log::warn!("Could not remove _last_session.toml: {e}");
                        }
                        Some(ws)
                    }
                    Err(e) => {
                        log::warn!(
                            "Could not parse _last_session.toml, starting fresh: {e}"
                        );
                        None
                    }
                }
            } else {
                None
            }
        }
    };

    #[cfg(feature = "latency-trace")]
    let cold_start = TraceInstant::now();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let event_loop = EventLoop::new().unwrap();
    let mut app = App {
        state: None,
        modifiers: ModifiersState::empty(),
        initial_workspace,
        dev_plugin,
        #[cfg(feature = "latency-trace")]
        cold_start,
    };
    event_loop.run_app(&mut app).unwrap();
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

struct AppState {
    window: Arc<Window>,
    renderer: Renderer,

    // ---- multiplexer core ----
    /// All open terminal instances, keyed by pane ID.
    panes: HashMap<PaneId, Terminal>,
    /// Decoded Kitty image channels for every open pane.
    ///
    /// Receivers are drained via `try_recv` in `about_to_wait` before each
    /// frame, delivering decoded `PendingImage` values produced on the tokio
    /// blocking thread pool by the pre-filter reader thread.
    image_channels: HashMap<PaneId, mpsc::Receiver<PendingImage>>,
    /// Tab manager: tab labels, per-tab focused pane, per-tab zoom state.
    tab_manager: TabManager,
    /// Layout trees for each tab, index-aligned with `tab_manager.tabs`.
    tab_layouts: Vec<PaneNode>,
    /// Leader-key state machine.
    keymap: KeymapHandler,

    // ---- structured output ----
    /// Highlight engine: owns SyntaxSet + ThemeSet; loaded once at startup.
    highlight_engine: HighlightEngine,
    /// Auto-detector per pane: heuristic structured-content detection.
    auto_detectors: HashMap<PaneId, AutoDetector>,
    /// Accumulated structured blocks per pane (OSC 7770 + auto-detected).
    structured_blocks: HashMap<PaneId, Vec<StructuredBlock>>,
    /// Per-pane pixel rects of code-block copy buttons: (pane_id, rect [x,y,w,h]).
    copy_button_rects: Vec<(PaneId, [f32; 4], usize)>, // (id, rect, block_idx)

    // ---- configuration ----
    config: config::ArctermConfig,
    config_rx: Option<std::sync::mpsc::Receiver<config::ArctermConfig>>,

    // ---- selection & clipboard ----
    selection: Selection,
    clipboard: Option<Clipboard>,
    /// Last known physical cursor position (pixels).
    last_cursor_position: (f64, f64),
    /// Timestamp of the last left-button press (for multi-click detection).
    last_click_time: Option<Instant>,
    /// Consecutive click count: 1 = single, 2 = double, 3 = triple.
    click_count: u32,

    // ---- border drag state ----
    /// If Some, we are dragging the border that contains this pane to resize.
    drag_pane: Option<PaneId>,

    // ---- deferred resize ----
    /// Pending window resize received from `WindowEvent::Resized`.
    ///
    /// Multiple `Resized` events arriving between frames (e.g. during a rapid
    /// window drag) overwrite this field; only the last value is applied in
    /// `about_to_wait`, coalescing all intermediate sizes into a single resize
    /// per frame.
    pending_resize: Option<winit::dpi::PhysicalSize<u32>>,

    // ---- selection rendering ----
    selection_quads: Vec<SelectionQuad>,

    // ---- performance / control flow ----

    /// Cached terminal snapshots from `about_to_wait`, reused in `RedrawRequested`
    /// to avoid taking a second snapshot per pane per frame.
    cached_snapshots: HashMap<PaneId, RenderSnapshot>,

    fps_last_log: Instant,
    fps_frame_count: u32,

    /// Command palette state; `None` when the palette is closed.
    palette_mode: Option<PaletteState>,

    /// Workspace switcher state; `None` when the switcher is closed.
    workspace_switcher: Option<WorkspaceSwitcherState>,

    // ---- Neovim integration ----
    /// Cached Neovim detection state for each pane.  Updated lazily on
    /// `NavigatePane` events with a 2-second TTL to avoid syscall spam.
    nvim_states: HashMap<PaneId, neovim::NeovimState>,

    // ---- AI agent detection ----
    /// Cached AI agent detection state for each pane.  Updated lazily on
    /// `NavigatePane` events with a 5-second TTL.
    ai_states: HashMap<PaneId, ai_detect::AiAgentState>,
    /// Per-pane context: last command, exit code, and recent output lines.
    pane_contexts: HashMap<PaneId, context::PaneContext>,
    /// The pane that most recently received PTY data while running an AI agent.
    last_ai_pane: Option<PaneId>,
    /// Pending error contexts from shell panes awaiting injection into the AI pane.
    ///
    /// Populated when a non-zero exit code is received and an AI pane exists.
    /// Drained and injected into the AI pane's PTY input on Leader+a navigation.
    pending_errors: Vec<context::ErrorContext>,

    // ---- plugin system ----
    /// Plugin manager: owns all loaded plugin instances.
    plugin_manager: Option<arcterm_plugin::manager::PluginManager>,
    /// Sender for broadcasting terminal lifecycle events to plugins.
    plugin_event_tx: Option<tokio::sync::broadcast::Sender<arcterm_plugin::manager::PluginEvent>>,

    // ---- cross-pane search ----
    /// Search overlay state; `None` when the search overlay is closed.
    search_overlay: Option<search::SearchOverlayState>,

    // ---- plan status layer ----
    /// Ambient plan status bar; `None` until the first workspace scan.
    plan_strip: Option<plan::PlanStripState>,
    /// Expanded plan view overlay; `None` when closed.
    plan_view: Option<plan::PlanViewState>,
    /// File-system watcher for `.shipyard/`, `PLAN.md`, `TODO.md`.
    // Held to keep the watcher alive; events are received via plan_watcher_rx.
    #[allow(dead_code)]
    plan_watcher: Option<notify::RecommendedWatcher>,
    /// Channel from plan_watcher for receiving change notifications.
    plan_watcher_rx: Option<std::sync::mpsc::Receiver<notify::Result<notify::Event>>>,
    /// Cached workspace root (current working directory at launch).
    workspace_root: std::path::PathBuf,

    // ---- config overlay review ----
    /// Config overlay review state; `None` when the overlay is closed.
    overlay_review: Option<overlay::OverlayReviewState>,

    // ---- deferred plugin loading ----
    /// Set to `true` once plugins have been loaded after the first frame.
    plugins_loaded: bool,

    // ---- latency tracing ----
    /// Timestamp of the most recent key-press event; used to measure
    /// key-to-frame-presented latency under the `latency-trace` feature.
    #[cfg(feature = "latency-trace")]
    key_press_t0: Option<std::time::Instant>,
}

impl AppState {
    // -----------------------------------------------------------------------
    // Active-tab helpers
    // -----------------------------------------------------------------------

    /// The `PaneId` of the focused pane in the active tab.
    fn focused_pane(&self) -> PaneId {
        self.tab_manager.active_tab().focus
    }

    /// Set the focused pane on the active tab.
    fn set_focused_pane(&mut self, id: PaneId) {
        self.tab_manager.active_tab_mut().focus = id;
    }

    /// The layout tree for the active tab.
    fn active_layout(&self) -> &PaneNode {
        &self.tab_layouts[self.tab_manager.active]
    }

    /// A mutable reference to the layout tree for the active tab.
    #[allow(dead_code)] // Available for future use
    fn active_layout_mut(&mut self) -> &mut PaneNode {
        let active = self.tab_manager.active;
        &mut self.tab_layouts[active]
    }

    // -----------------------------------------------------------------------
    // Session save
    // -----------------------------------------------------------------------

    /// Capture the current session and write it atomically to
    /// `~/.config/arcterm/workspaces/_last_session.toml`.
    ///
    /// Called on `CloseRequested` before exiting. Errors are logged but never
    /// prevent the exit — session save is best-effort.
    fn save_session(&self) -> Result<(), workspace::WorkspaceError> {
        use std::collections::HashMap as HM;

        // Build per-pane metadata from live terminals.
        let mut pane_metadata: HM<PaneId, workspace::PaneMetadata> = HM::new();
        for (id, terminal) in &self.panes {
            let directory = terminal.cwd().and_then(|p| p.to_str().map(str::to_string));
            pane_metadata.insert(
                *id,
                workspace::PaneMetadata { command: None, directory, env: None },
            );
        }

        // Capture using the active tab's layout.
        let win_size = self.window.inner_size();
        let workspace_file = workspace::capture_session(
            &self.tab_manager,
            &pane_metadata,
            "_last_session",
            Some((win_size.width, win_size.height)),
        );

        // Ensure the workspaces directory exists.
        let dir = workspace::workspaces_dir();
        std::fs::create_dir_all(&dir)?;

        let path = dir.join("_last_session.toml");
        workspace_file.save_to_file(&path)?;
        log::info!("Session saved to {}", path.display());

        Ok(())
    }

    /// Capture the current session and write it to a named workspace file in
    /// `workspaces_dir()/<name>.toml`.
    ///
    /// The `name` parameter is used as the workspace name and the file stem.
    /// Called by the Leader+s keybinding handler with a timestamp-generated name.
    fn save_named_session(&self, name: &str) -> Result<(), workspace::WorkspaceError> {
        use std::collections::HashMap as HM;

        // Build per-pane metadata from live terminals.
        let mut pane_metadata: HM<PaneId, workspace::PaneMetadata> = HM::new();
        for (id, terminal) in &self.panes {
            let directory = terminal.cwd().and_then(|p| p.to_str().map(str::to_string));
            pane_metadata.insert(
                *id,
                workspace::PaneMetadata { command: None, directory, env: None },
            );
        }

        // Capture using the active tab's layout.
        let win_size = self.window.inner_size();
        let workspace_file = workspace::capture_session(
            &self.tab_manager,
            &pane_metadata,
            name,
            Some((win_size.width, win_size.height)),
        );

        // Ensure the workspaces directory exists.
        let dir = workspace::workspaces_dir();
        std::fs::create_dir_all(&dir)?;

        let path = dir.join(format!("{name}.toml"));
        workspace_file.save_to_file(&path)?;
        log::info!("Session saved to {}", path.display());

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Geometry helpers
    // -----------------------------------------------------------------------

    /// Compute the pixel rect available for pane content (below tab bar, above plan strip).
    fn pane_area(&self) -> PixelRect {
        let w = self.renderer.gpu.surface_config.width as f32;
        let h = self.renderer.gpu.surface_config.height as f32;
        let sf = self.window.scale_factor() as f32;

        let tab_h = if self.config.multiplexer.show_tab_bar && self.tab_manager.tab_count() > 1 {
            arcterm_render::tab_bar_height(&self.renderer.text.cell_size, sf)
        } else {
            0.0
        };

        // Reserve one cell row at the bottom for the plan strip when active.
        let strip_h = if self.plan_strip.is_some() {
            self.renderer.text.cell_size.height * sf
        } else {
            0.0
        };

        PixelRect { x: 0.0, y: tab_h, width: w, height: h - tab_h - strip_h }
    }

    /// Compute pixel rects for all panes in the active tab.
    fn compute_pane_rects(&self) -> HashMap<PaneId, PixelRect> {
        let available = self.pane_area();
        let tab = self.tab_manager.active_tab();

        if let Some(zoomed_id) = tab.zoomed {
            self.active_layout().compute_zoomed_rect(zoomed_id, available)
        } else {
            self.active_layout().compute_rects(available, BORDER_PX)
        }
    }

    /// Returns cell pixel dimensions `(cell_width, cell_height)` from the renderer.
    ///
    /// Used when creating a new `Terminal` which requires pixel-level cell dimensions
    /// to configure the PTY TIOCSWINSZ correctly.
    fn cell_dims(&self) -> (u16, u16) {
        let sf = self.window.scale_factor() as f32;
        let cell_w = (self.renderer.text.cell_size.width * sf).max(1.0) as u16;
        let cell_h = (self.renderer.text.cell_size.height * sf).max(1.0) as u16;
        (cell_w, cell_h)
    }

    /// Given a rect, compute the grid size (rows, cols) for a pane.
    fn grid_size_for_rect(&self, rect: PixelRect) -> (usize, usize) {
        let sf = self.window.scale_factor() as f32;
        let cell_w = self.renderer.text.cell_size.width * sf;
        let cell_h = self.renderer.text.cell_size.height * sf;
        if cell_w <= 0.0 || cell_h <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return (1, 1);
        }
        let cols = (rect.width / cell_w).floor() as usize;
        let rows = (rect.height / cell_h).floor() as usize;
        (rows.max(1), cols.max(1))
    }

    // -----------------------------------------------------------------------
    // Spawn a new pane
    // -----------------------------------------------------------------------

    /// Spawn a new Terminal + PTY, insert into the pane and channel maps, and
    /// return its `PaneId`.  `size` is `(rows, cols)`.
    fn spawn_pane(&mut self, size: (usize, usize)) -> PaneId {
        self.spawn_pane_with_cwd(size, None)
    }

    /// Spawn a new Terminal + PTY in the given working directory.
    ///
    /// Used by workspace restore to recreate panes in their saved directories.
    /// Pass `cwd = None` to inherit the process's current working directory.
    /// `size` is `(rows, cols)`.
    fn spawn_pane_with_cwd(&mut self, size: (usize, usize), cwd: Option<&std::path::Path>) -> PaneId {
        let id = PaneId::next();
        let (rows, cols) = size;
        let (cell_w, cell_h) = self.cell_dims();
        match Terminal::new(cols, rows, cell_w, cell_h, self.config.shell.clone(), cwd) {
            Ok((terminal, image_rx)) => {
                self.panes.insert(id, terminal);
                self.image_channels.insert(id, image_rx);
                self.auto_detectors.insert(id, AutoDetector::new());
                self.structured_blocks.insert(id, Vec::new());
                self.ai_states.insert(id, ai_detect::AiAgentState::check(None));
                self.pane_contexts.insert(id, context::PaneContext::new(200));
            }
            Err(e) => {
                log::error!("Failed to create PTY for pane {:?}: {e}", id);
            }
        }

        // Notify plugins that a new pane was opened.
        if let Some(ref tx) = self.plugin_event_tx {
            let _ = tx.send(arcterm_plugin::manager::PluginEvent::PaneOpened(
                format!("{:?}", id),
            ));
        }

        id
    }

    // -----------------------------------------------------------------------
    // Workspace restore
    // -----------------------------------------------------------------------

    /// Close all current panes and restore pane layout from a [`workspace::WorkspaceFile`].
    ///
    /// Reuses the same restore logic as the `resumed()` workspace startup path.
    /// Called by the workspace switcher when the user presses Enter on an entry.
    fn restore_workspace(&mut self, ws: &workspace::WorkspaceFile) {
        // Determine initial grid size from current window dimensions.
        let win_size = self.window.inner_size();
        let initial_size = self.renderer.grid_size_for_window(
            win_size.width,
            win_size.height,
            self.window.scale_factor(),
        );

        // Shut down all current panes and remove pane state.
        let all_ids: Vec<PaneId> = self.panes.keys().copied().collect();
        for id in &all_ids {
            self.panes.remove(id);
            self.image_channels.remove(id);
            self.auto_detectors.remove(id);
            self.structured_blocks.remove(id);
            self.cached_snapshots.remove(id);
            self.nvim_states.remove(id);
            self.ai_states.remove(id);
            self.pane_contexts.remove(id);
        }
        self.last_ai_pane = None;

        // Restore the layout from the workspace file.
        let leaf_count = count_leaves(&ws.layout);
        if leaf_count == 0 {
            log::warn!(
                "Workspace '{}' has no panes; spawning a fresh pane",
                ws.workspace.name
            );
            let id = self.spawn_pane(initial_size);
            let tab_idx = 0;
            if tab_idx < self.tab_layouts.len() {
                self.tab_layouts[tab_idx] = PaneNode::Leaf { pane_id: id };
            } else {
                self.tab_layouts.push(PaneNode::Leaf { pane_id: id });
            }
            let tab = self.tab_manager.active_tab_mut();
            tab.focus = id;
            return;
        }

        let (pane_tree, leaf_metadata) = ws.layout.to_pane_tree();
        let leaf_ids = pane_tree.all_pane_ids();

        let (init_rows, init_cols) = initial_size;
        let (cell_w, cell_h) = self.cell_dims();
        for (id, meta) in leaf_ids.iter().zip(leaf_metadata.iter()) {
            let cwd: Option<std::path::PathBuf> =
                meta.directory.as_deref().map(std::path::PathBuf::from);
            match Terminal::new(init_cols, init_rows, cell_w, cell_h, self.config.shell.clone(), cwd.as_deref()) {
                Ok((terminal, image_rx)) => {
                    // Inject workspace-level environment variables.
                    for (key, val) in &ws.environment {
                        terminal.write_input(format!("export {key}={val}\n").as_bytes());
                    }
                    // Inject per-pane environment overrides.
                    if let Some(env) = &meta.env {
                        for (key, val) in env {
                            terminal.write_input(format!("export {key}={val}\n").as_bytes());
                        }
                    }
                    // Replay saved command.
                    if let Some(cmd) = &meta.command {
                        terminal.write_input(format!("{cmd}\n").as_bytes());
                    }
                    self.panes.insert(*id, terminal);
                    self.image_channels.insert(*id, image_rx);
                }
                Err(e) => {
                    log::error!("Failed to spawn restored pane {:?}: {e}", id);
                }
            }
            self.auto_detectors.insert(*id, AutoDetector::new());
            self.structured_blocks.insert(*id, Vec::new());
            self.ai_states.insert(*id, ai_detect::AiAgentState::check(None));
            self.pane_contexts.insert(*id, context::PaneContext::new(200));
        }

        let focus_id = leaf_ids.first().copied().unwrap_or_else(PaneId::next);

        // Replace the active tab layout; reset other tabs.
        let active = self.tab_manager.active;
        if active < self.tab_layouts.len() {
            self.tab_layouts[active] = pane_tree;
        } else {
            self.tab_layouts.push(pane_tree);
        }

        let tab = self.tab_manager.active_tab_mut();
        tab.focus = focus_id;
        tab.zoomed = None;

        self.selection.clear();

        log::info!(
            "Restored workspace '{}' with {} pane(s)",
            ws.workspace.name,
            leaf_count
        );

        self.window.set_title(&format!("Arcterm — {}", ws.workspace.name));
    }
}

struct App {
    state: Option<AppState>,
    modifiers: ModifiersState,
    /// Workspace to restore on launch, set by `arcterm open <name>`.
    initial_workspace: Option<workspace::WorkspaceFile>,
    /// Dev plugin path, set by `arcterm plugin dev <path>`.
    dev_plugin: Option<std::path::PathBuf>,
    #[cfg(feature = "latency-trace")]
    cold_start: TraceInstant,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_inner_size(LogicalSize::new(1024u32, 768u32))
            .with_title("Arcterm");

        let cfg = config::ArctermConfig::load();
        let config_rx = config::watch_config();
        log::info!(
            "config: font_size={}, scrollback_lines={}, color_scheme={}",
            cfg.font_size,
            cfg.scrollback_lines,
            cfg.color_scheme,
        );

        let window = match event_loop.create_window(window_attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("Window creation failed: {e}");
                event_loop.exit();
                return;
            }
        };

        let mut renderer = match Renderer::new(window.clone(), cfg.font_size) {
            Ok(r) => r,
            Err(e) => {
                log::error!("GPU initialization failed: {e}");
                event_loop.exit();
                return;
            }
        };

        let palette = palette_from_config(&cfg);
        log::info!("config: color_scheme={:?}", cfg.color_scheme);
        renderer.set_palette(palette);

        // Compute initial grid size for a full-window single pane.
        let win_size = window.inner_size();
        let initial_size = renderer.grid_size_for_window(
            win_size.width,
            win_size.height,
            window.scale_factor(),
        );

        let keymap = KeymapHandler::new(cfg.multiplexer.leader_timeout_ms);

        let clipboard = Clipboard::new()
            .map_err(|e| log::warn!("Clipboard unavailable: {e}"))
            .ok();

        // ---------------------------------------------------------------
        // Determine initial panes and layout.
        //
        // If `initial_workspace` is set (from `arcterm open` or auto-restore
        // of `_last_session.toml`), restore panes from the workspace layout.
        // Otherwise spawn a single default pane.
        // ---------------------------------------------------------------
        let (panes, image_channels, tab_manager, tab_layouts, auto_detectors, structured_blocks_map) =
            if let Some(ref ws) = self.initial_workspace {
                let ws_layout = &ws.layout;

                // Validate: if the workspace has no leaves, fall back to
                // a single fresh pane to avoid an empty-pane state.
                let leaf_count = count_leaves(ws_layout);
                if leaf_count == 0 {
                    log::warn!(
                        "Workspace '{}' has no panes; starting with a single fresh pane",
                        ws.workspace.name
                    );
                    spawn_default_pane(&cfg, initial_size)
                } else {
                    // Produce a fresh PaneNode tree with new PaneIds, plus per-leaf metadata.
                    let (pane_tree, leaf_metadata) = ws_layout.to_pane_tree();

                    let mut panes = HashMap::new();
                    let mut image_channels = HashMap::new();
                    let mut auto_detectors = HashMap::new();
                    let mut structured_blocks_map: HashMap<PaneId, Vec<StructuredBlock>> =
                        HashMap::new();

                    // Collect all leaf PaneIds from the restored tree in
                    // the same traversal order as `to_pane_tree()`.
                    let leaf_ids = pane_tree.all_pane_ids();

                    // Spawn a terminal for each leaf, using the saved CWD.
                    for (id, meta) in leaf_ids.iter().zip(leaf_metadata.iter()) {
                        let cwd: Option<std::path::PathBuf> =
                            meta.directory.as_deref().map(std::path::PathBuf::from);
                        let (init_rows, init_cols) = initial_size;
                        match Terminal::new(
                            init_cols,
                            init_rows,
                            8,
                            16,
                            cfg.shell.clone(),
                            cwd.as_deref(),
                        ) {
                            Ok((terminal, image_rx)) => {
                                // Inject workspace-level environment variables.
                                for (key, val) in &ws.environment {
                                    terminal.write_input(
                                        format!("export {key}={val}\n").as_bytes(),
                                    );
                                }
                                // Inject per-pane environment overrides.
                                if let Some(env) = &meta.env {
                                    for (key, val) in env {
                                        terminal.write_input(
                                            format!("export {key}={val}\n").as_bytes(),
                                        );
                                    }
                                }
                                // If a command was saved, replay it.
                                if let Some(cmd) = &meta.command {
                                    terminal.write_input(format!("{cmd}\n").as_bytes());
                                }
                                panes.insert(*id, terminal);
                                image_channels.insert(*id, image_rx);
                            }
                            Err(e) => {
                                log::error!(
                                    "Failed to create PTY for restored pane {:?}: {e}",
                                    id
                                );
                            }
                        }
                        auto_detectors.insert(*id, AutoDetector::new());
                        structured_blocks_map.insert(*id, Vec::new());
                    }

                    // Use the first leaf as focus for the tab.
                    let focus_id = leaf_ids.first().copied().unwrap_or_else(PaneId::next);
                    let tab_manager = TabManager::new(focus_id);
                    let tab_layouts = vec![pane_tree];

                    log::info!(
                        "Restored workspace '{}' with {} pane(s)",
                        ws.workspace.name,
                        leaf_count
                    );

                    (panes, image_channels, tab_manager, tab_layouts, auto_detectors, structured_blocks_map)
                }
            } else {
                spawn_default_pane(&cfg, initial_size)
            };

        // Set window title to include workspace name when restoring.
        if let Some(ref ws) = self.initial_workspace {
            window.set_title(&format!("Arcterm — {}", ws.workspace.name));
        }

        // ---------------------------------------------------------------
        // Plugin manager initialization is DEFERRED to after the first
        // frame renders.  WASM compilation can take hundreds of ms; doing
        // it here would block the GPU surface from presenting its first
        // frame.  The actual load happens in `about_to_wait` when
        // `fps_frame_count == 1` and `plugins_loaded == false`.
        // ---------------------------------------------------------------
        let plugin_manager: Option<arcterm_plugin::manager::PluginManager> = None;
        let plugin_event_tx: Option<tokio::sync::broadcast::Sender<arcterm_plugin::manager::PluginEvent>> = None;

        // Pre-populate ai_states and pane_contexts for all initial panes.
        let mut ai_states: HashMap<PaneId, ai_detect::AiAgentState> = HashMap::new();
        let mut pane_contexts: HashMap<PaneId, context::PaneContext> = HashMap::new();
        for id in panes.keys() {
            ai_states.insert(*id, ai_detect::AiAgentState::check(None));
            pane_contexts.insert(*id, context::PaneContext::new(200));
        }

        // ---------------------------------------------------------------
        // Plan status layer initialization.
        //
        // Scan the workspace root for plan files and start a file-system
        // watcher for `.shipyard/`, `PLAN.md`, and `TODO.md`.
        // ---------------------------------------------------------------
        let workspace_root = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));

        let plan_strip = {
            let strip = plan::PlanStripState::discover(&workspace_root);
            if strip.summaries.is_empty() { None } else { Some(strip) }
        };

        let (plan_watcher, plan_watcher_rx) = {
            let (tx, rx) = std::sync::mpsc::channel();
            let watcher_result = notify::recommended_watcher(move |res| {
                // Forward all events; the main loop filters by relevance.
                let _ = tx.send(res);
            });
            match watcher_result {
                Ok(mut w) => {
                    use notify::{RecursiveMode, Watcher};
                    let shipyard = workspace_root.join(".shipyard");
                    if shipyard.exists() {
                        let _ = w.watch(&shipyard, RecursiveMode::Recursive);
                    }
                    let plan_md = workspace_root.join("PLAN.md");
                    if plan_md.exists() {
                        let _ = w.watch(&plan_md, RecursiveMode::NonRecursive);
                    }
                    let todo_md = workspace_root.join("TODO.md");
                    if todo_md.exists() {
                        let _ = w.watch(&todo_md, RecursiveMode::NonRecursive);
                    }
                    (Some(w), Some(rx))
                }
                Err(e) => {
                    log::warn!("plan watcher: failed to initialize: {e}");
                    (None, None)
                }
            }
        };

        self.state = Some(AppState {
            window,
            renderer,
            panes,
            image_channels,
            tab_manager,
            tab_layouts,
            keymap,
            highlight_engine: HighlightEngine::new(),
            auto_detectors,
            structured_blocks: structured_blocks_map,
            copy_button_rects: Vec::new(),
            config: cfg,
            config_rx,
            selection: Selection::default(),
            clipboard,
            last_cursor_position: (0.0, 0.0),
            last_click_time: None,
            click_count: 0,
            drag_pane: None,
            pending_resize: None,
            selection_quads: Vec::new(),

            cached_snapshots: HashMap::new(),
            fps_last_log: Instant::now(),
            fps_frame_count: 0,
            palette_mode: None,
            workspace_switcher: None,
            search_overlay: None,
            nvim_states: HashMap::new(),
            ai_states,
            pane_contexts,
            last_ai_pane: None,
            pending_errors: Vec::new(),
            plugin_manager,
            plugin_event_tx,
            plan_strip,
            plan_view: None,
            plan_watcher,
            plan_watcher_rx,
            workspace_root,
            overlay_review: None,
            plugins_loaded: false,
            #[cfg(feature = "latency-trace")]
            key_press_t0: None,
        });
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = &mut self.state else {
            return;
        };

        // ------------------------------------------------------------------
        // Deferred plugin loading — runs once, after the first frame has been
        // presented (fps_frame_count >= 1).  Loading WASM plugins involves
        // Cranelift compilation which can take 100+ ms; deferring it here
        // keeps the cold-start path fast.
        // ------------------------------------------------------------------
        if !state.plugins_loaded && state.fps_frame_count >= 1 {
            state.plugins_loaded = true;
            log::info!("[startup] loading plugins (deferred after first frame)");
            match arcterm_plugin::manager::PluginManager::new() {
                Ok(mut mgr) => {
                    let results = mgr.load_all_installed();
                    for result in results {
                        match result {
                            Ok(id) => log::info!("Plugin loaded: {id}"),
                            Err(e) => log::warn!("Failed to load plugin: {e}"),
                        }
                    }
                    // Load dev plugin if requested via `arcterm plugin dev <path>`.
                    if let Some(ref dev_path) = self.dev_plugin {
                        match mgr.load_dev(dev_path) {
                            Ok(id) => log::info!("Dev plugin loaded: {id}"),
                            Err(e) => log::warn!("Failed to load dev plugin: {e}"),
                        }
                    }
                    let event_tx = mgr.event_sender();
                    state.plugin_manager = Some(mgr);
                    state.plugin_event_tx = Some(event_tx);
                }
                Err(e) => {
                    log::warn!("Failed to initialize plugin manager: {e}");
                }
            }
        }

        // ------------------------------------------------------------------
        // Config hot-reload
        // ------------------------------------------------------------------
        if let Some(rx) = &state.config_rx {
            loop {
                match rx.try_recv() {
                    Ok(new_cfg) => {
                        if (new_cfg.font_size - state.config.font_size).abs() > f32::EPSILON {
                            log::info!(
                                "config: font_size changed ({} → {}): restart required",
                                state.config.font_size,
                                new_cfg.font_size,
                            );
                        }

                        if new_cfg.scrollback_lines != state.config.scrollback_lines {
                            log::info!(
                                "config: scrollback_lines changed ({} → {})",
                                state.config.scrollback_lines,
                                new_cfg.scrollback_lines,
                            );
                            // Update all live panes.
                            // NOTE: alacritty Term scrollback is configured at construction time;
                            // dynamic update not yet supported — tracked for Plan 3.1.
                            let _ = new_cfg.scrollback_lines; // acknowledged
                        }

                        if new_cfg.color_scheme != state.config.color_scheme
                            || new_cfg.colors.foreground != state.config.colors.foreground
                            || new_cfg.colors.background != state.config.colors.background
                            || new_cfg.colors.cursor != state.config.colors.cursor
                            || new_cfg.colors.red != state.config.colors.red
                            || new_cfg.colors.green != state.config.colors.green
                            || new_cfg.colors.blue != state.config.colors.blue
                            || new_cfg.colors.yellow != state.config.colors.yellow
                            || new_cfg.colors.magenta != state.config.colors.magenta
                            || new_cfg.colors.cyan != state.config.colors.cyan
                            || new_cfg.colors.white != state.config.colors.white
                            || new_cfg.colors.black != state.config.colors.black
                            || new_cfg.colors.bright_red != state.config.colors.bright_red
                            || new_cfg.colors.bright_green != state.config.colors.bright_green
                            || new_cfg.colors.bright_blue != state.config.colors.bright_blue
                            || new_cfg.colors.bright_yellow != state.config.colors.bright_yellow
                            || new_cfg.colors.bright_magenta != state.config.colors.bright_magenta
                            || new_cfg.colors.bright_cyan != state.config.colors.bright_cyan
                            || new_cfg.colors.bright_white != state.config.colors.bright_white
                            || new_cfg.colors.bright_black != state.config.colors.bright_black
                        {
                            log::info!(
                                "config: color_scheme changed ({:?} → {:?}), reloading palette",
                                state.config.color_scheme,
                                new_cfg.color_scheme,
                            );
                            let new_palette = palette_from_config(&new_cfg);
                            state.renderer.set_palette(new_palette);
                            state.window.request_redraw();
                        }

                        state.config = new_cfg;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        log::warn!("config: watcher channel disconnected");
                        break;
                    }
                }
            }
        }

        // ------------------------------------------------------------------
        // Plan watcher hot-reload — refresh summaries on file changes.
        // ------------------------------------------------------------------
        {
            let mut plan_changed = false;
            if let Some(rx) = &state.plan_watcher_rx {
                loop {
                    match rx.try_recv() {
                        Ok(Ok(_event)) => {
                            plan_changed = true;
                        }
                        Ok(Err(e)) => {
                            log::warn!("plan watcher: error: {e}");
                            break;
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            log::warn!("plan watcher: channel disconnected");
                            break;
                        }
                    }
                }
            }
            if plan_changed {
                let root = state.workspace_root.clone();
                if let Some(ref mut strip) = state.plan_strip {
                    strip.refresh(&root);
                } else {
                    let strip = plan::PlanStripState::discover(&root);
                    if !strip.summaries.is_empty() {
                        state.plan_strip = Some(strip);
                    }
                }
                state.window.request_redraw();
            }
        }

        // ------------------------------------------------------------------
        // Debounced auto-search: trigger execute_search if query has been
        // stable for 200ms and a compiled regex is ready.
        // ------------------------------------------------------------------
        if state
            .search_overlay
            .as_ref()
            .map(|so| so.should_auto_search())
            .unwrap_or(false)
        {
            if let Some(ref mut overlay) = state.search_overlay {
                let pane_rows: Vec<(PaneId, Vec<String>)> = state
                    .panes
                    .iter()
                    .map(|(&id, t)| (id, t.all_text_rows()))
                    .collect();
                overlay.execute_search(&pane_rows);
            }
            state.window.request_redraw();
        }

        // ------------------------------------------------------------------
        // Poll all panes for new PTY data via wakeup signals.
        //
        // The alacritty reader thread processes PTY bytes and sends `Wakeup`
        // events via `ArcTermEventListener::send_event`. We drain those here
        // and process all structured content that arrived since the last call.
        // ------------------------------------------------------------------
        let mut got_data = false;
        let mut closed_panes: Vec<PaneId> = Vec::new();

        // Collect pane IDs to avoid borrow checker conflicts.
        let pane_ids: Vec<PaneId> = state.panes.keys().copied().collect();

        for id in pane_ids {
            #[cfg(feature = "latency-trace")]
            let t0 = TraceInstant::now();

            let had_wakeup = if let Some(terminal) = state.panes.get_mut(&id) {
                terminal.has_wakeup()
            } else {
                false
            };

            // Check if the terminal has exited.
            let has_exited = state.panes.get(&id).is_some_and(|t| t.has_exited());
            if has_exited {
                log::info!("PTY child exited for pane {:?}", id);
                closed_panes.push(id);
            }

            if had_wakeup || has_exited {
                if let Some(terminal) = state.panes.get_mut(&id) {
                    // Drain completed OSC 7770 blocks.
                    let completed = terminal.take_completed_blocks();
                    if !completed.is_empty() {
                        let cursor_row = terminal.cursor_row();
                        let pane_blocks = state.structured_blocks.entry(id).or_default();
                        for acc in completed {
                            let attrs: Vec<(String, String)> = acc.attrs.into_iter().collect();
                            let rendered = state.highlight_engine.render_block(
                                acc.content_type.clone(),
                                &acc.buffer,
                                &attrs,
                            );
                            let line_count = rendered.len();
                            pane_blocks.push(StructuredBlock {
                                block_type: acc.content_type,
                                start_row: cursor_row.saturating_sub(line_count),
                                line_count,
                                rendered_lines: rendered,
                                raw_content: acc.buffer,
                            });
                        }
                    }

                    // Drain Kitty inline images decoded asynchronously.
                    if let Some(img_rx) = state.image_channels.get_mut(&id) {
                        while let Ok(img) = img_rx.try_recv() {
                            state.renderer.upload_image(
                                img.command.image_id,
                                &img.rgba,
                                img.width,
                                img.height,
                            );
                            let sf = state.window.scale_factor() as f32;
                            let cell_h = state.renderer.text.cell_size.height * sf;
                            let cursor_row_y = terminal.cursor_row() as f32 * cell_h;
                            let image_w = img.width as f32;
                            let image_h = img.height as f32;
                            state.renderer.image_placements.push((
                                img.command.image_id,
                                [0.0, cursor_row_y, image_w, image_h],
                            ));
                        }
                    }

                    // Auto-detect structured content in newly-written rows.
                    // Lock Term briefly to extract a snapshot for detection, then unlock.
                    // The snapshot is cached for reuse in RedrawRequested to avoid
                    // taking a second snapshot per pane per frame.
                    let cursor_row = terminal.cursor_row();
                    #[cfg(feature = "latency-trace")]
                    let t_snap = TraceInstant::now();
                    let snapshot = {
                        let term = terminal.lock_term();
                        arcterm_render::snapshot_from_term(&*term)
                    };
                    #[cfg(feature = "latency-trace")]
                    log::debug!("[latency] snapshot acquired in {:?}", t_snap.elapsed());
                    if let Some(detector) = state.auto_detectors.get_mut(&id) {
                        let detections = detector.scan_rows(&snapshot, cursor_row);
                        if !detections.is_empty() {
                            let pane_blocks = state.structured_blocks.entry(id).or_default();
                            for det in detections {
                                let attrs: Vec<(String, String)> = det.attrs.clone();
                                let rendered = state.highlight_engine.render_block(
                                    det.content_type.clone(),
                                    &det.content,
                                    &attrs,
                                );
                                let line_count = rendered.len();
                                pane_blocks.push(StructuredBlock {
                                    block_type: det.content_type,
                                    start_row: det.start_row,
                                    line_count,
                                    rendered_lines: rendered,
                                    raw_content: det.content,
                                });
                            }
                        }
                    }
                    // Cache snapshot for reuse in RedrawRequested.
                    state.cached_snapshots.insert(id, snapshot);
                }

                // Drain OSC 133 exit codes into PaneContext.
                {
                    let (exit_codes, cwd) = if let Some(terminal) = state.panes.get_mut(&id) {
                        (terminal.take_exit_codes(), terminal.cwd())
                    } else {
                        (Vec::new(), None)
                    };
                    if !exit_codes.is_empty() {
                        let last_code = *exit_codes.last().unwrap();
                        {
                            let ctx = state.pane_contexts.entry(id).or_insert_with(|| {
                                context::PaneContext::new(200)
                            });
                            ctx.set_exit_code(last_code);
                        }
                        if last_code != 0 && state.last_ai_pane.is_some() {
                            let maybe_err = state
                                .pane_contexts
                                .get(&id)
                                .and_then(|ctx| ctx.error_context_for(id, cwd));
                            if let Some(err_ctx) = maybe_err {
                                state.pending_errors.push(err_ctx);
                            }
                        }
                    }
                }

                // Drain MCP tool-list queries and write responses back to PTY.
                {
                    let tool_queries = state.panes.get_mut(&id)
                        .map(|t| t.take_tool_queries())
                        .unwrap_or_default();

                    if !tool_queries.is_empty()
                        && let Some(ref mgr) = state.plugin_manager
                    {
                        let tools = mgr.list_tools();
                        let entries: Vec<String> = tools.iter().map(|t| {
                            format!(
                                "{{\"name\":{},\"description\":{},\"inputSchema\":{}}}",
                                serde_json::Value::String(t.name.clone()),
                                serde_json::Value::String(t.description.clone()),
                                t.input_schema.as_str(),
                            )
                        }).collect();
                        let json = format!("[{}]", entries.join(","));
                        use base64::Engine as _;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(json.as_bytes());
                        let response = format!("\x1b]7770;tools/response;{}\x07", b64);
                        if let Some(terminal) = state.panes.get_mut(&id) {
                            terminal.write_input(response.as_bytes());
                        }
                    }
                }

                // Drain MCP tool calls and write results back to PTY.
                {
                    let tool_calls = state.panes.get_mut(&id)
                        .map(|t| t.take_tool_calls())
                        .unwrap_or_default();

                    for (name, args_json) in tool_calls {
                        let result_json = if let Some(ref mgr) = state.plugin_manager {
                            mgr.call_tool(&name, &args_json)
                                .unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
                        } else {
                            "{\"error\":\"plugin manager unavailable\"}".to_string()
                        };
                        use base64::Engine as _;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(result_json.as_bytes());
                        let response = format!("\x1b]7770;tools/result;result={}\x07", b64);
                        if let Some(terminal) = state.panes.get_mut(&id) {
                            terminal.write_input(response.as_bytes());
                        }
                    }
                }

                // Drain cross-pane context queries.
                {
                    let queries = state.panes.get_mut(&id)
                        .map(|t| t.take_context_queries())
                        .unwrap_or_default();

                    if !queries.is_empty() {
                        let siblings = context::collect_sibling_contexts(
                            &state.pane_contexts,
                            &state.panes,
                            id,
                        );
                        let response_bytes = context::format_context_osc7770(&siblings);
                        if let Some(terminal) = state.panes.get_mut(&id) {
                            terminal.write_input(&response_bytes);
                        }
                    }
                }

                // Update last_ai_pane when an AI-detected pane receives data.
                let is_ai_pane = state
                    .ai_states
                    .get(&id)
                    .map(|s| s.kind.is_some())
                    .unwrap_or(false);
                if is_ai_pane {
                    state.last_ai_pane = Some(id);
                }

                if had_wakeup {
                    got_data = true;
                }

                #[cfg(feature = "latency-trace")]
                log::debug!("[latency] PTY wakeup processed in {:?}", t0.elapsed());
            }
        }

        // Remove closed panes and associated state.
        // Also update the layout tree for each closed pane so stale Leaf nodes
        // are pruned and siblings are promoted.
        let active = state.tab_manager.active;
        for id in closed_panes {
            // Update layout tree: promote sibling when a split loses a child.
            if let Some(new_root) = state.tab_layouts[active].close(id) {
                state.tab_layouts[active] = new_root;
            }

            state.panes.remove(&id);
            state.image_channels.remove(&id);
            state.auto_detectors.remove(&id);
            state.structured_blocks.remove(&id);
            state.cached_snapshots.remove(&id);
            state.ai_states.remove(&id);
            state.pane_contexts.remove(&id);
            if state.last_ai_pane == Some(id) {
                state.last_ai_pane = None;
            }

            // Notify plugins that a pane was closed.
            if let Some(ref tx) = state.plugin_event_tx {
                let _ = tx.send(arcterm_plugin::manager::PluginEvent::PaneClosed(
                    format!("{:?}", id),
                ));
            }
        }

        // When ALL panes are gone, the session has ended — exit immediately.
        if state.panes.is_empty() {
            log::info!("All panes closed — exiting");
            event_loop.exit();
            return;
        }

        // If the focused pane was among those that exited, move focus to the
        // first remaining pane in the active tab's layout tree.
        {
            let focused = state.focused_pane();
            if !state.panes.contains_key(&focused) {
                let remaining = state.tab_layouts[active].all_pane_ids();
                if let Some(&new_focus) = remaining.first() {
                    state.set_focused_pane(new_focus);
                }
            }
        }

        // Apply deferred window resize (coalesced from `WindowEvent::Resized`).
        // Multiple resize events between frames overwrite `pending_resize`; only
        // the last size is processed here, once per frame.
        if let Some(size) = state.pending_resize.take() {
            state.renderer.resize(size.width, size.height);
            let rects = state.compute_pane_rects();
            let (cell_w, cell_h) = state.cell_dims();
            for (id, rect) in &rects {
                let (new_rows, new_cols) = state.grid_size_for_rect(*rect);
                if let Some(terminal) = state.panes.get_mut(id) {
                    terminal.resize(new_cols, new_rows, cell_w, cell_h);
                }
            }
            state.window.request_redraw();
        }

        if got_data {
            // Clear selection and scroll-to-live on the focused pane only.
            let focused = state.focused_pane();
            if let Some(terminal) = state.panes.get_mut(&focused)
                && terminal.scroll_offset() > 0
            {
                state.selection.clear();
                terminal.set_scroll_offset(0);
            }
            state.window.request_redraw();

            // Use WaitUntil with a short deadline to coalesce rapid PTY wakeups
            // without busy-spinning.  This gives the kernel ~2ms to accumulate
            // more output before the next frame, reducing CPU usage during heavy
            // output while keeping latency low.
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + std::time::Duration::from_millis(2),
            ));
        } else {
            // No new data — switch to Wait immediately so the event loop
            // sleeps until the next wakeup, keyboard event, or window event.
            event_loop.set_control_flow(ControlFlow::Wait);
        }

        // DSR/DA replies are handled by ArcTermEventListener::send_event(PtyWrite).
        // No need to drain take_pending_replies() here.

        // Wire window title from focused pane.
        {
            let focused = state.focused_pane();
            if let Some(terminal) = state.panes.get(&focused) {
                let title = terminal.title();
                if let Some(t) = title
                    && !t.is_empty()
                {
                    state.window.set_title(&t);
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                if let Err(e) = state.save_session() {
                    log::error!("Failed to save session on close: {e}");
                }
                event_loop.exit();
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }

            // -----------------------------------------------------------------
            // Window resize — recompute rects and resize all panes.
            // -----------------------------------------------------------------
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    // Defer the actual resize work to `about_to_wait` so that
                    // multiple `Resized` events arriving in a single frame (e.g.
                    // during a rapid window drag) are coalesced into one resize.
                    state.pending_resize = Some(size);
                    state.window.request_redraw();
                }
            }

            // -----------------------------------------------------------------
            // Mouse cursor movement — extend drag selection OR border drag.
            // -----------------------------------------------------------------
            WindowEvent::CursorMoved { position, .. } => {
                state.last_cursor_position = (position.x, position.y);

                // Border drag resize.
                if let Some(drag_id) = state.drag_pane {
                    let rects = state.compute_pane_rects();
                    if let Some(drag_rect) = rects.get(&drag_id) {
                        let px = position.x as f32;
                        // Determine delta based on whether it's closer to left/right or top/bottom edge.
                        let left_dist = (px - drag_rect.x).abs();
                        let right_dist = (px - (drag_rect.x + drag_rect.width)).abs();
                        let is_right_edge = right_dist < left_dist;

                        let delta = if is_right_edge {
                            // Dragging the right edge: moving right increases ratio.
                            let available_w = rects.values().map(|r| r.width).sum::<f32>() + rects.len() as f32 * BORDER_PX;
                            if available_w > 0.0 {
                                // Compute fraction of new position vs total width.
                                let new_ratio = (px - drag_rect.x) / (available_w - BORDER_PX);
                                let old_ratio = drag_rect.width / (available_w - BORDER_PX);
                                new_ratio - old_ratio
                            } else {
                                0.0
                            }
                        } else {
                            0.0
                        };

                        if delta.abs() > 0.001 {
                            let active = state.tab_manager.active;
                            state.tab_layouts[active].resize_split(drag_id, delta);
                            state.window.request_redraw();
                        }
                    }
                    return;
                }

                // Extend drag selection.
                if state.selection.mode != SelectionMode::None {
                    let focused = state.focused_pane();
                    let rects = state.compute_pane_rects();
                    if let Some(rect) = rects.get(&focused) {
                        let cell = cursor_to_cell_in_rect(state, position.x, position.y, *rect);
                        state.selection.update(cell);
                        state.window.request_redraw();
                    }
                }
            }

            // -----------------------------------------------------------------
            // Mouse button press/release — click-to-focus, selection, border drag.
            // -----------------------------------------------------------------
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                if button == MouseButton::Left {
                    match btn_state {
                        ElementState::Pressed => {
                            let (px, py) = state.last_cursor_position;
                            let pxf = px as f32;
                            let pyf = py as f32;

                            let rects = state.compute_pane_rects();

                            // --- Tab bar click ---
                            let sf = state.window.scale_factor() as f32;
                            let tab_h = if state.config.multiplexer.show_tab_bar && state.tab_manager.tab_count() > 1 {
                                arcterm_render::tab_bar_height(&state.renderer.text.cell_size, sf)
                            } else {
                                0.0
                            };

                            if tab_h > 0.0 && pyf < tab_h {
                                let win_w = state.renderer.gpu.surface_config.width as f32;
                                let tab_count = state.tab_manager.tab_count();
                                let tab_w = win_w / tab_count as f32;
                                let clicked_tab = (pxf / tab_w).floor() as usize;
                                let clicked_tab = clicked_tab.min(tab_count - 1);
                                state.tab_manager.switch_to(clicked_tab);
                                state.window.request_redraw();
                                return;
                            }

                            // --- Copy button click ---
                            // Check if click falls within a code block copy button.
                            let mut copy_handled = false;
                            for (pane_id, btn_rect, block_idx) in &state.copy_button_rects {
                                let [bx, by, bw, bh] = *btn_rect;
                                if pxf >= bx && pxf <= bx + bw && pyf >= by && pyf <= by + bh {
                                    // Hit: copy the block's raw content to clipboard.
                                    if let Some(blocks) = state.structured_blocks.get(pane_id)
                                        && let Some(block) = blocks.get(*block_idx)
                                        && let Some(cb) = &mut state.clipboard
                                        && let Err(e) = cb.copy(&block.raw_content)
                                    {
                                        log::warn!("Copy button clipboard write failed: {e}");
                                    }
                                    copy_handled = true;
                                    break;
                                }
                            }
                            if copy_handled {
                                return;
                            }

                            // --- Border drag detection ---
                            let mut on_border = false;
                            for (&id, rect) in &rects {
                                // Check if we're near the right edge of this pane (which is a border).
                                let right_edge = rect.x + rect.width;
                                if (pxf - right_edge).abs() < BORDER_DRAG_THRESHOLD
                                    && pyf >= rect.y
                                    && pyf < rect.y + rect.height
                                {
                                    state.drag_pane = Some(id);
                                    on_border = true;
                                    break;
                                }
                                // Check bottom edge.
                                let bottom_edge = rect.y + rect.height;
                                if (pyf - bottom_edge).abs() < BORDER_DRAG_THRESHOLD
                                    && pxf >= rect.x
                                    && pxf < rect.x + rect.width
                                {
                                    state.drag_pane = Some(id);
                                    on_border = true;
                                    break;
                                }
                            }

                            if on_border {
                                return;
                            }

                            // --- Click-to-focus ---
                            let clicked_pane = rects.iter().find_map(|(&id, rect)| {
                                if rect.contains(pxf, pyf) { Some(id) } else { None }
                            });

                            if let Some(new_focus) = clicked_pane {
                                let current_focus = state.focused_pane();
                                if new_focus != current_focus {
                                    state.set_focused_pane(new_focus);
                                    state.selection.clear();
                                    state.window.request_redraw();
                                }
                            }

                            // --- Selection ---
                            let focused = state.focused_pane();
                            if let Some(&focus_rect) = rects.get(&focused)
                                && focus_rect.contains(pxf, pyf)
                            {
                                let now = Instant::now();
                                let multi = state
                                    .last_click_time
                                    .map(|prev| {
                                        now.duration_since(prev) < Duration::from_millis(MULTI_CLICK_INTERVAL_MS)
                                    })
                                    .unwrap_or(false);

                                if multi {
                                    state.click_count = (state.click_count + 1).min(3);
                                } else {
                                    state.click_count = 1;
                                }
                                state.last_click_time = Some(now);

                                let cell = cursor_to_cell_in_rect(state, px, py, focus_rect);
                                let mode = match state.click_count {
                                    1 => SelectionMode::Character,
                                    2 => SelectionMode::Word,
                                    _ => SelectionMode::Line,
                                };
                                state.selection.start(cell, mode);
                                state.window.request_redraw();
                            }
                        }
                        ElementState::Released => {
                            // Stop border drag.
                            state.drag_pane = None;
                            // Keep the selection visible but stop extending it.
                        }
                    }
                }
            }

            // -----------------------------------------------------------------
            // Mouse wheel — scroll the focused pane's viewport.
            // -----------------------------------------------------------------
            WindowEvent::MouseWheel { delta, .. } => {
                let lines: i32 = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as i32,
                    MouseScrollDelta::PixelDelta(pos) => {
                        let cell_h = state.renderer.text.cell_size.height as f64;
                        (pos.y / cell_h).round() as i32
                    }
                };

                if lines != 0 {
                    let focused = state.focused_pane();
                    if let Some(terminal) = state.panes.get_mut(&focused) {
                        let current = terminal.scroll_offset() as i32;
                        let new_offset = (current - lines * SCROLL_LINES_PER_TICK as i32)
                            .max(0) as usize;
                        terminal.set_scroll_offset(new_offset);
                        state.window.request_redraw();
                    }
                }
            }

            // -----------------------------------------------------------------
            // Redraw — render all panes, borders, and tab bar.
            // -----------------------------------------------------------------
            WindowEvent::RedrawRequested => {
                // FPS counter.
                state.fps_frame_count += 1;
                let fps_elapsed = state.fps_last_log.elapsed();
                if fps_elapsed >= Duration::from_secs(5) {
                    let fps = state.fps_frame_count as f64 / fps_elapsed.as_secs_f64();
                    log::debug!("fps: {:.1} ({} frames in {:.1}s)", fps, state.fps_frame_count, fps_elapsed.as_secs_f64());
                    state.fps_frame_count = 0;
                    state.fps_last_log = Instant::now();
                }

                #[cfg(feature = "latency-trace")]
                let t0 = TraceInstant::now();

                let scale = state.window.scale_factor();
                let sf = scale as f32;
                let rects = state.compute_pane_rects();
                let focused = state.focused_pane();

                // Recompute selection quads for the focused pane.
                {
                    if let (Some(&focus_rect), Some(terminal)) =
                        (rects.get(&focused), state.panes.get(&focused))
                    {
                        let cell_w = state.renderer.text.cell_size.width;
                        let cell_h = state.renderer.text.cell_size.height;
                        state.selection_quads = generate_selection_quads(
                            &state.selection,
                            terminal.rows(),
                            terminal.cols(),
                            cell_w,
                            cell_h,
                            sf,
                        );
                        let _ = focus_rect; // rect is used contextually for offset in future
                        log::trace!(
                            "selection quads: {} rect(s) for mode {:?}",
                            state.selection_quads.len(),
                            state.selection.mode
                        );
                    }
                }

                // Collect overlay quads: borders + tab bar.
                let mut overlay_quads: Vec<OverlayQuad> = Vec::new();

                // Tab bar (only when > 1 tab and show_tab_bar is true).
                if state.config.multiplexer.show_tab_bar && state.tab_manager.tab_count() > 1 {
                    let win_w = state.renderer.gpu.surface_config.width as f32;
                    let tab_quads = arcterm_render::render_tab_bar_quads(
                        state.tab_manager.tab_count(),
                        state.tab_manager.active,
                        &state.renderer.text.cell_size,
                        sf,
                        win_w,
                        &state.renderer.palette,
                    );
                    for q in tab_quads {
                        overlay_quads.push(OverlayQuad {
                            rect: q.rect,
                            color: q.color,
                        });
                    }
                }

                // Pane borders from the active layout tree.
                {
                    let available = state.pane_area();
                    let tab = state.tab_manager.active_tab();
                    // Only draw borders when NOT zoomed (zoomed = single pane fills area).
                    if tab.zoomed.is_none() {
                        let border_quads = state.active_layout().compute_border_quads(
                            available,
                            BORDER_PX,
                            focused,
                            BORDER_COLOR_NORMAL,
                            BORDER_COLOR_FOCUS,
                        );
                        for bq in border_quads {
                            overlay_quads.push(OverlayQuad {
                                rect: [bq.rect.x, bq.rect.y, bq.rect.width, bq.rect.height],
                                color: [
                                    bq.color[0] as f32 / 255.0,
                                    bq.color[1] as f32 / 255.0,
                                    bq.color[2] as f32 / 255.0,
                                    bq.color[3] as f32 / 255.0,
                                ],
                            });
                        }
                    }
                }

                // Build pane render infos.
                let mut pane_infos: Vec<PaneRenderInfo<'_>> = Vec::new();

                // Normal multi-pane render.
                // Reuse cached snapshots from about_to_wait when available;
                // fall back to a fresh snapshot for panes that had no PTY wakeup.
                let mut pane_frames: Vec<(PaneId, PixelRect, arcterm_render::RenderSnapshot)> = Vec::new();
                for (id, rect) in &rects {
                    if rect.width <= 0.0 || rect.height <= 0.0 {
                        continue;
                    }
                    if let Some(t) = state.panes.get(id) {
                        let snapshot = state.cached_snapshots.remove(id).unwrap_or_else(|| {
                            let term = t.lock_term();
                            arcterm_render::snapshot_from_term(&*term)
                        });
                        pane_frames.push((*id, *rect, snapshot));
                    }
                }

                // Clear stale image placements from the previous frame.
                // New placements are pushed in about_to_wait when PTY output
                // delivers Kitty images.  We retain the image_store (GPU textures)
                // across frames so images can persist on screen.
                // TODO(phase-5): implement proper placement tracking per image_id.
                state.renderer.image_placements.retain(|_| false);

                // Recompute copy button rects for this frame.
                state.copy_button_rects.clear();
                let cell_h_phys = state.renderer.text.cell_size.height * sf;
                let cell_w_phys = state.renderer.text.cell_size.width * sf;
                let _ = cell_w_phys; // may be used for future sizing

                // Empty block list used as a default when a pane has no structured blocks.
                let empty_blocks: Vec<StructuredBlock> = Vec::new();

                for (pane_id, rect, _) in &pane_frames {
                    let blocks = state.structured_blocks.get(pane_id).unwrap_or(&empty_blocks);
                    let pw = rect.width;
                    let py = rect.y;
                    let px = rect.x;
                    for (block_idx, block) in blocks.iter().enumerate() {
                        if matches!(block.block_type, ContentType::CodeBlock) {
                            let block_y = py + block.start_row as f32 * cell_h_phys;
                            let btn_size = 14.0_f32 * sf;
                            let btn_x = px + pw - btn_size - 4.0 * sf;
                            let btn_y = block_y + 2.0 * sf;
                            state.copy_button_rects.push((
                                *pane_id,
                                [btn_x, btn_y, btn_size, btn_size],
                                block_idx,
                            ));
                        }
                    }
                }

                for (pane_id, rect, snapshot) in &pane_frames {
                    let blocks = state.structured_blocks.get(pane_id).unwrap_or(&empty_blocks);
                    pane_infos.push(PaneRenderInfo {
                        snapshot,
                        rect: [rect.x, rect.y, rect.width, rect.height],
                        structured_blocks: blocks,
                    });
                }

                // Build palette overlay quads and text when the palette is open.
                let mut palette_text: Vec<(String, f32, f32)> = Vec::new();
                if let Some(pal) = &state.palette_mode {
                    let win_w = state.renderer.gpu.surface_config.width as f32;
                    let win_h = state.renderer.gpu.surface_config.height as f32;
                    let cell_w = state.renderer.text.cell_size.width;
                    let cell_h = state.renderer.text.cell_size.height;
                    let pal_sf = sf;

                    // Quads.
                    let pal_quads = pal.render_quads(win_w, win_h, cell_w, cell_h, pal_sf);
                    for pq in pal_quads {
                        overlay_quads.push(OverlayQuad {
                            rect: pq.rect,
                            color: pq.color,
                        });
                    }

                    // Text.
                    let pal_texts = pal.render_text_content(win_w, win_h, cell_w, cell_h, pal_sf);
                    for pt in pal_texts {
                        palette_text.push((pt.text, pt.x, pt.y));
                    }
                }

                // Build workspace switcher overlay quads and text when the switcher is open.
                if let Some(sw) = &state.workspace_switcher {
                    let win_w = state.renderer.gpu.surface_config.width as f32;
                    let win_h = state.renderer.gpu.surface_config.height as f32;
                    let cell_w = state.renderer.text.cell_size.width;
                    let cell_h = state.renderer.text.cell_size.height;
                    let sw_sf = sf;

                    // Quads.
                    let sw_quads = sw.render_quads(win_w, win_h, cell_w, cell_h, sw_sf);
                    for pq in sw_quads {
                        overlay_quads.push(OverlayQuad {
                            rect: pq.rect,
                            color: pq.color,
                        });
                    }

                    // Text.
                    let sw_texts = sw.render_text_content(win_w, win_h, cell_w, cell_h, sw_sf);
                    for pt in sw_texts {
                        palette_text.push((pt.text, pt.x, pt.y));
                    }
                }

                // Plan strip — ambient status bar at the bottom of the window.
                if let Some(ref strip) = state.plan_strip {
                    let win_w = state.renderer.gpu.surface_config.width as f32;
                    let win_h = state.renderer.gpu.surface_config.height as f32;
                    let cell_h = state.renderer.text.cell_size.height * sf;
                    // One-row bar at the very bottom.
                    let strip_y = win_h - cell_h;
                    overlay_quads.push(OverlayQuad {
                        rect: [0.0, strip_y, win_w, cell_h],
                        color: [0.10, 0.11, 0.15, 0.92],
                    });
                    // Text label.
                    let text = strip.strip_text();
                    if !text.is_empty() {
                        palette_text.push((text, 8.0 * sf, strip_y + (cell_h - state.renderer.text.cell_size.height * sf) / 2.0));
                    }
                }

                // Config overlay review — diff view for pending overlays.
                if let Some(ref review) = state.overlay_review {
                    let win_w = state.renderer.gpu.surface_config.width as f32;
                    let win_h = state.renderer.gpu.surface_config.height as f32;

                    let (review_quads, review_texts) = review.render_quads(win_w, win_h);
                    for pq in review_quads {
                        overlay_quads.push(OverlayQuad {
                            rect: pq.rect,
                            color: pq.color,
                        });
                    }

                    // Render text lines: header at top of panel, then diff lines.
                    let cell_h = state.renderer.text.cell_size.height * sf;
                    let cell_w = state.renderer.text.cell_size.width * sf;
                    let panel_x = (win_w - (win_w * 0.80).max(400.0)) / 2.0 + cell_w;
                    let panel_y = (win_h - (win_h * 0.80).max(300.0)) / 2.0;
                    for (i, line) in review_texts.iter().enumerate() {
                        palette_text.push((
                            line.clone(),
                            panel_x,
                            panel_y + cell_h * i as f32,
                        ));
                    }
                }

                // Plan view — expanded modal overlay (like command palette).
                if let Some(ref view) = state.plan_view {
                    let win_w = state.renderer.gpu.surface_config.width as f32;
                    let win_h = state.renderer.gpu.surface_config.height as f32;
                    let cell_h = state.renderer.text.cell_size.height * sf;

                    let view_quads = view.render_quads(win_w, win_h, cell_h);
                    for pq in view_quads {
                        overlay_quads.push(OverlayQuad {
                            rect: pq.rect,
                            color: pq.color,
                        });
                    }

                    let cell_w = state.renderer.text.cell_size.width * sf;
                    let view_texts = view.render_text(win_w, win_h, cell_w, cell_h);
                    for pt in view_texts {
                        palette_text.push((pt.text, pt.x, pt.y));
                    }
                }

                // Search overlay — input bar and match highlight quads.
                if let Some(ref so) = state.search_overlay {
                    let win_w = state.renderer.gpu.surface_config.width as f32;
                    let _win_h = state.renderer.gpu.surface_config.height as f32;
                    let cell_h = state.renderer.text.cell_size.height * sf;
                    let cell_w = state.renderer.text.cell_size.width * sf;
                    let bar_h = cell_h * 1.5;

                    // Search input bar at the top of the window.
                    overlay_quads.push(OverlayQuad {
                        rect: [0.0, 0.0, win_w, bar_h],
                        color: [0.10, 0.11, 0.18, 0.92],
                    });
                    // Query text.
                    let match_info = if so.matches.is_empty() {
                        if so.error_msg.is_some() {
                            format!("/{} [invalid regex]", so.query)
                        } else {
                            format!("/{}", so.query)
                        }
                    } else {
                        format!("/{} — {}/{} matches", so.query, so.current_match + 1, so.matches.len())
                    };
                    palette_text.push((match_info, 8.0 * sf, (bar_h - cell_h) / 2.0));

                    // Match highlight quads on each pane's grid.
                    for (&pane_id, rect) in &rects {
                        if let Some(terminal) = state.panes.get(&pane_id) {
                            let total_rows = terminal.all_text_rows().len();
                            let visible_rows = terminal.rows();
                            let scroll_offset = terminal.scroll_offset();
                            let quads = so.match_quads_for_pane(
                                pane_id,
                                [rect.x, rect.y, rect.width, rect.height],
                                cell_w,
                                cell_h,
                                scroll_offset,
                                visible_rows,
                                total_rows,
                            );
                            for (q, _is_current) in quads {
                                overlay_quads.push(OverlayQuad {
                                    rect: q.rect,
                                    color: q.color,
                                });
                            }
                        }
                    }
                }

                // Collect plugin pane render infos from the active layout.
                let plugin_pane_infos: Vec<PluginPaneRenderInfo> = {
                    let mut out = Vec::new();
                    if let Some(ref mgr) = state.plugin_manager {
                        collect_plugin_panes(state.active_layout(), &rects, mgr, &mut out);
                    }
                    out
                };

                state.renderer.render_multipane(
                    &pane_infos,
                    &plugin_pane_infos,
                    &overlay_quads,
                    &palette_text,
                    scale,
                );

                #[cfg(feature = "latency-trace")]
                {
                    log::debug!("[latency] frame submitted in {:?}", t0.elapsed());

                    // Log key-to-frame-presented latency when a key press preceded this frame.
                    if let Some(kp_t0) = state.key_press_t0.take() {
                        log::info!("[latency] key → frame presented: {:?}", kp_t0.elapsed());
                    }

                    static FIRST_FRAME: std::sync::atomic::AtomicBool =
                        std::sync::atomic::AtomicBool::new(true);
                    if FIRST_FRAME.swap(false, std::sync::atomic::Ordering::Relaxed) {
                        log::info!(
                            "[latency] cold start → first frame: {:?}",
                            self.cold_start.elapsed()
                        );
                    }
                }
            }

            // -----------------------------------------------------------------
            // Keyboard input — routed through KeymapHandler.
            // -----------------------------------------------------------------
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    #[cfg(feature = "latency-trace")]
                    let t0 = TraceInstant::now();

                    // Capture key-press timestamp for key→frame latency measurement.
                    #[cfg(feature = "latency-trace")]
                    {
                        state.key_press_t0 = Some(TraceInstant::now());
                    }

                    let super_key = self.modifiers.super_key();

                    // Cmd+C — copy selection to clipboard (before keymap).
                    if super_key {
                        use winit::keyboard::Key;
                        if let Key::Character(ref s) = event.logical_key {
                            match s.as_str() {
                                "c" | "C" => {
                                    let focused = state.focused_pane();
                                    if let Some(terminal) = state.panes.get(&focused) {
                                        let snapshot = {
                                            let term = terminal.lock_term();
                                            arcterm_render::snapshot_from_term(&*term)
                                        };
                                        let text = state.selection.extract_text(&snapshot);
                                        if !text.is_empty()
                                            && let Some(cb) = &mut state.clipboard
                                            && let Err(e) = cb.copy(&text)
                                        {
                                            log::warn!("Clipboard copy failed: {e}");
                                        }
                                    }
                                    return;
                                }
                                "v" | "V" => {
                                    if let Some(cb) = &mut state.clipboard {
                                        match cb.paste() {
                                            Ok(text) => {
                                                let focused = state.focused_pane();
                                                if let Some(terminal) = state.panes.get_mut(&focused) {
                                                    let bracketed = terminal.bracketed_paste();
                                                    if bracketed {
                                                        let mut payload = b"\x1b[200~".to_vec();
                                                        payload.extend_from_slice(text.as_bytes());
                                                        payload.extend_from_slice(b"\x1b[201~");
                                                        terminal.write_input(&payload);
                                                    } else {
                                                        terminal.write_input(text.as_bytes());
                                                    }
                                                    state.window.request_redraw();
                                                }
                                            }
                                            Err(e) => {
                                                log::warn!("Clipboard paste failed: {e}");
                                            }
                                        }
                                    }
                                    return;
                                }
                                _ => {}
                            }
                        }
                    }

                    // Workspace switcher is modal — route ALL key input through it when open.
                    if let Some(switcher) = &mut state.workspace_switcher {
                        use palette::WorkspaceSwitcherEvent;
                        let sw_event = switcher.handle_input(&event, self.modifiers);
                        match sw_event {
                            WorkspaceSwitcherEvent::Close => {
                                state.workspace_switcher = None;
                                state.window.request_redraw();
                            }
                            WorkspaceSwitcherEvent::Open(path) => {
                                state.workspace_switcher = None;
                                match workspace::WorkspaceFile::load_from_file(&path) {
                                    Ok(ws) => {
                                        state.restore_workspace(&ws);
                                    }
                                    Err(e) => {
                                        log::error!(
                                            "Workspace switcher: failed to load {:?}: {e}",
                                            path
                                        );
                                    }
                                }
                                state.window.request_redraw();
                            }
                            WorkspaceSwitcherEvent::Consumed => {
                                state.window.request_redraw();
                            }
                        }
                        return;
                    }

                    // Palette is modal — route ALL key input through it when open.
                    if let Some(palette) = &mut state.palette_mode {
                        use palette::PaletteEvent;
                        let palette_event = palette.handle_input(&event, self.modifiers);
                        match palette_event {
                            PaletteEvent::Close => {
                                state.palette_mode = None;
                                state.window.request_redraw();
                            }
                            PaletteEvent::Execute(palette_action) => {
                                let key_action = palette_action.to_key_action();
                                state.palette_mode = None;
                                // Dispatch the converted KeyAction through the same
                                // path used by regular keymap bindings.
                                execute_key_action(state, event_loop, key_action);
                                state.window.request_redraw();
                            }
                            PaletteEvent::Consumed => {
                                state.window.request_redraw();
                            }
                        }
                        return;
                    }

                    // Overlay review is modal — route ALL key input through it when open.
                    if state.overlay_review.is_some() {
                        use overlay::OverlayAction;
                        let mut review = state.overlay_review.take().unwrap();
                        let action = review.handle_key(&event.logical_key);
                        match action {
                            OverlayAction::Accept => {
                                let src = review.current_path().to_path_buf();
                                let dst = config::accepted_dir().join(
                                    src.file_name().unwrap_or_default(),
                                );
                                if let Err(e) = std::fs::create_dir_all(config::accepted_dir()) {
                                    log::warn!("overlay: cannot create accepted dir: {e}");
                                }
                                if let Err(e) = std::fs::rename(&src, &dst) {
                                    log::warn!("overlay: failed to move overlay to accepted: {e}");
                                }
                                // Advance to next file or close.
                                let mut files = review.pending_files;
                                files.remove(review.current_index);
                                if files.is_empty() {
                                    // No more pending — close overlay.
                                } else {
                                    let idx = review.current_index.min(files.len() - 1);
                                    let (new_cfg, _) = config::ArctermConfig::load_with_overlays();
                                    state.config = new_cfg.clone();
                                    let diff = overlay::compute_diff(&new_cfg, &files[idx]);
                                    state.overlay_review = Some(overlay::OverlayReviewState {
                                        pending_files: files,
                                        current_index: idx,
                                        diff_text: diff,
                                        scroll_offset: 0,
                                    });
                                }
                                // Reload config with newly accepted overlays.
                                let (new_cfg, _) = config::ArctermConfig::load_with_overlays();
                                state.config = new_cfg;
                                state.window.request_redraw();
                            }
                            OverlayAction::Reject => {
                                let src = review.current_path().to_path_buf();
                                if let Err(e) = std::fs::remove_file(&src) {
                                    log::warn!("overlay: failed to delete pending overlay: {e}");
                                }
                                let mut files = review.pending_files;
                                files.remove(review.current_index);
                                if !files.is_empty() {
                                    let idx = review.current_index.min(files.len() - 1);
                                    let base_cfg = state.config.clone();
                                    let diff = overlay::compute_diff(&base_cfg, &files[idx]);
                                    state.overlay_review = Some(overlay::OverlayReviewState {
                                        pending_files: files,
                                        current_index: idx,
                                        diff_text: diff,
                                        scroll_offset: 0,
                                    });
                                }
                                state.window.request_redraw();
                            }
                            OverlayAction::Edit(path) => {
                                // Close overlay review and spawn .
                                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
                                let _ = std::process::Command::new(&editor).arg(&path).spawn();
                                // overlay_review stays None (taken above)
                                state.window.request_redraw();
                            }
                            OverlayAction::Close => {
                                // overlay_review stays None (already taken)
                                state.window.request_redraw();
                            }
                            OverlayAction::NextFile => {
                                let total = review.pending_files.len();
                                let next_idx = (review.current_index + 1).min(total - 1);
                                let base_cfg = state.config.clone();
                                let diff = overlay::compute_diff(&base_cfg, &review.pending_files[next_idx]);
                                review.current_index = next_idx;
                                review.diff_text = diff;
                                review.scroll_offset = 0;
                                state.overlay_review = Some(review);
                                state.window.request_redraw();
                            }
                            OverlayAction::PrevFile => {
                                let prev_idx = review.current_index.saturating_sub(1);
                                let base_cfg = state.config.clone();
                                let diff = overlay::compute_diff(&base_cfg, &review.pending_files[prev_idx]);
                                review.current_index = prev_idx;
                                review.diff_text = diff;
                                review.scroll_offset = 0;
                                state.overlay_review = Some(review);
                                state.window.request_redraw();
                            }
                            OverlayAction::Noop => {
                                state.overlay_review = Some(review);
                            }
                        }
                        return;
                    }

                    // Search overlay is modal — route ALL key input through it when open.
                    if state.search_overlay.is_some() {
                        use search::SearchAction;
                        // Take the overlay out of state to avoid borrow conflict.
                        let mut overlay = state.search_overlay.take().unwrap();
                        let search_action = overlay.handle_key(&event.logical_key);
                        match search_action {
                            SearchAction::Close => {
                                // Overlay stays None (already taken).
                                state.window.request_redraw();
                            }
                            SearchAction::Execute => {
                                let pane_rows: Vec<(PaneId, Vec<String>)> = state
                                    .panes
                                    .iter()
                                    .map(|(&id, t)| (id, t.all_text_rows()))
                                    .collect();
                                overlay.execute_search(&pane_rows);
                                state.search_overlay = Some(overlay);
                                state.window.request_redraw();
                            }
                            SearchAction::UpdateQuery => {
                                state.search_overlay = Some(overlay);
                                state.window.request_redraw();
                            }
                            SearchAction::NextMatch => {
                                overlay.next_match();
                                if let Some(m) = overlay.current().cloned() {
                                    let focused = state.focused_pane();
                                    if m.pane_id == focused
                                        && let Some(terminal) = state.panes.get_mut(&focused)
                                    {
                                        let total = terminal.all_text_rows().len();
                                        let visible = terminal.rows();
                                        terminal.set_scroll_offset(
                                            search::SearchOverlayState::scroll_offset_for_match(
                                                m.row_index,
                                                total,
                                                visible,
                                            ),
                                        );
                                    }
                                }
                                state.search_overlay = Some(overlay);
                                state.window.request_redraw();
                            }
                            SearchAction::PrevMatch => {
                                overlay.prev_match();
                                if let Some(m) = overlay.current().cloned() {
                                    let focused = state.focused_pane();
                                    if m.pane_id == focused
                                        && let Some(terminal) = state.panes.get_mut(&focused)
                                    {
                                        let total = terminal.all_text_rows().len();
                                        let visible = terminal.rows();
                                        terminal.set_scroll_offset(
                                            search::SearchOverlayState::scroll_offset_for_match(
                                                m.row_index,
                                                total,
                                                visible,
                                            ),
                                        );
                                    }
                                }
                                state.search_overlay = Some(overlay);
                                state.window.request_redraw();
                            }
                            SearchAction::Noop => {
                                state.search_overlay = Some(overlay);
                            }
                        }
                        return;
                    }

                    // Route through the keymap handler.
                    let focused_id = state.focused_pane();
                    let app_cursor = state
                        .panes
                        .get(&focused_id)
                        .map(|t| t.app_cursor_keys())
                        .unwrap_or(false);

                    let action = state.keymap.handle_key(&event, self.modifiers, app_cursor);

                    // Check if the focused pane is a plugin pane — if so, route
                    // key events to the plugin rather than to a PTY.
                    let focused_plugin_id: Option<String> = {
                        fn find_plugin_id(node: &PaneNode, target: PaneId) -> Option<String> {
                            match node {
                                PaneNode::PluginPane { pane_id, plugin_id } => {
                                    if *pane_id == target { Some(plugin_id.clone()) } else { None }
                                }
                                PaneNode::Leaf { .. } => None,
                                PaneNode::HSplit { left, right, .. } => {
                                    find_plugin_id(left, target)
                                        .or_else(|| find_plugin_id(right, target))
                                }
                                PaneNode::VSplit { top, bottom, .. } => {
                                    find_plugin_id(top, target)
                                        .or_else(|| find_plugin_id(bottom, target))
                                }
                            }
                        }
                        find_plugin_id(state.active_layout(), focused_id)
                    };

                    match action {
                        KeyAction::Forward(bytes) => {
                            #[cfg(feature = "latency-trace")]
                            log::debug!(
                                "[latency] key → PTY write ({} bytes) after {:?}",
                                bytes.len(),
                                t0.elapsed()
                            );

                            // If the focused pane is a plugin pane, forward as key-input.
                            if let Some(ref pid) = focused_plugin_id {
                                if let Some(ref mgr) = state.plugin_manager {
                                    use winit::keyboard::Key;
                                    let key_char: Option<String> =
                                        if let Key::Character(ref s) = event.logical_key {
                                            Some(s.to_string())
                                        } else {
                                            None
                                        };
                                    let key_name = format!("{:?}", event.logical_key);
                                    let consumed = mgr.send_key_input(
                                        pid,
                                        key_char,
                                        key_name,
                                        self.modifiers.control_key(),
                                        self.modifiers.alt_key(),
                                        self.modifiers.shift_key(),
                                    );
                                    if consumed {
                                        state.window.request_redraw();
                                    }
                                }
                            } else if let Some(terminal) = state.panes.get_mut(&focused_id) {
                                terminal.write_input(&bytes);
                                // ISSUE-002: request_redraw() must follow write_input so the
                                // terminal display refreshes immediately after keyboard input.
                                // This is a winit integration concern — do not remove this call.
                                state.window.request_redraw();
                            }
                        }

                        KeyAction::NavigatePane(dir) => {
                            // ── Neovim-aware pane crossing ──────────────────
                            // 1. Retrieve (or refresh) the cached Neovim state
                            //    for the focused pane.
                            let child_pid = state
                                .panes
                                .get(&focused_id)
                                .and_then(|t| t.child_pid());

                            let nvim_state = {
                                let needs_refresh = state
                                    .nvim_states
                                    .get(&focused_id)
                                    .map(|s| !s.is_fresh())
                                    .unwrap_or(true);

                                if needs_refresh {
                                    let fresh = neovim::NeovimState::check(child_pid);
                                    state.nvim_states.insert(focused_id, fresh);
                                }

                                // Borrow the state immutably for the check below.
                                let s = state.nvim_states.get(&focused_id).unwrap();
                                (s.is_nvim, s.socket_path.clone())
                            };

                            // 2. If the pane is running Neovim and we have a
                            //    socket path, query whether Neovim has a split
                            //    in the requested direction.
                            let nvim_consumed = if nvim_state.0 {
                                if let Some(ref socket_path) = nvim_state.1 {
                                    // Use block_in_place — we are already inside
                                    // the Tokio runtime context established in
                                    // main().  This runs the synchronous socket
                                    // I/O on the current thread without blocking
                                    // the async executor.
                                    tokio::task::block_in_place(|| {
                                        match neovim::NvimRpcClient::connect(socket_path) {
                                            Ok(mut client) => {
                                                match neovim::has_nvim_neighbor(&mut client, dir) {
                                                    Ok(true) => {
                                                        // Neovim has a split in this
                                                        // direction — forward the key.
                                                        let ctrl_byte: &[u8] = match dir {
                                                            Direction::Left  => &[0x08], // Ctrl+h
                                                            Direction::Down  => &[0x0A], // Ctrl+j
                                                            Direction::Up    => &[0x0B], // Ctrl+k
                                                            Direction::Right => &[0x0C], // Ctrl+l
                                                        };
                                                        if let Some(terminal) =
                                                            state.panes.get_mut(&focused_id)
                                                        {
                                                            terminal.write_input(ctrl_byte);
                                                        }
                                                        true // consumed by Neovim
                                                    }
                                                    Ok(false) => false, // fall through
                                                    Err(e) => {
                                                        log::debug!(
                                                            "nvim RPC query failed for {:?}: {e}",
                                                            focused_id
                                                        );
                                                        false // fall through
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                log::debug!(
                                                    "nvim socket connect failed for {:?}: {e}",
                                                    focused_id
                                                );
                                                false // fall through
                                            }
                                        }
                                    })
                                } else {
                                    false // no socket → fall through
                                }
                            } else {
                                false // not nvim → fall through
                            };

                            // Lazily refresh AI detection for the focused pane
                            // (5-second TTL, same pattern as Neovim detection).
                            {
                                let needs_ai_refresh = state
                                    .ai_states
                                    .get(&focused_id)
                                    .map(|s| !s.is_fresh())
                                    .unwrap_or(true);

                                if needs_ai_refresh {
                                    let fresh = ai_detect::AiAgentState::check(child_pid);
                                    // SC-1: log when a new AI agent is first detected.
                                    if let Some(ref kind) = fresh.kind {
                                        let was_known = state
                                            .ai_states
                                            .get(&focused_id)
                                            .and_then(|s| s.kind.as_ref())
                                            .is_some();
                                        if !was_known {
                                            log::info!(
                                                "AI agent detected in pane {:?}: {:?}",
                                                focused_id,
                                                kind
                                            );
                                        }
                                        // Update last_ai_pane and sync ai_type into PaneContext.
                                        state.last_ai_pane = Some(focused_id);
                                        if let Some(ctx) = state.pane_contexts.get_mut(&focused_id) {
                                            ctx.ai_type = Some(kind.clone());
                                        }
                                    }
                                    state.ai_states.insert(focused_id, fresh);
                                }
                            }

                            // 3. If Neovim did not consume the key, use
                            //    arcterm's layout-based navigation.
                            if !nvim_consumed {
                                let rects = state.compute_pane_rects();
                                if let Some(new_focus) = state
                                    .active_layout()
                                    .focus_in_direction(focused_id, dir, &rects)
                                {
                                    state.set_focused_pane(new_focus);
                                    state.selection.clear();
                                    state.window.request_redraw();
                                }
                            }
                        }

                        KeyAction::Split(axis) => {
                            let rects = state.compute_pane_rects();
                            let focused = focused_id;
                            let focused_rect = rects.get(&focused).copied().unwrap_or(PixelRect {
                                x: 0.0,
                                y: 0.0,
                                width: 800.0,
                                height: 600.0,
                            });

                            // Compute size for the new pane (half of focused pane's rect).
                            let new_rect = match axis {
                                Axis::Horizontal => PixelRect {
                                    width: focused_rect.width / 2.0,
                                    ..focused_rect
                                },
                                Axis::Vertical => PixelRect {
                                    height: focused_rect.height / 2.0,
                                    ..focused_rect
                                },
                            };
                            let new_size = state.grid_size_for_rect(new_rect);
                            let new_id = state.spawn_pane(new_size);

                            // Also resize the original pane to its new half.
                            let orig_size = state.grid_size_for_rect(new_rect);
                            let (cell_w, cell_h) = state.cell_dims();
                            if let Some(terminal) = state.panes.get_mut(&focused) {
                                terminal.resize(orig_size.1, orig_size.0, cell_w, cell_h);
                            }

                            // Update the layout tree.
                            let active = state.tab_manager.active;
                            state.tab_layouts[active].split(focused, axis, new_id);

                            // Focus the new pane.
                            state.set_focused_pane(new_id);
                            state.window.request_redraw();
                        }

                        KeyAction::ClosePane => {
                            let focused = focused_id;
                            let active = state.tab_manager.active;
                            let pane_count = state.tab_layouts[active].all_pane_ids().len();

                            if pane_count <= 1 {
                                // Last pane in the tab — close the tab if possible.
                                let removed_ids = state.tab_manager.close_tab(active);
                                // Remove layout for this tab.
                                if active < state.tab_layouts.len() {
                                    state.tab_layouts.remove(active);
                                }
                                for id in removed_ids {
                                    let lid = id;
                                    state.panes.remove(&lid);
                                    // pty_channels removed (alacritty reader thread owns PTY)
                                    state.image_channels.remove(&lid);
                                    state.nvim_states.remove(&lid);
                                    state.ai_states.remove(&lid);
                                    state.pane_contexts.remove(&lid);
                                    if state.last_ai_pane == Some(lid) {
                                        state.last_ai_pane = None;
                                    }
                                }
                                // If no tabs left, exit.
                                if state.tab_manager.tab_count() == 0 {
                                    event_loop.exit();
                                    return;
                                }
                                // Focus the first pane of the now-active tab.
                                let new_focus = state.tab_manager.active_tab().focus;
                                state.set_focused_pane(new_focus);
                            } else {
                                // Multiple panes: promote sibling.
                                let replacement = state.tab_layouts[active].close(focused);
                                if let Some(new_root) = replacement {
                                    state.tab_layouts[active] = new_root;
                                }

                                // Remove the terminal and channel.
                                state.panes.remove(&focused);
                                // pty_channels removed (alacritty reader thread owns PTY)
                                state.image_channels.remove(&focused);
                                state.nvim_states.remove(&focused);
                                state.ai_states.remove(&focused);
                                state.pane_contexts.remove(&focused);
                                if state.last_ai_pane == Some(focused) {
                                    state.last_ai_pane = None;
                                }

                                // Focus the first remaining pane.
                                let remaining = state.tab_layouts[active].all_pane_ids();
                                if let Some(&new_focus) = remaining.first() {
                                    state.set_focused_pane(new_focus);
                                }
                            }
                            state.selection.clear();
                            state.window.request_redraw();
                        }

                        KeyAction::ToggleZoom => {
                            let focused = focused_id;
                            let tab = state.tab_manager.active_tab_mut();
                            if tab.zoomed == Some(focused) {
                                tab.zoomed = None;
                            } else {
                                tab.zoomed = Some(focused);
                            }
                            state.window.request_redraw();
                        }

                        KeyAction::ResizePane(dir) => {
                            let focused = focused_id;
                            let delta = match dir {
                                Direction::Right | Direction::Down => RESIZE_DELTA,
                                Direction::Left | Direction::Up => -RESIZE_DELTA,
                            };
                            let active = state.tab_manager.active;
                            state.tab_layouts[active].resize_split(focused, delta);
                            state.window.request_redraw();
                        }

                        KeyAction::NewTab => {
                            // Compute size for a full-window single pane.
                            let win_size = state.window.inner_size();
                            let full_size = state.renderer.grid_size_for_window(
                                win_size.width,
                                win_size.height,
                                state.window.scale_factor(),
                            );
                            let new_id = state.spawn_pane(full_size);
                            let tab_idx = state.tab_manager.add_tab(new_id);
                            // Add the layout tree for the new tab.
                            state.tab_layouts.push(PaneNode::Leaf { pane_id: new_id });
                            state.tab_manager.switch_to(tab_idx);
                            state.set_focused_pane(new_id);
                            state.selection.clear();
                            state.window.request_redraw();
                        }

                        KeyAction::SwitchTab(n) => {
                            // n is 1-indexed.
                            state.tab_manager.switch_to(n.saturating_sub(1));
                            // Focus the active pane of the newly-switched-to tab.
                            let new_focus = state.tab_manager.active_tab().focus;
                            state.set_focused_pane(new_focus);
                            state.selection.clear();
                            state.window.request_redraw();
                        }

                        KeyAction::CloseTab => {
                            let active = state.tab_manager.active;
                            let removed_ids = state.tab_manager.close_tab(active);
                            if !removed_ids.is_empty() {
                                // Remove layout for this tab.
                                if active < state.tab_layouts.len() {
                                    state.tab_layouts.remove(active);
                                }
                                for id in removed_ids {
                                    let lid = id;
                                    state.panes.remove(&lid);
                                    // pty_channels removed (alacritty reader thread owns PTY)
                                    state.image_channels.remove(&lid);
                                }
                                // Focus active tab's pane.
                                let new_focus = state.tab_manager.active_tab().focus;
                                state.set_focused_pane(new_focus);
                                state.selection.clear();
                                state.window.request_redraw();
                            }
                        }

                        KeyAction::OpenPalette => {
                            state.palette_mode = Some(PaletteState::new());
                            log::info!("Command palette: open");
                            state.window.request_redraw();
                        }

                        KeyAction::OpenWorkspaceSwitcher => {
                            let entries = workspace::discover_workspaces();
                            log::info!("Workspace switcher: open ({} entries)", entries.len());
                            state.workspace_switcher = Some(WorkspaceSwitcherState::new(entries));
                            state.window.request_redraw();
                        }

                        KeyAction::SaveWorkspace => {
                            // Generate a timestamp-based name: session-YYYYMMDD-HHMM
                            // Uses std::time::SystemTime to avoid adding a chrono dependency.
                            let name = {
                                use std::time::{SystemTime, UNIX_EPOCH};
                                let secs = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs();
                                // Decompose epoch seconds into date/time components.
                                let mins_total = secs / 60;
                                let hh = (mins_total / 60) % 24;
                                let mm = mins_total % 60;
                                // Days since epoch (Unix epoch = 1970-01-01).
                                let days = secs / 86400;
                                // Gregorian calendar decomposition.
                                // Algorithm: http://howardhinnant.github.io/date_algorithms.html
                                let z = days as i64 + 719468;
                                let era = if z >= 0 { z } else { z - 146096 } / 146097;
                                let doe = z - era * 146097;
                                let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
                                let y = yoe + era * 400;
                                let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                                let mp = (5 * doy + 2) / 153;
                                let d = doy - (153 * mp + 2) / 5 + 1;
                                let m = if mp < 10 { mp + 3 } else { mp - 9 };
                                let y = if m <= 2 { y + 1 } else { y };
                                format!("session-{y:04}{m:02}{d:02}-{hh:02}{mm:02}")
                            };
                            if let Err(e) = state.save_named_session(&name) {
                                log::error!("Leader+s: failed to save workspace '{name}': {e}");
                            }
                        }

                        KeyAction::JumpToAiPane => {
                            // Jump to the pane that most recently ran an AI agent.
                            // If pending_errors are queued, drain and inject them into the
                            // AI pane's PTY input as OSC 7770 error blocks.
                            if let Some(ai_id) = state.last_ai_pane
                                && state.panes.contains_key(&ai_id)
                            {
                                // Drain and inject any pending error contexts.
                                let errors =
                                    std::mem::take(&mut state.pending_errors);
                                if !errors.is_empty()
                                    && let Some(ai_terminal) =
                                        state.panes.get_mut(&ai_id)
                                {
                                    for err_ctx in &errors {
                                        let payload =
                                            context::format_error_osc7770(err_ctx);
                                        ai_terminal.write_input(&payload);
                                    }
                                    log::info!(
                                        "JumpToAiPane: injected {} error context(s) into pane {:?}",
                                        errors.len(),
                                        ai_id
                                    );
                                }
                                state.set_focused_pane(ai_id);
                                state.selection.clear();
                                state.window.request_redraw();
                            }
                        }

                        KeyAction::TogglePlanView => {
                            if state.plan_view.is_some() {
                                // Close the expanded overlay.
                                state.plan_view = None;
                            } else {
                                // Ensure the strip is populated before opening the view.
                                if state.plan_strip.is_none() {
                                    let root = state.workspace_root.clone();
                                    let strip = plan::PlanStripState::discover(&root);
                                    if !strip.summaries.is_empty() {
                                        state.plan_strip = Some(strip);
                                    }
                                }
                                // Open the expanded overlay with current summaries.
                                let summaries = state
                                    .plan_strip
                                    .as_ref()
                                    .map(|s| s.summaries.clone())
                                    .unwrap_or_default();
                                state.plan_view = Some(plan::PlanViewState::new(summaries));
                            }
                            state.window.request_redraw();
                        }

                        KeyAction::Consumed => {
                            // Key consumed by state machine (leader chord entered).
                            // No PTY write needed.
                        }

                        KeyAction::CrossPaneSearch => {
                            // Open the cross-pane search overlay.
                            state.search_overlay = Some(search::SearchOverlayState::new());
                            state.window.request_redraw();
                        }

                        KeyAction::ReviewOverlay => {
                            // Open the config overlay review if there are pending files.
                            let cfg = state.config.clone();
                            if let Some(review) = overlay::OverlayReviewState::new(&cfg) {
                                state.overlay_review = Some(review);
                                state.window.request_redraw();
                            }
                        }

                        // Menu-only actions: not triggered via keyboard chords.
                        // They will be handled when wired through dispatch_action() in Task 4.
                        KeyAction::Copy
                        | KeyAction::Paste
                        | KeyAction::SelectAll
                        | KeyAction::SearchNext
                        | KeyAction::SearchPrevious
                        | KeyAction::ClearScrollback
                        | KeyAction::IncreaseFontSize
                        | KeyAction::DecreaseFontSize
                        | KeyAction::ResetFontSize
                        | KeyAction::ToggleFullScreen
                        | KeyAction::Minimize
                        | KeyAction::EqualizeSplits
                        | KeyAction::NextTab
                        | KeyAction::PreviousTab
                        | KeyAction::ResetTerminal
                        | KeyAction::ShowDebugInfo
                        | KeyAction::OpenHelp
                        | KeyAction::ReportIssue => {}
                    }
                }
            }

            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: dispatch a KeyAction produced by the command palette.
//
// This covers the subset of KeyAction variants reachable from PaletteAction;
// Forward, SwitchTab, ResizePane, Consumed, and OpenPalette are never emitted
// by the palette and are handled as no-ops here.
// ---------------------------------------------------------------------------

fn execute_key_action(state: &mut AppState, event_loop: &ActiveEventLoop, action: KeyAction) {
    let focused_id = state.focused_pane();

    match action {
        KeyAction::NavigatePane(dir) => {
            let rects = state.compute_pane_rects();
            if let Some(new_focus) =
                state.active_layout().focus_in_direction(focused_id, dir, &rects)
            {
                state.set_focused_pane(new_focus);
                state.selection.clear();
            }
        }

        KeyAction::Split(axis) => {
            let rects = state.compute_pane_rects();
            let focused_rect = rects.get(&focused_id).copied().unwrap_or(PixelRect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
            });
            let new_rect = match axis {
                Axis::Horizontal => PixelRect { width: focused_rect.width / 2.0, ..focused_rect },
                Axis::Vertical => PixelRect { height: focused_rect.height / 2.0, ..focused_rect },
            };
            let new_size = state.grid_size_for_rect(new_rect);
            let new_id = state.spawn_pane(new_size);
            let orig_size = state.grid_size_for_rect(new_rect);
            let (cell_w, cell_h) = state.cell_dims();
            if let Some(terminal) = state.panes.get_mut(&focused_id) {
                terminal.resize(orig_size.1, orig_size.0, cell_w, cell_h);
            }
            let active = state.tab_manager.active;
            state.tab_layouts[active].split(focused_id, axis, new_id);
            state.set_focused_pane(new_id);
        }

        KeyAction::ClosePane => {
            let active = state.tab_manager.active;
            let pane_count = state.tab_layouts[active].all_pane_ids().len();
            if pane_count <= 1 {
                let removed_ids = state.tab_manager.close_tab(active);
                if active < state.tab_layouts.len() {
                    state.tab_layouts.remove(active);
                }
                for id in removed_ids {
                    let lid = id;
                    state.panes.remove(&lid);
                    // pty_channels removed (alacritty reader thread owns PTY)
                    state.image_channels.remove(&lid);
                }
                if state.tab_manager.tab_count() == 0 {
                    event_loop.exit();
                    return;
                }
                let new_focus = state.tab_manager.active_tab().focus;
                state.set_focused_pane(new_focus);
            } else {
                let replacement = state.tab_layouts[active].close(focused_id);
                if let Some(new_root) = replacement {
                    state.tab_layouts[active] = new_root;
                }
                state.panes.remove(&focused_id);
                // pty_channels removed (alacritty reader thread owns PTY)
                state.image_channels.remove(&focused_id);
                let remaining = state.tab_layouts[active].all_pane_ids();
                if let Some(&new_focus) = remaining.first() {
                    state.set_focused_pane(new_focus);
                }
            }
            state.selection.clear();
        }

        KeyAction::ToggleZoom => {
            let tab = state.tab_manager.active_tab_mut();
            if tab.zoomed == Some(focused_id) {
                tab.zoomed = None;
            } else {
                tab.zoomed = Some(focused_id);
            }
        }

        KeyAction::NewTab => {
            let win_size = state.window.inner_size();
            let full_size = state.renderer.grid_size_for_window(
                win_size.width,
                win_size.height,
                state.window.scale_factor(),
            );
            let new_id = state.spawn_pane(full_size);
            let tab_idx = state.tab_manager.add_tab(new_id);
            state.tab_layouts.push(PaneNode::Leaf { pane_id: new_id });
            state.tab_manager.switch_to(tab_idx);
            state.set_focused_pane(new_id);
            state.selection.clear();
        }

        KeyAction::CloseTab => {
            let active = state.tab_manager.active;
            let removed_ids = state.tab_manager.close_tab(active);
            if !removed_ids.is_empty() {
                if active < state.tab_layouts.len() {
                    state.tab_layouts.remove(active);
                }
                for id in removed_ids {
                    let lid = id;
                    state.panes.remove(&lid);
                    // pty_channels removed (alacritty reader thread owns PTY)
                    state.image_channels.remove(&lid);
                }
                let new_focus = state.tab_manager.active_tab().focus;
                state.set_focused_pane(new_focus);
                state.selection.clear();
            }
        }

        // These are not reachable from palette actions but must be exhaustive.
        KeyAction::Forward(_)
        | KeyAction::ResizePane(_)
        | KeyAction::SwitchTab(_)
        | KeyAction::OpenPalette
        | KeyAction::OpenWorkspaceSwitcher
        | KeyAction::SaveWorkspace
        | KeyAction::JumpToAiPane
        | KeyAction::TogglePlanView
        | KeyAction::ReviewOverlay
        | KeyAction::CrossPaneSearch
        | KeyAction::Copy
        | KeyAction::Paste
        | KeyAction::SelectAll
        | KeyAction::SearchNext
        | KeyAction::SearchPrevious
        | KeyAction::ClearScrollback
        | KeyAction::IncreaseFontSize
        | KeyAction::DecreaseFontSize
        | KeyAction::ResetFontSize
        | KeyAction::ToggleFullScreen
        | KeyAction::Minimize
        | KeyAction::EqualizeSplits
        | KeyAction::NextTab
        | KeyAction::PreviousTab
        | KeyAction::ResetTerminal
        | KeyAction::ShowDebugInfo
        | KeyAction::OpenHelp
        | KeyAction::ReportIssue
        | KeyAction::Consumed => {}
    }
}

// ---------------------------------------------------------------------------
// Helper: convert a physical pixel position to a grid CellPos relative to a
// pane's origin rect.
// ---------------------------------------------------------------------------

fn cursor_to_cell_in_rect(
    state: &AppState,
    px: f64,
    py: f64,
    pane_rect: PixelRect,
) -> selection::CellPos {
    let scale = state.window.scale_factor();
    let cell_w = state.renderer.text.cell_size.width as f64;
    let cell_h = state.renderer.text.cell_size.height as f64;
    // Convert pixel position relative to pane origin.
    let rel_x = px - pane_rect.x as f64;
    let rel_y = py - pane_rect.y as f64;
    pixel_to_cell(rel_x, rel_y, cell_w, cell_h, scale)
}

// ---------------------------------------------------------------------------
// Helper: collect PluginPaneRenderInfo for all PluginPane nodes in the layout.
// ---------------------------------------------------------------------------

/// Walk the pane tree and collect render infos for every `PluginPane` node
/// that has a non-zero rect.  The draw buffer for each plugin is taken
/// (replaced with empty) and translated to `PluginStyledLine` records.
fn collect_plugin_panes(
    node: &PaneNode,
    rects: &HashMap<PaneId, PixelRect>,
    mgr: &arcterm_plugin::manager::PluginManager,
    out: &mut Vec<PluginPaneRenderInfo>,
) {
    match node {
        PaneNode::PluginPane { pane_id, plugin_id } => {
            let Some(&rect) = rects.get(pane_id) else { return };
            if rect.width <= 0.0 || rect.height <= 0.0 {
                return;
            }
            let raw_lines = mgr.take_draw_buffer(plugin_id);
            let lines: Vec<PluginStyledLine> = raw_lines
                .into_iter()
                .map(|l| PluginStyledLine {
                    text: l.text,
                    fg: l.fg.map(|c| (c.r, c.g, c.b)),
                    bg: l.bg.map(|c| (c.r, c.g, c.b)),
                    bold: l.bold,
                    italic: l.italic,
                })
                .collect();
            out.push(PluginPaneRenderInfo {
                rect: [rect.x, rect.y, rect.width, rect.height],
                lines,
            });
        }
        PaneNode::Leaf { .. } => {}
        PaneNode::HSplit { left, right, .. } => {
            collect_plugin_panes(left, rects, mgr, out);
            collect_plugin_panes(right, rects, mgr, out);
        }
        PaneNode::VSplit { top, bottom, .. } => {
            collect_plugin_panes(top, rects, mgr, out);
            collect_plugin_panes(bottom, rects, mgr, out);
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: resolve a RenderPalette from the loaded configuration.
// ---------------------------------------------------------------------------

/// Build a [`RenderPalette`] from an [`ArctermConfig`].
fn palette_from_config(cfg: &config::ArctermConfig) -> RenderPalette {
    let app_palette = colors::ColorPalette::by_name(&cfg.color_scheme)
        .unwrap_or_else(|| {
            log::warn!(
                "config: unknown color_scheme {:?}, falling back to cool-night",
                cfg.color_scheme
            );
            colors::ColorPalette::default()
        })
        .with_overrides(&cfg.colors);

    RenderPalette {
        foreground: app_palette.foreground,
        background: app_palette.background,
        cursor:     app_palette.cursor,
        ansi:       app_palette.ansi,
    }
}
