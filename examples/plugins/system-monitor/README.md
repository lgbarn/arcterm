# system-monitor — Arcterm Plugin Example

A full-featured example plugin that demonstrates the complete Arcterm plugin API:

| API surface | Usage |
|---|---|
| `host::subscribe_event(CommandExecuted)` | Called from `load()` |
| `host::register_mcp_tool(...)` | Registers `get-system-info` tool |
| `host::get_config(key)` | Reads hostname / cwd config |
| `host::render_text(StyledLine { ... })` | Multi-line styled dashboard |
| `update(CommandExecuted)` | Increments command counter |
| `update(PaneOpened)` | Logs pane open events |
| WASI `/etc/hostname` | Reads hostname via filesystem |
| WASI `/proc/loadavg` | Reads load average (Linux only) |

## Dashboard layout

```
 System Monitor
────────────────────────────────────────
  Host:     myhost
  CWD:      /home/user
  Uptime:   ~42s (frames: 1273)
  Commands: 7
  Tools:    1 (get-system-info)
────────────────────────────────────────
  Load avg: 0.52 0.61 0.70 (1m 5m 15m)
```

## MCP tool: `get-system-info`

When the AI layer calls the `get-system-info` tool (after Phase 7 MCP
JSON-RPC serving is wired up), it receives current system information from
the running arcterm session.  The tool has no required inputs.

## Building

```sh
# Install the wasm32-wasip2 target:
rustup target add wasm32-wasip2

# Build with cargo-component (recommended):
cargo install cargo-component
cargo component build --release

# Or with wasm-tools post-processing:
cargo build --target wasm32-wasip2 --release
wasm-tools component new \
    target/wasm32-wasip2/release/system_monitor_plugin.wasm \
    -o system_monitor_plugin.wasm
```

## Load in arcterm

```sh
arcterm-app plugin dev /path/to/examples/plugins/system-monitor
```

## Permissions

The `plugin.toml` declares:

```toml
[permissions]
filesystem = ["/proc", "/etc"]
panes      = "read"
ai         = true
```

- `filesystem = ["/proc", "/etc"]` allows WASI reads for hostname and load avg.
- `panes = "read"` allows rendering output to the pane.
- `ai = true` allows `register_mcp_tool` calls.
