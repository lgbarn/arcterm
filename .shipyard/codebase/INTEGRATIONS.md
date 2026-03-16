# INTEGRATIONS.md

## Overview
Arcterm has no outbound network connections to external APIs in the host application — there are no API keys, HTTP clients, or cloud service SDKs. Integrations are limited to: Neovim (local Unix socket IPC via msgpack-RPC), AI agent detection (local process inspection only, no network), a plugin system that exposes a `register-mcp-tool` WIT function (plugins can declare MCP tools, but the transport and broker are not yet visible in the host code), and WASI-sandboxed filesystem access for plugins. CI/CD publishes to GitHub Releases via `GITHUB_TOKEN`.

## Findings

### Neovim Integration (Local IPC)

- **Protocol**: msgpack-RPC over Unix domain socket
  - Evidence: `arcterm-app/src/neovim.rs` — `NvimRpcClient` uses `std::os::unix::net::UnixStream`
  - Evidence: `Cargo.toml` line 31 — `rmpv = "1"` (MessagePack value encoding/decoding)
- **Socket discovery**: reads the Neovim process's `--listen <path>` CLI argument via OS process introspection
  - Evidence: `arcterm-app/src/neovim.rs` — `discover_nvim_socket(pid)` scans `process_args`
- **Detection**: checks process name starts with `nvim` via `process_comm(pid)`
  - Evidence: `arcterm-app/src/neovim.rs` — `detect_neovim(pid)`
- **Capability used**: `has_nvim_neighbor` — queries Neovim window layout to coordinate split navigation
  - Evidence: `arcterm-app/src/neovim.rs` docstring — "queries Neovim's window layout to determine whether a Neovim split exists in a given direction"
- **Caching**: 2-second TTL on detection + socket results (`NeovimState`)
  - Evidence: `arcterm-app/src/neovim.rs` — "2-second cache so that every keypress does not trigger syscalls"
- **Scope**: local only, no network; only active when Neovim is detected in an adjacent pane

### AI Agent Detection (Local Process Inspection)

- **Mechanism**: reads process name (`process_comm`) and args (`process_args`) from the OS; no network calls, no API keys
  - Evidence: `arcterm-app/src/ai_detect.rs` lines 8-9
- **Detected agents**:
  - Claude Code (Anthropic) — binary name `claude`
  - Codex CLI (OpenAI) — binary name `codex`
  - Gemini CLI (Google) — binary name `gemini`
  - Aider — detected via Python interpreter args ending in `aider`
  - Cursor, Copilot — detected, classified as `Unknown(String)`
  - Evidence: `arcterm-app/src/ai_detect.rs` lines 35-46
- **Caching**: 5-second TTL per pane (`AI_CACHE_TTL`)
  - Evidence: `arcterm-app/src/ai_detect.rs` line 12
- **Use in context**: `arcterm-app/src/context.rs` serializes detection results as `"ai_type": "claude-code"` etc. into JSON context payloads
  - Evidence: `arcterm-app/src/context.rs` lines 227-229, 489

### Plugin System — WASI Sandbox

- **Sandbox**: WebAssembly Component Model running under wasmtime 42 with WASI p2
  - Evidence: `arcterm-plugin/src/runtime.rs` — `add_to_linker_sync(&mut linker)?`; `Config::wasm_component_model(true)`
- **Filesystem access**: controlled per-plugin via `plugin.toml` `[permissions] filesystem` allowlist
  - Evidence: `examples/plugins/hello-world/plugin.toml` — `filesystem = []` (no access)
  - Evidence: `examples/plugins/system-monitor/plugin.toml` — `filesystem = ["/proc", "/etc"]`
- **Network access**: controlled per-plugin via `[permissions] network = false/true`
  - Evidence: both example `plugin.toml` files show `network = false`; the field exists for future use
- **AI access flag**: `[permissions] ai = true/false` — present in manifest schema
  - Evidence: `examples/plugins/system-monitor/plugin.toml` — `ai = true`
  - [Inferred] This flag gates some host-side AI feature; exact enforcement not confirmed without reading `arcterm-plugin/src/manifest.rs` in full
- **Host functions exposed to plugins** (via WIT imports):
  - `log(msg)` — logging to host
  - `render-text(line)` — push styled output lines
  - `subscribe-event(kind)` — subscribe to terminal lifecycle events
  - `get-config(key)` — read host config values
  - `register-mcp-tool(schema)` — declare an MCP tool
  - Evidence: `arcterm-plugin/wit/arcterm.wit` lines 65-70

### Plugin MCP Tool Registration

- **Interface**: plugins call `register-mcp-tool` with a `tool-schema` (name, description, JSON input schema string)
  - Evidence: `arcterm-plugin/wit/arcterm.wit` — `record tool-schema { name: string, description: string, input-schema: string }`
- **Transport/broker**: [Inferred] the host receives tool schemas and presumably exposes them via an MCP server interface, but no HTTP server or MCP transport code was located in the source files examined. This likely lives in `arcterm-plugin/src/host.rs` or `arcterm-plugin/src/manager.rs` and was not fully read.
- **Current usage**: `arcterm-app/src/main.rs` imports `arcterm-plugin` — plugin manager is wired into the event loop
  - Evidence: `arcterm-app/Cargo.toml` line 39 — `arcterm-plugin = { path = "../arcterm-plugin" }`

### Clipboard

- **Library**: arboard 3 — cross-platform clipboard read/write
  - Evidence: `arcterm-app/Cargo.toml` line 28 — `arboard = "3"`
- **Scope**: local OS clipboard; no cloud sync or remote clipboard service

### Filesystem (Configuration and Platform Paths)

- **User config**: `~/.config/arcterm/config.toml` (resolved via `dirs` crate for XDG/macOS/Windows portability)
  - Evidence: `arcterm-app/src/config.rs` lines 7-8; `arcterm-app/Cargo.toml` line 31 — `dirs = "6"`
- **Plugin config/data directories**: also resolved via `dirs`
  - Evidence: `arcterm-plugin/Cargo.toml` line 16 — `dirs = "6"`
- **Config hot-reload**: `notify 8` watcher on the config file path; no cloud/remote config source
  - Evidence: `arcterm-app/Cargo.toml` line 32; `arcterm-app/src/config.rs` — `watch_config`

### CI/CD — GitHub

- **GitHub Releases**: cargo-dist creates releases and uploads build artifacts on SemVer tag push using `GITHUB_TOKEN`
  - Evidence: `.github/workflows/release.yml` lines 57, 279 — `gh release create`
- **GitHub Actions**: standard CI platform; no other CI service (no CircleCI, Travis, etc.)
  - Evidence: `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- **No external secret stores**: only `secrets.GITHUB_TOKEN` (built-in) is referenced
  - Evidence: `.github/workflows/release.yml` lines 57, 113, 175, 225

### Not Present / Explicitly Absent

| Integration | Status |
|---|---|
| HTTP client (reqwest, hyper, ureq) | Not found in any Cargo.toml |
| External AI API (Anthropic, OpenAI, etc.) | Not found — AI detection is process-local only |
| Database (SQLite, Postgres, etc.) | Not found |
| Auth provider (OAuth, OIDC) | Not found |
| Email/notification service | Not found |
| Message queue (Kafka, NATS, etc.) | Not found |
| Telemetry/observability (OpenTelemetry, Sentry) | Not found |
| Docker / container runtime | Not found |

## Summary Table

| Integration | Type | Protocol | Scope | Confidence |
|---|---|---|---|---|
| Neovim | Local IPC | msgpack-RPC over Unix socket | Local host only | Observed |
| AI agent detection | OS process inspection | syscalls (`/proc` or macOS API) | Local host only | Observed |
| Plugin WASI filesystem | Sandboxed file I/O | WASI p2 | Allowlisted paths per plugin | Observed |
| Plugin MCP tool registration | In-process registration | WIT function call | Local plugin↔host | Observed |
| Plugin network (flag) | Permission flag | TBD | Per-plugin opt-in | Observed (flag only) |
| Clipboard | OS clipboard API | arboard | Local host only | Observed |
| Config file watching | inotify/kqueue/FSEvents | notify crate | Local filesystem | Observed |
| GitHub Releases | CI/CD | GitHub API via `gh` CLI | Remote (CI only) | Observed |

## Open Questions

- `arcterm-plugin/src/host.rs` was not fully read — the MCP tool broker/transport implementation may be there. Confirm whether `register-mcp-tool` results in an actual MCP server socket being opened, or whether it is queued for a future phase.
- `[permissions] network = true` is allowed by the plugin manifest schema (`examples/plugins/system-monitor/plugin.toml`). Confirm whether wasmtime's WASI p2 network socket capability is actually granted when this flag is set, or whether it is reserved/unimplemented.
- `[permissions] ai = true` is present in the system-monitor plugin. Confirm what host-side resource or API this unlocks.
