// System monitor example plugin for Arcterm.
//
// Demonstrates the full plugin API surface:
//   - load()              — subscribes to CommandExecuted events; registers an
//                           MCP tool "get-system-info".
//   - render()            — displays a multi-line styled dashboard.
//   - update(event)       — increments a counter on CommandExecuted; returns
//                           true to trigger re-render.
//
// System information is obtained via host config keys and WASI filesystem reads
// of /proc/loadavg and /proc/uptime (Linux) or best-effort alternatives.
//
// To build:
//   cargo component build --release
//   # or:
//   cargo build --target wasm32-wasip2 --release
//   wasm-tools component new target/wasm32-wasip2/release/system_monitor_plugin.wasm \
//       -o system_monitor_plugin.wasm

wit_bindgen::generate!({
    path: "../../../arcterm-plugin/wit/arcterm.wit",
    world: "arcterm-plugin",
});

use std::cell::RefCell;

// ---------------------------------------------------------------------------
// Plugin state
// ---------------------------------------------------------------------------

struct State {
    /// Number of CommandExecuted events received since load.
    command_count: u32,
    /// Hostname read from config or /etc/hostname at load time.
    hostname: String,
    /// Current working directory from config.
    cwd: String,
    /// A simple frame counter used as a stand-in for uptime when WASI clock
    /// access is unavailable.
    frame: u32,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        command_count: 0,
        hostname: String::new(),
        cwd: String::new(),
        frame: 0,
    });
}

// ---------------------------------------------------------------------------
// Guest export implementations
// ---------------------------------------------------------------------------

struct SystemMonitorPlugin;

impl Guest for SystemMonitorPlugin {
    fn load() {
        host::log("system-monitor plugin: loaded");

        // Subscribe to CommandExecuted so we can update the counter.
        host::subscribe_event(EventKind::CommandExecuted);

        // Register an MCP tool that callers can invoke via the AI layer.
        host::register_mcp_tool(ToolSchema {
            name: "get-system-info".to_string(),
            description: "Returns current system information from the running arcterm session."
                .to_string(),
            // JSON Schema for the tool's input parameters (none required).
            input_schema: r#"{"type":"object","properties":{},"additionalProperties":false}"#
                .to_string(),
        });

        // Read hostname from config, falling back to a WASI filesystem read.
        let hostname = host::get_config("hostname".to_string())
            .or_else(|| read_hostname_from_fs())
            .unwrap_or_else(|| "unknown".to_string());

        let cwd = host::get_config("cwd".to_string())
            .unwrap_or_else(|| "/".to_string());

        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.hostname = hostname;
            st.cwd = cwd;
        });
    }

    fn render() -> Vec<StyledLine> {
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.frame = st.frame.wrapping_add(1);

            let hostname = st.hostname.clone();
            let cwd = st.cwd.clone();
            let cmd_count = st.command_count;
            let frame = st.frame;

            // Line 1: title bar — bold cyan.
            host::render_text(StyledLine {
                text: " System Monitor ".to_string(),
                fg: Some(Color { r: 80, g: 220, b: 220 }),
                bg: Some(Color { r: 20, g: 30, b: 40 }),
                bold: true,
                italic: false,
            });

            // Line 2: separator.
            host::render_text(StyledLine {
                text: "─".repeat(40),
                fg: Some(Color { r: 80, g: 80, b: 100 }),
                bg: None,
                bold: false,
                italic: false,
            });

            // Line 3: hostname.
            host::render_text(StyledLine {
                text: format!("  Host:     {}", hostname),
                fg: Some(Color { r: 200, g: 200, b: 200 }),
                bg: None,
                bold: false,
                italic: false,
            });

            // Line 4: CWD.
            host::render_text(StyledLine {
                text: format!("  CWD:      {}", cwd),
                fg: Some(Color { r: 180, g: 180, b: 220 }),
                bg: None,
                bold: false,
                italic: false,
            });

            // Line 5: uptime (approximate via frame counter; real uptime would
            // use wasi:clocks/wall-clock when available).
            let uptime_s = frame * 33 / 1000; // rough 30fps estimate
            host::render_text(StyledLine {
                text: format!("  Uptime:   ~{}s (frames: {})", uptime_s, frame),
                fg: Some(Color { r: 160, g: 200, b: 160 }),
                bg: None,
                bold: false,
                italic: false,
            });

            // Line 6: command count.
            host::render_text(StyledLine {
                text: format!("  Commands: {}", cmd_count),
                fg: Some(Color { r: 220, g: 180, b: 80 }),
                bg: None,
                bold: false,
                italic: false,
            });

            // Line 7: MCP tools registered.
            host::render_text(StyledLine {
                text: "  Tools:    1 (get-system-info)".to_string(),
                fg: Some(Color { r: 160, g: 140, b: 220 }),
                bg: None,
                bold: false,
                italic: false,
            });

            // Line 8: separator.
            host::render_text(StyledLine {
                text: "─".repeat(40),
                fg: Some(Color { r: 80, g: 80, b: 100 }),
                bg: None,
                bold: false,
                italic: false,
            });

            // Line 9: load average (read from /proc/loadavg if available).
            let load_str = read_load_average()
                .unwrap_or_else(|| "unavailable".to_string());
            host::render_text(StyledLine {
                text: format!("  Load avg: {}", load_str),
                fg: Some(Color { r: 200, g: 160, b: 120 }),
                bg: None,
                bold: false,
                italic: false,
            });
        });

        // Lines are delivered via host::render_text; return empty vec.
        Vec::new()
    }

    fn update(event: PluginEvent) -> bool {
        match event {
            PluginEvent::CommandExecuted(cmd) => {
                host::log(&format!("system-monitor: command executed: {}", cmd));
                STATE.with(|s| {
                    s.borrow_mut().command_count += 1;
                });
                true // request re-render
            }
            PluginEvent::PaneOpened(pane_id) => {
                host::log(&format!("system-monitor: pane opened: {}", pane_id));
                false // no re-render needed
            }
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// WASI filesystem helpers
// ---------------------------------------------------------------------------

/// Read hostname from /etc/hostname (Linux) or return None.
///
/// On WASM targets with WASI filesystem access, `std::fs::read_to_string`
/// works against the host filesystem via the WASI sandbox.  This requires
/// the plugin manifest to declare `filesystem = ["/etc"]`.
fn read_hostname_from_fs() -> Option<String> {
    // Try /etc/hostname (Linux / macOS).
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Read load average from /proc/loadavg (Linux only).
///
/// Returns the first three values (1m, 5m, 15m) as a formatted string, or
/// None when /proc is not available (e.g. macOS or restricted sandbox).
fn read_load_average() -> Option<String> {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| {
            let mut parts = s.split_whitespace();
            let m1 = parts.next()?;
            let m5 = parts.next()?;
            let m15 = parts.next()?;
            Some(format!("{} {} {} (1m 5m 15m)", m1, m5, m15))
        })
}

export!(SystemMonitorPlugin);
