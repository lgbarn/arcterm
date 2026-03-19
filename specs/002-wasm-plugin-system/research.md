# Research: WASM Plugin System

**Date**: 2026-03-19
**Feature**: 002-wasm-plugin-system

## Decision 1: WASM Runtime

**Decision**: Use wasmtime LTS (v36.x series) with Component Model support.

**Rationale**: Wasmtime is the most mature WASM runtime for Rust. The LTS
branch provides 24-month stability, avoiding the monthly breaking API changes
of the latest releases. The Component Model is production-ready and eliminates
hand-written type glue via the `bindgen!` macro and WIT IDL.

**Alternatives considered**:
- Wasmer — less mature Component Model, smaller ecosystem
- wasm3 — interpreter-only, no Component Model
- wasmtime latest (v42.x) — monthly breaking changes, maintenance burden

**Open risks**: Need to verify wasmtime v36.x LTS compiles with Rust 1.71.0
minimum. May need to bump minimum Rust version.

## Decision 2: Crate Structure

**Decision**: Create `arcterm-wasm-plugin` as a new workspace member crate.
It depends on `config` and `mux` crates. It does NOT live under
`lua-api-crates/`. Registration follows the existing pattern: call
`add_context_setup_func()` from `env-bootstrap` for Lua-facing API, and
subscribe to `MuxNotification` from `wezterm-gui/src/main.rs` for events.

**Rationale**: Following the existing crate patterns minimizes architectural
disruption. Keeping it under the `arcterm-` prefix per constitution keeps
ArcTerm-specific code clearly separated for upstream merges.

**Alternatives considered**:
- Standalone crate with no mux dependency — too limited, can't read terminal state
- Embed in existing `config` crate — violates separation of concerns

## Decision 3: Host API Design

**Decision**: Define the host API using WIT (WASM Interface Types). The API
surface mirrors the existing Lua pane API from `lua-api-crates/mux/src/pane.rs`:

**Read capabilities** (granted by `terminal:read`):
- `get-visible-text() -> string`
- `get-lines(start, end) -> list<string>`
- `get-cursor-position() -> (row, col)`
- `get-working-directory() -> string`
- `get-pane-dimensions() -> (rows, cols)`
- `get-last-exit-code() -> option<i32>`

**Write capabilities** (granted by `terminal:write`):
- `send-text(text: string)`
- `inject-output(text: string)`

**Event subscriptions**:
- `on-output(callback)` — terminal output changed
- `on-bell(callback)` — bell received
- `on-pane-focus(callback)` — pane gained/lost focus

**Keybinding registration** (granted by `keybinding:register`):
- `register-key-binding(key, mods, callback)`

**Rationale**: The Lua API already proves these are the right abstractions —
25+ methods exposed, battle-tested by WezTerm's user community. Starting with
the core subset keeps the initial scope manageable.

**Alternatives considered**:
- Custom binary protocol — fragile, no tooling
- Direct FFI — defeats sandbox isolation

## Decision 4: Capability Model

**Decision**: Capabilities follow the pattern `<resource>:<operation>:<target>`:
- `terminal:read` — read terminal state (default, always granted)
- `terminal:write` — write to terminal input/output
- `fs:read:<path>` — read files under the specified path
- `fs:write:<path>` — write files under the specified path
- `net:connect:<host>:<port>` — make outbound network connections
- `keybinding:register` — register custom key bindings

Capabilities are declared per-plugin in `arcterm.lua` config:
```lua
wezterm.plugin.register({
  name = "my-plugin",
  path = wezterm.home_dir .. "/.config/arcterm/plugins/my-plugin.wasm",
  capabilities = { "terminal:read", "fs:read:." },
  memory_limit_mb = 64,
  fuel_per_callback = 1000000,
})
```

**Rationale**: This is the simplest model that satisfies the security
requirements. Deny-by-default with explicit grants. The config syntax
integrates naturally with the existing Lua config system.

## Decision 5: Execution Model

**Decision**: Use wasmtime's fuel-based metering for CPU limits and store-level
memory limits. Plugin callbacks run on the existing smol async runtime. Each
plugin gets its own `wasmtime::Store` (isolated memory). Plugins are loaded at
startup and persist for the terminal lifetime.

**Rationale**: Fuel metering is deterministic and doesn't require OS-level
timers. Store-per-plugin provides memory isolation. Smol is already the async
runtime in use.

**Open risks**: Smol + wasmtime async is untested in this combination. A small
prototype spike should verify this works before full implementation.

## Decision 6: Plugin Configuration Integration

**Decision**: Plugins are configured via the existing Lua config system using
`wezterm.plugin.register()`. This is a new Lua API function registered from
`arcterm-wasm-plugin`. The `config` crate stores plugin declarations as a new
`Vec<WasmPluginConfig>` field.

**Rationale**: Using the Lua config system means users don't need to learn
a second config format. The `wezterm.plugin.register()` call pattern is
familiar from the existing Lua API style.

## Decision 7: MuxNotification Event Routing

**Decision**: Subscribe to `MuxNotification` events from the GUI event loop.
Filter for `PaneOutput` (debounced) and route to plugins that have registered
`on-output` callbacks. Other events (`Alert::Bell`, focus changes) route
similarly.

**Rationale**: `MuxNotification` already provides 13 event types covering
all plugin-relevant terminal events. This avoids creating a parallel event
system.

**Performance note**: `PaneOutput` is high-frequency. Must debounce before
dispatching to WASM (batch multiple outputs into a single callback).
