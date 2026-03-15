# Research: Phase 6 — WASM Plugin System

## Context

Arcterm is a GPU-rendered terminal emulator built on wgpu + winit + tokio (Rust 2024 edition,
stable toolchain). After five completed phases it has: a pane tree layout engine
(`PaneNode` enum in `layout.rs`), per-pane `Terminal` structs wiring PTY/VT/Grid, a
`TextRenderer` (glyphon) that accepts arbitrary styled text for multi-pane rendering, a
workspace TOML system, a CLI via clap 4, and an async `tokio::sync::mpsc` channel per pane
for PTY byte delivery.

Phase 6 adds a WASM plugin runtime. Decisions already locked in `CONTEXT-6.md`:

- Interface definition: wit-bindgen (Component Model)
- Plugin UI: text-based draw commands rendered via the existing `TextRenderer`
- Plugin storage: `~/.config/arcterm/plugins/`
- Permission model: capability-based sandbox declared in `plugin.toml` manifest
- Event bus: pub/sub with typed events (PaneOpened, PaneClosed, CommandExecuted, WorkspaceSwitched)
- MCP tool registration: included in Phase 6 scope

The research below addresses six open technical questions:

1. Which wasmtime API surface to use (core modules vs. Component Model)
2. How wit-bindgen host bindings work and how to wire them to a Linker
3. How Zellij implemented its WASM plugin system (architecture reference)
4. WASI preview 2 support for the filesystem/network capability sandbox
5. What async channel primitive to use for the plugin event bus
6. How MCP tool registration integrates with the plugin API

---

## Comparison Matrix

### Topic 1: WASM Runtime — Core Modules vs Component Model vs Extism

| Criteria | wasmtime (core module API) | wasmtime (Component Model) | Extism |
|----------|---------------------------|---------------------------|--------|
| Latest version (Mar 2026) | 42.0.1 (Feb 25, 2026) | 42.0.1 (same crate) | 1.13.0 (Nov 25, 2025) |
| Monthly downloads | ~1.35M | same crate | ~14k |
| GitHub stars | #1 WASM crate on lib.rs | same repo | not quantified |
| License | Apache-2.0 WITH LLVM exception | same | BSD-3-Clause |
| Interface definition | Manual: host fn registration via `Linker::func_new` | WIT files + `bindgen!` macro → type-safe Rust traits | Extism PDK ABI (custom, not WIT) |
| Type safety at boundary | None — raw `Val` arrays | Full — generated types from WIT | Medium — generic byte-in/byte-out via `extism-convert` |
| Multi-language guest support | Any wasm32 target | Any lang with Component Model support | 10+ PDKs (Rust, Go, Python, JS…) |
| WASI p2 support | Via `wasmtime-wasi` p2 module | Via `wasmtime-wasi` p2 module | Depends on internal wasmtime (pins >=27.0.0) |
| Release cadence | Monthly (20th of each month) | Same | Irregular (last Nov 2025) |
| Breaking change policy | Major version per month; patch = no breaks | Same | Undefined public policy |
| wasmtime version pinning | Direct — you choose | Direct — you choose | Extism pins its own wasmtime range |
| CONTEXT-6.md alignment | Low — no WIT, manual ABI | High — exactly the stated decision | Low — contradicts wit-bindgen decision |
| Stack compatibility | Rust 2024, tokio async | Rust 2024, tokio async, `async` feature | Rust, but opaque dependency on wasmtime internals |

### Topic 2: Plugin Host API Interface Layer

| Criteria | wit-bindgen guest + wasmtime::bindgen! host | Manual protobuf/msgpack ABI | Raw function table (C-style) |
|----------|---------------------------------------------|----------------------------|------------------------------|
| Maintenance cost | Generated code — WIT is the source of truth | High — maintain schema + codec in two layers | Very high — no tooling |
| Language support for guests | Any lang with wit-bindgen support (Rust, Go, Python, JS) | Any lang with protobuf | Only langs targeting stable C ABI |
| API evolution | WIT versioning support planned | Requires schema migration | No versioning story |
| Type safety on host | Full — generated Rust traits | Partial — decoded into structs | None |
| Compile time (host) | Medium — proc macro `bindgen!` | Medium — codegen step | Low |
| CONTEXT-6.md alignment | Exact match | Contradicts stated decision | Contradicts stated decision |

### Topic 3: WASI Preview 2 Capability Model

| Criteria | wasmtime-wasi p1 (legacy) | wasmtime-wasi p2 (current) | No WASI (custom host-only API) |
|----------|--------------------------|---------------------------|-------------------------------|
| Version (Mar 2026) | 42.0.1 | 42.0.1 | n/a |
| Monthly downloads | 522k | same crate | n/a |
| Stability | Stable | Stable since WASI 0.2.0 (Jan 2024) | n/a |
| Filesystem sandboxing | preopened dirs via cap-std | preopened dirs via cap-std | Must implement manually |
| Network sandboxing | Basic sockets | socket preopening via `preopened_socket` | Must implement manually |
| Component Model compatible | No — core module only | Yes — `add_to_linker_async` for component linker | n/a |
| Integration complexity | `add_to_linker_sync` | `add_to_linker_async` + `WasiView` trait on Store T | Low — but re-invents the wheel |
| CONTEXT-6.md alignment | Low — not Component Model | High | Low — more work, less standard |

### Topic 4: Plugin Event Bus Channel Primitive

| Criteria | `tokio::sync::broadcast` | `tokio::sync::mpsc` | `async-channel` (unbounded) |
|----------|--------------------------|---------------------|-----------------------------|
| Already in project | Yes (tokio full) | Yes (tokio full) | No |
| Semantics | 1 sender, N receivers (fan-out) | N senders, 1 receiver | N senders, N receivers |
| Fan-out (one event → all plugins) | Native | Requires per-plugin channel | With clone of sender side |
| Slow receiver handling | Lagged: slow receivers drop messages | N/A (one receiver) | Backpressure blocks sender |
| Max capacity | Configurable ring buffer | Configurable | Unbounded (memory risk) |
| Stack fit | Identical tokio dependency | Identical | New dependency |
| Event bus use case fit | High — exactly pub/sub fan-out | Low for fan-out | High but adds dependency |

### Topic 5: MCP Tool Registration Integration

| Criteria | In-process JSON-RPC over stdio (mock MCP server) | Full MCP server per plugin | Arcterm-internal tool registry (struct-based) |
|----------|--------------------------------------------------|--------------------------|----------------------------------------------|
| Spec compliance (2025-03-26) | Partial — implements tools/list and tools/call | Full | Not spec-compliant |
| Implementation complexity | Low — serde_json already present | High — full jsonrpc server per plugin | Very low |
| Phase 7 upgrade path | Easy — extend to full MCP in Phase 7 | Already complete | Requires rewrite |
| Plugin authoring burden | Low — plugins declare tools as WIT records | Low | Low |
| CONTEXT-6.md alignment | High — "MCP basics in Phase 6" | Overkill | Low — no Phase 7 path |

---

## Detailed Analysis

### Topic 1: WASM Runtime

#### Option A: wasmtime Component Model (Recommended)

**Strengths:**

wasmtime v42.0.1 (Feb 25, 2026) is the production standard for WASM embedding in Rust. The
Component Model API is housed in the same crate under `wasmtime::component` — no additional
dependency is required beyond enabling the `component-model` feature flag. The `Engine`,
`Store<T>`, `component::Component`, `component::Linker`, and `component::Instance` types
mirror the core API structurally, so developers already familiar with core module embedding
will find the Component Model API intuitive.

The release cadence (monthly major releases, strict patch compatibility) is predictable.
wasmtime has shipped 175+ stable releases as of March 2026. Breaking changes to the embedding
API must meet published criteria and go through an RFC process for major changes; minor
breaking changes are documented in release notes. The practical implication for arcterm: a
monthly dependency update will occasionally require API adjustments, but the changes are
well-documented and the `CHANGELOG.md` is maintained.

The `bindgen!` macro generates Rust traits that the host implements — a pattern identical to
serde's `#[derive(Deserialize)]` in structure. This is consistent with the codebase's
existing convention.

Memory and resource limits are well-specified: `StoreLimits` bounds memory page counts,
`consume_fuel()` enables deterministic metering of plugin execution time, and
`epoch_interruption()` provides lightweight cooperative yielding. For the Phase 6 goal of
"under 10MB overhead per plugin," `StoreLimits::max_memory_size` is the direct control.

**Weaknesses:**

wasmtime's major version bumps monthly, which means `Cargo.lock` updates are frequent and
occasionally require API migration. The Component Model API has had 51 breaking releases of
`wit-bindgen` (tracked separately), indicating that the WIT toolchain has been unstable
historically. As of early 2026, WASI 0.2.0 is stable, but the WIT ecosystem is still young.

The async Component Model integration requires implementing the `WasiView` trait on the
host's `Store<T>` data type — a small but non-trivial setup step that ties the host state
struct to the WASI context.

**Integration notes:**

The existing `AppState` struct in `main.rs` will need a sibling `PluginManager` or
`PluginRuntime` struct. The `Engine` (one per process) can be stored as a field; `Store<T>`
instances are per-plugin (cheap to create/destroy). The plugin store's `T` will be a struct
containing the `WasiCtx` (for WASI p2) and any arcterm-specific host state (e.g., a channel
sender for the event bus). The `wasmtime::component::Linker` is constructed once and reused
across all plugin instantiations in the same `Engine`.

New workspace dependency required:

```toml
wasmtime = { version = "42", features = ["component-model", "async", "cranelift"] }
wasmtime-wasi = { version = "42", features = ["p2"] }
```

#### Option B: wasmtime Core Module API

**Strengths:** Simpler API surface; no WIT toolchain dependency. Adequate if the plugin ABI
is defined as a fixed set of exported/imported functions with primitive types.

**Weaknesses:** Type safety at the plugin boundary is entirely manual — host functions
receive `&[Val]` arrays and must pattern-match on `Val::I32`, `Val::ExternRef`, etc. Strings
and structs must be serialized to/from linear memory manually (writing to pointer+length
pairs). This is the approach Zellij used before migrating to WIT — see the Zellij analysis
below for why they found this painful. Contradicts the CONTEXT-6.md decision. Rejected.

#### Option C: Extism

**Strengths:** High-level, opinionated plugin API; 14 language SDKs; simpler host code than
raw wasmtime.

**Weaknesses:** Uses a custom ABI (byte-in/byte-out via `extism-convert`), not the Component
Model or WIT. This directly contradicts the CONTEXT-6.md decision to use wit-bindgen. Extism
pins its own internal wasmtime version range (>=27.0.0, <31.0.0 based on v1.13.0), which
would conflict with arcterm controlling its own wasmtime version. Downloads are ~14k/month —
a small community relative to the 1.35M/month for wasmtime directly. Last release was
November 2025, three months before this research was written, suggesting lower maintenance
velocity. Rejected on ABI incompatibility and loss of version control.

---

### Topic 2: WIT Interface and wit-bindgen

#### Host binding generation: wasmtime::component::bindgen!

The `wasmtime` crate provides `wasmtime::component::bindgen!()` — the host-side counterpart
to wit-bindgen's guest `generate!` macro. The macro accepts a WIT package (by path, inline
string, or directory) and generates:

- Rust traits the host must implement (one trait per WIT interface section)
- Type definitions matching WIT records, variants, enums, and resources
- A top-level struct with an `instantiate()` method that wires the component to a `Linker`

The generated code pattern:

```
// WIT definition (arcterm-plugin/wit/arcterm.wit):
package arcterm:plugin@0.1.0;

world plugin {
  import host: interface {
    render-text: func(lines: list<styled-line>);
    subscribe-event: func(event-type: event-kind);
    get-config: func(key: string) -> option<string>;
    register-mcp-tool: func(schema: tool-schema);
  }
  export plugin-main: interface {
    load:   func(config: list<tuple<string, string>>);
    update: func(event: plugin-event) -> bool;
    render: func(rows: u32, cols: u32);
  }
}
```

The host implements the generated `Host` trait on a struct that holds the arcterm state the
plugin needs to reach (e.g., an `mpsc::Sender` for draw commands, a reference to the config
system). The `Linker` is populated by calling the generated `Plugin::add_to_linker()` method
before instantiation.

The `wit-bindgen` guest crate (v0.53.1, Feb 2026, 13.6M monthly downloads) is used by the
plugin author — arcterm does not depend on it directly. The host side is entirely within
`wasmtime::component::bindgen!`.

**WIT versioning:** The WIT file ships as part of arcterm's repository. Plugins must compile
against a specific WIT version. CONTEXT-6.md acknowledges this by mandating a `v0` API with
explicit instability guarantees. The WIT file should be versioned in the package declaration
(`@0.1.0`) and plugins should declare compatibility via their `plugin.toml` manifest field
(e.g., `api-version = "0.1"`).

---

### Topic 3: Zellij Plugin Architecture Reference

Zellij is the most mature Rust terminal multiplexer with a production WASM plugin system.
Their architecture is directly relevant as a reference.

**ZellijPlugin trait (from zellij-tile):**

```rust
pub trait ZellijPlugin: Default {
    fn load(&mut self, configuration: BTreeMap<String, String>) {}
    fn update(&mut self, event: Event) -> bool { false }  // return true to trigger re-render
    fn pipe(&mut self, pipe_message: PipeMessage) -> bool { false }
    fn render(&mut self, rows: usize, cols: usize) {}
}
```

Key observations:
- `render()` receives terminal dimensions (rows, cols) and outputs to stdout/stderr using
  `print!` macros — the WASM module's stdout is captured by the host and injected into the
  pane as raw terminal output.
- `update()` returns `bool` to signal whether re-rendering is needed — this avoids
  unnecessary redraws.
- `load()` receives key-value config from the plugin manifest, mirroring CONTEXT-6.md's
  `plugin.toml` config access requirement.
- `pipe()` handles inter-plugin messaging — relevant for Phase 6's event bus design.

**Rendering approach:** Zellij plugins write ANSI escape sequences via `print!`. The host
reads the WASM module's stdout (captured via WASI stdio) and feeds it through the VT parser
into the pane's grid. This is a clean approach: plugins get the full ANSI color/style
palette without any custom draw protocol, and the existing VT parser handles everything.

However, CONTEXT-6.md chose "text-based draw commands" (styled text lines) instead of raw
ANSI passthrough. The difference matters for arcterm: the existing `TextRenderer` and
`StructuredBlock` system expects structured data, not raw ANSI sequences. Arcterm's approach
(plugins emit typed draw commands over the WIT boundary) is cleaner and avoids running the
VT parser on plugin output.

**Zellij permission types** (from zellij-tile source): ReadApplicationState, ChangeApplicationState,
OpenFiles, RunCommands, OpenTerminalsOrPlugins, WriteToStdin, WebAccess, ReadSecret,
RunPlugins. These map closely to CONTEXT-6.md's `filesystem`, `network`, `panes`, and `ai`
capabilities. Zellij declares permissions in its KDL layout files rather than a separate
plugin manifest — arcterm's TOML manifest approach is more portable and self-contained.

**What Zellij does well:** The worker system (background tasks via `ZellijWorker` + message
passing) addresses the problem of plugins blocking the render loop — directly relevant to
Phase 6's event bus design where slow plugins must not block terminal I/O.

**What arcterm should do differently:**
1. Use WIT draw commands (typed records) rather than raw ANSI stdout capture — this avoids
   feeding untrusted plugin output through the VT parser.
2. Use a typed event bus (tokio broadcast channel) rather than Zellij's protobuf-serialized
   message passing — arcterm already has tokio in the dependency graph.
3. Keep the plugin manifest in TOML (not KDL) for consistency with `config.toml` and
   workspace files.

---

### Topic 4: WASI Preview 2 for the Capability Sandbox

**Background:** WASI 0.2.0 was finalized January 25, 2024. `wasmtime-wasi` v42.0.1 ships
both `p1` (legacy WASI preview 1 for core modules) and `p2` (Component Model, async, full
WASIp2) under the same version number as wasmtime itself.

**WasiCtxBuilder (p2) key methods:**
- `preopened_dir(dir, path)`: grants the plugin access to a specific host directory at a
  given guest path — directly implements the `filesystem: ["/allowed/path"]` permission from
  CONTEXT-6.md.
- `preopened_socket(fd)`: grants a pre-opened socket for network access — implements the
  `network: true` permission.
- `inherit_stdio()` / `allow_blocking_current_thread()`: controls whether the plugin can
  write to arcterm's stdout. For sandboxed plugins, stdio should be redirected to a captured
  buffer (used for draw command output).
- `env(key, value)`: sets environment variables visible to the plugin.

**WasiView trait requirement:** The host store's data type `T` in `Store<T>` must implement
`WasiView`, which provides `fn ctx(&mut self) -> &mut WasiCtx` and `fn table(&mut self) -> &mut ResourceTable`.
This is a two-field struct pattern — not onerous.

**Capability mapping from plugin.toml to WasiCtxBuilder:**

| plugin.toml field | WasiCtxBuilder call |
|-------------------|---------------------|
| `filesystem = ["/home/user/data"]` | `preopened_dir(cap_std::fs::Dir::open_ambient_dir("/home/user/data", ...), "/data")` |
| `network = true` | `preopened_socket(...)` or allow inheriting network |
| `network = false` (default) | No socket preopening (sandbox enforced by WASI, not honor-system) |
| `panes = "read"` / `"write"` | Not WASI — enforced via host function gating in WIT |
| `ai = true` | Not WASI — plugin can call `register-mcp-tool` host function if permission granted |

The filesystem and network sandbox is enforced by wasmtime/cap-std at the OS level — plugins
cannot access paths outside preopened directories regardless of what they attempt. The
`panes` and `ai` permissions are enforced at the host function level: the WIT host
implementation checks the plugin's declared capabilities before allowing `render-text` writes
to arbitrary panes or `register-mcp-tool` registration.

**No WASI for plugins that don't need it:** For plugins that only render text and subscribe
to events, WASI is optional. The `WasiCtx` can be configured with no preopened directories
and no network — effectively a no-op sandbox that still lets wasmtime load and run the
component. This keeps the zero-permission plugin case lightweight.

---

### Topic 5: Plugin Event Bus

**Chosen primitive: `tokio::sync::broadcast`**

The event bus delivers terminal events (PaneOpened, PaneClosed, CommandExecuted,
WorkspaceSwitched) from the arcterm host to zero-or-more interested plugins. This is a
fan-out pattern: one sender (the arcterm event emitter), N receivers (one per active plugin
instance).

`tokio::sync::broadcast` is already available — arcterm depends on `tokio = { version = "1",
features = ["full"] }` in the workspace `Cargo.toml`. The broadcast channel:
- Supports one producer, multiple consumers natively.
- Allows new receivers to subscribe at runtime (when a new plugin is loaded mid-session).
- Handles slow receivers via the `Lagged` error — if a plugin's event queue overflows, it
  receives a `Lagged(n)` error rather than blocking the sender. This is the correct behavior:
  a slow plugin must not stall terminal I/O.

**Event dispatch pattern:**

The `AppState` event loop (in `about_to_wait` / the PTY drain loop) already produces
semantic events implicitly: pane spawn (`spawn_pane()`), pane close (PTY channel disconnect),
workspace restore (`restore_workspace()`). In Phase 6, each of these code paths gets one
additional line: `plugin_event_tx.send(PluginEvent::PaneOpened { pane_id })`.

Plugin instances run in their own `tokio::task` (via `tokio::task::spawn`), receiving from
a `broadcast::Receiver<PluginEvent>`. The task calls the plugin's `update()` WIT export when
an event arrives, then calls `render()` if `update()` returns true.

**Why not `async-channel`:** It would require a new dependency and does not natively support
fan-out without per-plugin sender clones. `tokio::sync::broadcast` is the right tool and is
already present. `async-channel` (v2.5.0, Jul 2025) was considered but rejected on
dependency grounds.

**Why not `tokio::sync::mpsc`:** mpsc is single-consumer. Fan-out requires either one channel
per plugin (complex lifecycle management) or a manual broadcast loop. `broadcast` is
semantically correct for pub/sub.

---

### Topic 6: MCP Tool Registration

The MCP specification (2025-03-26) defines tool registration via `tools/list` and invocation
via `tools/call` over JSON-RPC 2.0. A tool definition requires: `name` (string), `description`
(string), `inputSchema` (JSON Schema object).

For Phase 6, arcterm implements a lightweight **in-process MCP tool registry** — not a full
MCP server. This is justified by CONTEXT-6.md's "MCP basics in Phase 6" scope and the Phase 7
plan for "full MCP orchestration."

The registry is a `HashMap<String, ToolSchema>` (tool name → JSON Schema + description).
Plugins call the `register-mcp-tool` WIT host function to populate it. When a Phase 7 AI
agent queries arcterm for available tools, arcterm responds with the registry contents in
MCP-compatible JSON format via `serde_json` (already a workspace dependency).

The `ToolSchema` struct:

```
name: String
description: String
input_schema: serde_json::Value   // JSON Schema
plugin_id: PluginId               // which plugin owns this tool
```

Tool invocation in Phase 6 is out-of-scope (that is Phase 7). Phase 6 only implements
`register` and `list`. This matches CONTEXT-6.md: "Plugins can register tool schemas. AI
agents can discover tools."

---

## Recommendation

### WASM Runtime: wasmtime v42 with Component Model

**Selected: `wasmtime = { version = "42", features = ["component-model", "async", "cranelift"] }`
plus `wasmtime-wasi = { version = "42", features = [] }` (p2 module)**

Justification: wasmtime's Component Model API is the only option that honors the CONTEXT-6.md
decision (wit-bindgen, typed WIT interface). It is the #1 WASM crate in the Rust ecosystem
by downloads (1.35M/month), actively maintained by the Bytecode Alliance with a formal
release process. The wasmtime core module API was rejected because it requires manual ABI
encoding with raw `Val` arrays — Zellij suffered through this before migrating, and the
whole point of CONTEXT-6.md's wit-bindgen choice is to avoid it. Extism was rejected because
it uses a custom non-WIT ABI, pins its own wasmtime version range (constraining arcterm's
ability to manage its own dependency), and has low download volume indicating a smaller
support community.

### Plugin Host API: wasmtime::component::bindgen! (not a separate crate)

**Selected: `wasmtime::component::bindgen!()` macro (host side) + separate WIT file at `arcterm-plugin/wit/arcterm.wit`**

The WIT file defines the plugin world. The `bindgen!` macro generates Rust traits that the
host implements. Guest plugin authors use `wit-bindgen` v0.53.1 (guest crate, not a host
dependency). This is the canonical Bytecode Alliance approach and is consistent with the
derive-macro patterns already used throughout the codebase. Manual protobuf/msgpack ABIs and
raw function tables were rejected as high-maintenance non-standard approaches that contradict
the stated architectural decision.

### Capability Sandbox: wasmtime-wasi p2

**Selected: `wasmtime_wasi::p2::WasiCtxBuilder` with per-plugin capability configuration**

Filesystem and network capabilities declared in `plugin.toml` are enforced by cap-std at the
OS level — this is stronger than honor-system checks. The `WasiView` trait requirement on
`Store<T>` is a modest implementation cost. WASI p1 was rejected because it is the legacy
API and is not compatible with Component Model. A custom host-only API was rejected as
reinventing well-tested infrastructure.

### Event Bus: `tokio::sync::broadcast`

**Selected: existing `tokio::sync::broadcast::channel::<PluginEvent>(256)` (no new dependency)**

Fan-out to N plugin instances, slow-receiver lagging (not blocking), and dynamic receiver
subscription all match the requirements. Already in the dependency graph. `async-channel` was
rejected on unnecessary dependency grounds. `tokio::sync::mpsc` was rejected on semantics
grounds (single consumer).

### MCP Integration: in-process tool registry in Phase 6

**Selected: `HashMap<String, ToolSchema>` in arcterm host, exposed via `register-mcp-tool` WIT import**

This is the minimum viable MCP integration aligned with CONTEXT-6.md. Full JSON-RPC MCP
server infrastructure is deferred to Phase 7 where AI agent orchestration is the primary
goal.

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| wasmtime breaks Component Model API in a monthly major bump | Med | Med | Pin to wasmtime = "42" in workspace Cargo.toml; update after each release by reviewing CHANGELOG. Patch releases (42.0.x) are API-stable by policy. |
| WIT file evolution breaks existing plugins | High (early versions) | Med | Ship WIT with a `@0.1.0` version in the package declaration. Publish the WIT file in arcterm's git repository. Document that v0.x has no stability guarantee. |
| Plugin exceeds 10MB memory limit | Med | Med | Set `StoreLimits { max_memory_size: Some(10 * 1024 * 1024) }` on each plugin Store. wasmtime traps immediately when the limit is hit, not after the fact. |
| Slow plugin blocks event bus | Low | High | `broadcast::Receiver::try_recv()` + `Lagged` handling in plugin task. Plugin task never awaits the host event loop. PTY drain loop in `about_to_wait` is unaffected. |
| Plugin panics / traps crash arcterm process | Med | High | Run each plugin in its own `tokio::task`. Catch `wasmtime::Trap` from WIT export calls. Log and mark the plugin as failed without propagating the panic. |
| WASI filesystem escape (path traversal) | Low | High | wasmtime/cap-std enforces preopened directory boundaries at the OS level — path traversal outside the preopened set returns WASI EBADF, not a vulnerability. |
| plugin.toml declares network=false but plugin finds another network path | Very low | High | WASI p2 without `preopened_socket` means no socket capability — there is no ambient network access to find. |
| WasiCtxBuilder async interface conflicts with winit event loop | Med | Med | Plugin instances run in background `tokio::task`s, not in the winit event handler. Draw commands are sent over a channel back to the main thread for render. |
| MCP tool registry grows unboundedly when plugins reload | Low | Low | Registry is keyed by `(PluginId, tool_name)`. Plugin unload removes all entries for that plugin. |
| wasmtime-wasi p2 WasiView boilerplate is error-prone | Low | Low | Extract a `PluginHostData` struct that implements `WasiView` once; all plugin stores share the same `T` type. |

---

## Implementation Considerations

### New workspace crate: `arcterm-plugin`

Phase 6 should introduce a new workspace member `arcterm-plugin` that owns:
- `wit/arcterm.wit` — the WIT world definition
- The `PluginRuntime`, `PluginInstance`, `PluginManifest`, and `PluginEventBus` types
- The `wasmtime::component::bindgen!()` invocation and host trait implementation

This keeps plugin system code isolated from `arcterm-app` and allows the WIT file to be
published separately for plugin authors.

### Integration points with existing `arcterm-app` code

1. **`AppState` in `main.rs`**: Add a `plugin_runtime: Option<PluginRuntime>` field.
   `PluginRuntime` holds the `Engine` (one per process), the `component::Linker`, and a
   `HashMap<PluginId, PluginInstance>`. The `Engine` is constructed once at startup.

2. **`spawn_pane` / pane lifecycle in `main.rs`**: After each `spawn_pane()` call, emit
   `plugin_event_tx.send(PluginEvent::PaneOpened { pane_id })`. After each pane close
   (channel disconnect path in `about_to_wait`), emit `PluginEvent::PaneClosed`.

3. **`TextRenderer::prepare_overlay_text` in `arcterm-render/src/text.rs`**: Plugin draw
   commands (styled text lines) are already handled by the multi-pane accumulator pattern
   (`prepare_grid_at` + `prepare_overlay_text` + `submit_text_areas`). Plugin panes are a
   new `PaneNode::Plugin { plugin_id }` variant that, at render time, calls the plugin's
   accumulated draw buffer instead of a `Terminal` grid.

4. **`CliCommand` in `main.rs`**: Add `Plugin { subcommand: PluginCliCommand }` to the
   existing `clap` enum. `PluginCliCommand` has variants `Install { path: PathBuf }`,
   `List`, `Remove { name: String }`, `Dev { path: PathBuf }`.

5. **`config.rs`**: Plugin storage path follows the established `dirs::config_dir()` pattern:
   `dirs::config_dir().join("arcterm").join("plugins")`. The `PluginManifest` struct uses
   `#[derive(Deserialize)]` on TOML — identical pattern to `ArctermConfig`.

### `PaneNode` extension for plugin panes

The current `PaneNode` enum in `layout.rs`:

```rust
pub enum PaneNode {
    Leaf { pane_id: PaneId },
    HSplit { left: Box<PaneNode>, right: Box<PaneNode>, ratio: f32 },
    VSplit { top: Box<PaneNode>, bottom: Box<PaneNode>, ratio: f32 },
}
```

Phase 6 adds a variant:

```rust
PluginPane { pane_id: PaneId, plugin_id: PluginId },
```

This allows plugin panes to participate in the existing layout engine, border rendering,
and tab management without modification to those subsystems.

### Plugin manifest TOML structure

```toml
name = "my-plugin"
version = "0.1.0"
api-version = "0.1"
wasm = "my_plugin.wasm"

[permissions]
filesystem = []          # empty list = no filesystem access
network = false
panes = "read"           # "none" | "read" | "write"
ai = false               # register MCP tools
```

Parsed via `#[derive(Deserialize)]` on a `PluginManifest` struct — no new parsing library
needed.

### wasmtime instance lifecycle

```
PluginRuntime::load_plugin(manifest, wasm_bytes):
  1. Compile: component::Component::from_binary(&engine, &wasm_bytes)
     [expensive — do once, cache on disk via Module::serialize if compilation time matters]
  2. Build WasiCtx from manifest.permissions using WasiCtxBuilder
  3. Create Store<PluginHostData> with StoreLimits and fuel enabled
  4. Instantiate via linker: Plugin::instantiate(&mut store, &component, &linker)
  5. Call plugin.plugin_main().load(&mut store, config_kv_pairs)
  6. Store PluginInstance { store, plugin_exports } in HashMap<PluginId, PluginInstance>
  7. Spawn tokio::task for event loop
```

### Plugin render loop

The plugin task receives `PluginEvent` from the broadcast channel. On each event:

1. Call `plugin.plugin_main().update(&mut store, event)` → returns `bool`
2. If true, call `plugin.plugin_main().render(&mut store, rows, cols)`
3. The `render()` call triggers the plugin to call the `host.render-text()` WIT import,
   which the host implements by appending `StyledLine` items to the plugin pane's draw
   buffer.
4. The draw buffer is read by the render thread on the next frame (a lock-free swap pattern
   using `Arc<Mutex<Vec<StyledLine>>>` is sufficient at this scale).

### Testing strategy

- Unit test: compile a minimal WASM component from WAT source (no real plugin needed) and
  verify the host trait dispatch works end-to-end.
- Integration test: build the example "hello world" plugin as part of CI
  (`cargo build --target wasm32-wasip2`), load it via `PluginRuntime`, assert it renders
  the expected text.
- Permission test: assert that a plugin manifest with `network = false` cannot call any
  socket-creating host function (WASI returns EBADF).
- Memory limit test: write a plugin that allocates beyond the 10MB limit and assert wasmtime
  traps cleanly without crashing the host process.

### Performance implications

- **Load time (50ms goal):** `component::Component::from_binary` performs JIT compilation via
  Cranelift. For a "hello world" plugin (~50KB WASM), this is typically under 10ms on
  modern hardware. For larger plugins, serialized AOT compilation (`Module::serialize` /
  `Module::deserialize`) can be used to cache compiled artifacts and hit the 50ms target
  reliably.
- **Memory overhead per plugin:** A minimal wasmtime Store is under 1MB overhead. The 10MB
  target is achievable with `StoreLimits`. The Cranelift compiled artifact is shared across
  instantiations of the same component (the `component::Component` is `Arc`-based internally).
- **Render loop:** Plugin renders happen in background `tokio::task`s, completely isolated
  from the winit event loop and PTY drain path. There is no shared lock on the hot path.

---

## Sources

1. lib.rs crate page for wasmtime: https://lib.rs/crates/wasmtime
2. wasmtime docs.rs API reference: https://docs.rs/wasmtime/latest/wasmtime/
3. wasmtime Config docs (memory limits, fuel, epoch): https://docs.rs/wasmtime/latest/wasmtime/struct.Config.html
4. wasmtime release process and semver policy: https://docs.wasmtime.dev/stability-release.html
5. lib.rs crate page for wasmtime-wasi: https://lib.rs/crates/wasmtime-wasi
6. wasmtime-wasi docs.rs (WasiCtxBuilder, WasiView, p2 module): https://docs.rs/wasmtime-wasi/42.0.1/wasmtime_wasi/
7. lib.rs crate page for wit-bindgen: https://lib.rs/crates/wit-bindgen
8. wit-bindgen README (wasmtime::component::bindgen! macro): https://raw.githubusercontent.com/bytecodealliance/wit-bindgen/main/README.md
9. wasmtime::component::bindgen! macro docs: https://docs.wasmtime.dev/api/wasmtime/component/macro.bindgen.html
10. WebAssembly Component Model introduction: https://component-model.bytecodealliance.org/introduction.html
11. Zellij plugin architecture overview: https://zellij.dev/documentation/plugin-api.html
12. zellij-tile docs.rs (ZellijPlugin trait, Event enum): https://docs.rs/zellij-tile/latest/zellij_tile/
13. zellij-tile source (lib.rs, shim.rs): https://raw.githubusercontent.com/zellij-org/zellij/main/zellij-tile/src/lib.rs
14. lib.rs crate page for extism: https://lib.rs/crates/extism
15. Extism README (architecture, ABI, language SDKs): https://raw.githubusercontent.com/extism/extism/main/README.md
16. MCP Tool specification (2025-03-26): https://modelcontextprotocol.io/specification/2025-03-26/server/tools
17. lib.rs crate page for async-channel: https://lib.rs/crates/async-channel
18. wasmtime WASI p1 example: https://docs.wasmtime.dev/examples-wasip1.html

---

## Uncertainty Flags

- **wasmtime Component Model API churn:** The `wit-bindgen` crate has had 51 breaking releases
  noted on lib.rs as of March 2026. While the `wasmtime::component::bindgen!` macro (host
  side) has been more stable than the guest-side crate, the exact degree of host API stability
  between monthly wasmtime major versions was not independently verified. The recommendation
  is to pin to wasmtime = "42" initially and establish a documented upgrade procedure.

- **tokio::task + wasmtime async interaction:** wasmtime's async mode requires that the
  tokio runtime be active when calling async WIT exports. arcterm currently uses
  `tokio::runtime::Builder::new_multi_thread()` which is compatible, but the exact
  interaction between `tokio::task::spawn` (for plugin tasks) and wasmtime's async Store
  was not benchmarked. If async WIT exports exhibit unexpected latency, synchronous WIT
  exports (using `consume_fuel` for yield points instead of async) are a viable fallback.

- **`wasm32-wasip2` toolchain support on stable Rust:** As of March 2026, the
  `wasm32-wasip2` compilation target (required for Component Model guest plugins) requires
  Rust nightly for some use cases, though the target itself was stabilized. Plugin authors
  may need `wasm32-wasi` (p1) compiled through `wasm-tools component` post-processing, or
  may need to pin to a specific nightly. This should be verified before publishing the plugin
  authoring guide.

- **Zellij plugin permissions enforcement detail:** The specific mechanism by which Zellij
  enforces its permission types at runtime (beyond WASI capability preopening) was not
  directly accessible in the documentation — several pages returned 404 or 429 during
  research. The analysis above is inferred from the tile API and public documentation. A
  direct review of `zellij-utils/src/plugin_api/plugin_permission.rs` would clarify the
  enforcement model if arcterm wants to mirror Zellij's approach exactly.

- **WasiCtxBuilder `preopened_socket` API stability in p2 module:** The docs confirmed this
  method exists but the exact signature (file descriptor type, permission flags) was not
  extracted. This needs verification against the actual `wasmtime-wasi` 42.x docs before
  implementing the `network = true` permission path.

- **Plugin AOT compilation cache location:** The research confirmed `Module::serialize` /
  `Module::deserialize` API exists for caching Cranelift-compiled artifacts. The appropriate
  cache location (`~/.cache/arcterm/plugins/`) and cache invalidation strategy (hash of WASM
  bytes) were not designed in this research phase — that is an implementation detail for the
  architect.
