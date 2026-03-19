# Data Model: WASM Plugin System

**Date**: 2026-03-19
**Feature**: 002-wasm-plugin-system

## Plugin

The core entity. Represents a loaded WASM plugin instance.

**Attributes**:
- `name: String` — user-specified name, unique within a config
- `path: PathBuf` — filesystem path to the `.wasm` file
- `capabilities: Vec<Capability>` — granted permissions
- `memory_limit_mb: u32` — maximum memory (default: 64)
- `fuel_per_callback: u64` — execution budget per callback (default: 1,000,000)
- `state: PluginState` — current lifecycle state
- `api_version: u32` — host API version the plugin targets

**State transitions**:
```
Loading → Initializing → Running → Stopping → Stopped
    ↓          ↓            ↓
  Failed     Failed       Failed
```

- `Loading`: WASM file is being read and validated
- `Initializing`: Plugin's `init()` export is being called
- `Running`: Plugin is active and receiving callbacks
- `Stopping`: Plugin's `destroy()` export is being called
- `Stopped`: Plugin has been cleanly shut down
- `Failed`: Error at any stage — includes error message

## Capability

A permission grant scoped to a specific resource.

**Attributes**:
- `resource: CapabilityResource` — what kind of resource (terminal, fs, net, keybinding)
- `operation: CapabilityOperation` — what operations are allowed (read, write, connect, register)
- `target: Option<String>` — scope constraint (filesystem path, host:port, or none)

**Validation rules**:
- `resource` must be one of: `terminal`, `fs`, `net`, `keybinding`
- `fs` capabilities MUST include a path target
- `net` capabilities MUST include a host:port target
- `terminal:read` is implicitly granted to all plugins
- Duplicate capabilities are deduplicated

**Parsed from config string format**: `"fs:read:/home/user/projects"`

## PluginConfig

User-facing configuration for a plugin, declared in `arcterm.lua`.

**Attributes**:
- `name: String` — plugin identifier
- `path: String` — path to WASM file (supports `~` and env vars)
- `capabilities: Vec<String>` — capability strings to parse
- `memory_limit_mb: Option<u32>` — override default memory limit
- `fuel_per_callback: Option<u64>` — override default fuel budget
- `enabled: Option<bool>` — disable without removing (default: true)

## Host API Interface

The contract between the terminal and plugins, defined in WIT.

**Terminal Read** (requires `terminal:read`):
- `get-visible-text() -> string`
- `get-lines(start: u32, end: u32) -> list<string>`
- `get-cursor-position() -> tuple<u32, u32>`
- `get-working-directory() -> string`
- `get-pane-dimensions() -> tuple<u32, u32>`
- `get-last-exit-code() -> option<s32>`

**Terminal Write** (requires `terminal:write`):
- `send-text(text: string) -> result<_, string>`
- `inject-output(text: string) -> result<_, string>`

**Filesystem** (requires `fs:read:<path>` or `fs:write:<path>`):
- `read-file(path: string) -> result<list<u8>, string>`
- `write-file(path: string, data: list<u8>) -> result<_, string>`

**Network** (requires `net:connect:<host>:<port>`):
- `http-get(url: string) -> result<http-response, string>`
- `http-post(url: string, body: list<u8>) -> result<http-response, string>`

**Keybinding** (requires `keybinding:register`):
- `register-key-binding(key: string, mods: string) -> result<u32, string>`

**Plugin Exports** (called by the host):
- `init() -> result<_, string>`
- `destroy()`
- `on-output(text: string)` — optional, called when terminal output changes
- `on-bell()` — optional, called when bell is received
- `on-focus(focused: bool)` — optional, called when pane focus changes
- `on-key-binding(binding-id: u32)` — optional, called when registered key is pressed

## Relationships

```
PluginConfig ──1:1──▶ Plugin (loaded at startup)
Plugin ──1:N──▶ Capability (parsed from config strings)
Plugin ──1:1──▶ Host API (accessed through wasmtime Store)
Host API ──N:1──▶ Pane (reads from / writes to active pane)
MuxNotification ──1:N──▶ Plugin (events routed to subscriber callbacks)
```
