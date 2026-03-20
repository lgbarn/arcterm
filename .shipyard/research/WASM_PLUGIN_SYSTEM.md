# Research: ArcTerm WASM Plugin System

## Context

ArcTerm is a Rust workspace fork of WezTerm (~60 crates). The project has an existing Lua 5.4 scripting
system (mlua 0.9) and plans to add a WASM plugin system (`arcterm-wasm-plugin`) alongside it, not as a
replacement. No WASM runtime exists in the workspace today. This document covers the four research areas
needed before implementation begins.

---

## Topic 1: WezTerm's Existing Plugin Extension Points

### How the Lua plugin system loads and initializes

The Lua context is created in `config/src/lua.rs` by the `make_lua_context()` function (line 211). It
constructs a fresh `mlua::Lua` instance, configures `package.path` to search ArcTerm-specific config
directories, registers a patched file searcher for watch-list tracking, populates a `wezterm` global
module with core helpers, and then **iterates the `SETUP_FUNCS` registry** (lines 384-386), calling
each registered `fn(&Lua) -> anyhow::Result<()>`.

The registration mechanism is:

```
// config/src/lua.rs, line 23-31
pub type SetupFunc = fn(&Lua) -> anyhow::Result<()>;
static SETUP_FUNCS: Mutex<Vec<SetupFunc>> = Mutex::new(vec![]);

pub fn add_context_setup_func(func: SetupFunc) {
    SETUP_FUNCS.lock().unwrap().push(func);
}
```

Any crate that wants to expose a Lua API registers a function pointer into this global list before
`make_lua_context` is called. `make_lua_context` then calls every registered function, giving each one
a reference to the live Lua state to populate.

### Where registration happens

There are two call sites:

1. **`env-bootstrap/src/lib.rs` lines 190-206** (`register_lua_modules()`): registers the 14 core
   lua-api-crates (battery, color-funcs, termwiz-funcs, logging, mux-lua, procinfo-funcs, filesystem,
   serde-funcs, plugin, ssh-funcs, spawn-funcs, share-data, time-funcs, url-funcs). This runs for both
   the GUI and the headless mux server.

2. **`wezterm-gui/src/main.rs` lines 1205-1207**: registers three GUI-specific modules
   (`window_funcs::register`, `crate::scripting::register`, `crate::stats::register`). These are
   GUI-only and run after `env_bootstrap::bootstrap()`.

### What terminal state is already exposed to Lua

The `lua-api-crates/mux/src/pane.rs` `MuxPane` UserData type exposes the following `Pane` trait
surface to Lua scripts:

| Method | Underlying Pane call |
|--------|---------------------|
| `pane_id` | `pane_id()` |
| `get_title` | `get_title()` |
| `get_cursor_position` | `get_cursor_position()` |
| `get_dimensions` | `get_dimensions()` (returns `RenderableDimensions`) |
| `get_lines_as_text(n)` | `get_lines(range)` then text extraction |
| `get_lines_as_escapes(n)` | `get_lines(range)` then re-encoding as escape sequences |
| `get_logical_lines_as_text(n)` | `get_logical_lines(range)` |
| `get_semantic_zones` | `get_semantic_zones()` |
| `get_semantic_zone_at(x, y)` | binary search over semantic zones |
| `get_text_from_semantic_zone` | `get_logical_lines(range)` |
| `get_text_from_region(x0,y0,x1,y1)` | same |
| `get_current_working_dir` | `get_current_working_dir(FetchImmediate)` |
| `get_foreground_process_name` | `get_foreground_process_name(FetchImmediate)` |
| `get_foreground_process_info` | `get_foreground_process_info(AllowStale)` |
| `get_user_vars` | `copy_user_vars()` |
| `get_metadata` | `get_metadata()` |
| `has_unseen_output` | `has_unseen_output()` |
| `is_alt_screen_active` | `is_alt_screen_active()` |
| `send_paste(text)` | `send_paste()` |
| `send_text(text)` | `writer().write_all()` |
| `inject_output(text)` | parses escape sequences then `perform_actions()` |
| `split` (async) | `Domain::split_pane()` |
| `move_to_new_tab` (async) | `Mux::move_pane_to_new_tab()` |
| `activate` | `Tab::set_active_pane()` |

The mux Lua module also exposes `get_window`, `get_pane`, `get_tab`, `all_windows`, `all_domains`,
`spawn_window`, workspace management, and domain switching. This is the complete API surface the WASM
host must match or subset.

### Key event hooks and callbacks

The event/callback system is in `config/src/lua.rs`:

- `wezterm.on(event_name, fn)` — `register_event()` (line 722): stores handler functions in the Lua
  registry under `"wezterm-event-{name}"` as a sequential table, allowing multiple handlers per event.
- `wezterm.emit(event_name, ...)` — `emit_event()` async (line 767): calls each handler in order;
  handler returning `false` stops the chain and signals "prevent default action".
- `emit_sync_callback` and `emit_async_callback` (lines 795, 816): host-side entry points for firing
  events into Lua from Rust.

The GUI emits events via `config::with_lua_config_on_main_thread` which marshals calls onto the GUI
thread. The event names observed in the codebase include `"window-resized"`, `"window-focus-changed"`,
`"update-right-status"`, `"format-tab-title"`, `"format-window-title"`, `"open-uri"`,
`"mux-startup"`, and key/mouse assignment callbacks.

### How plugins are declared in config

The existing Lua plugin system (`lua-api-crates/plugin/`) handles `wezterm.plugin.require()` which
loads external Lua plugins from `$DATA_DIR/plugins/?/plugin/init.lua`. The `package.path` is
pre-populated with this path in `make_lua_context()` (line 237).

For a WASM plugin system, plugin declarations would most naturally live as a new config field on the
`Config` struct in `config/src/config.rs`, following the existing pattern of typed config fields
(similar to how `serial_ports`, `wsl_domains`, and `exec_domains` are declared).

**Key finding**: The `add_context_setup_func` mechanism is the single registration point for all
plugin API crates. An `arcterm-wasm-plugin` crate would register itself here to add a
`wezterm.wasm_plugin` (or `arcterm.plugin`) Lua table, and separately would need to hook
`MuxNotification` subscribers for event dispatch to loaded WASM modules.

---

## Topic 2: Wasmtime Component Model in Rust

### Latest version and release cadence

- **Latest stable version**: 42.0.1, released February 25, 2026
  (Source: docs.rs/crate/wasmtime/latest)
- **Previous version referenced**: 41.0.4, released February 24, 2026
- **Release cadence**: One new semver-major version per month, branched on the 5th, released on the
  20th. This means **12 major versions per year**, each of which reserves the right to break the API.
  Source: docs.wasmtime.dev/stability-release.html

### LTS policy

LTS releases occur every 12 releases (version numbers divisible by 12) and receive **24 months** of
support. Non-LTS releases receive only **2 months** of support. Security fixes are backported to all
supported releases; bug fixes are volunteer-effort only. The last LTS version would be v36.x (36 is
divisible by 12); the next is v48.x.

**Implication**: Pinning a non-LTS wasmtime version means it goes unsupported within 2 months. The
project should pin to an LTS release (currently v36.x) or accept updating wasmtime every 1-2 months.

### Breaking change history (v41)

Wasmtime 41 introduced a notable API break: `wasmtime::Error` and `wasmtime::Result` are now defined
in the wasmtime crate itself rather than being re-exports from `anyhow`. The crate now exports a
mostly-compatible anyhow-like API at `wasmtime::error`. This is the type of change that occurs in
nearly every major version.

### Component Model support

The component model is available behind the `component-model` feature flag, which is enabled by
default. Key structures:

- **`wasmtime::component::Component`**: compiled component, analogous to `wasmtime::Module` for core
  wasm. Takes compiled WIT-generated `.wasm` files.
- **`wasmtime::component::Linker`**: used to define host functions that components can import. Host
  functions are attached via `func_wrap()` or via the type-safe bindings generated by `bindgen!`.
- **`wasmtime::component::bindgen!` macro**: takes a WIT world as input and generates:
  - Rust traits the host must implement (for imports)
  - Rust structs for exported component functions
  - Type mappings for all WIT primitive and compound types

### WIT (WebAssembly Interface Types)

WIT is the IDL for the Component Model. A WIT package defines interfaces and worlds:

```wit
package arcterm:plugin@0.1.0;

interface terminal-api {
    get-pane-text: func(pane-id: u32, nlines: u32) -> string;
    send-text: func(pane-id: u32, text: string);
}

interface plugin-lifecycle {
    on-pane-output: func(pane-id: u32);
    on-window-focus: func(window-id: u32);
}

world arcterm-plugin {
    import terminal-api;
    export plugin-lifecycle;
}
```

The host implements `terminal-api` (the imports); each plugin WASM binary exports `plugin-lifecycle`
(the exports). The `bindgen!` macro turns this into typed Rust traits at compile time.

### Capability-based security

Wasmtime's sandbox properties:

- All memory accesses are bounds-checked; the callstack is inaccessible from within WASM.
- All I/O must go through explicitly imported host functions; there are no implicit syscalls.
- Control flow transfers are to known, type-checked destinations only.
- Additional defenses: 2 GB guard regions on linear memories, guard pages on thread stacks, memory
  zeroing after instance completion, CFI via hardware features.

Capability-based security in practice means: **a plugin can only do what its host imports allow**.
If the host does not expose a filesystem import, the plugin cannot access the filesystem. This is
the correct model for ArcTerm plugins: expose only `terminal-api` functions, and plugins are
structurally incapable of, for example, spawning processes or writing to disk unless explicitly
given those imports.

Remaining attack surface: Cranelift compiler bugs could theoretically break sandbox isolation. The
Bytecode Alliance maintains a security policy and CVE disclosure process for such issues.

### Fuel-based execution budgets

`wasmtime::Config::consume_fuel(true)` instruments generated code to deduct "fuel" on each
operation. `store.add_fuel(n)` sets the budget. When fuel runs out, execution traps. This provides
deterministic, cooperative infinite-loop prevention. In async mode, the store can also be configured
to yield to the host every N fuel units via `Config::fuel_async_yielding_interval`, allowing
cooperative multitasking without OS thread blocking.

### Exposing async Rust functions as WASM host imports

Wasmtime supports async stores (`Config::async_support(true)`). With async support:

- Host functions can be `async fn` closures.
- `Linker::func_wrap_async()` accepts closures that return `impl Future`.
- The WASM guest sees these as synchronous (blocking from its perspective), but from the host's
  perspective they are `async` and integrate with whatever executor is driving the store.
- The component model `bindgen!` macro supports `async: true` to generate async-aware bindings.

**Compatibility note**: ArcTerm uses `smol` as its background executor, not Tokio. Wasmtime's async
support is executor-agnostic at the `std::future::Future` level, but its underlying thread-stack
switching uses `wasmtime-fiber`. This is compatible with `smol`; the WASM plugin executor would be
a `smol` task that drives the wasmtime async store.

---

## Comparison Matrix: WASM Runtime Options

| Criteria | Wasmtime | Wasmer | Extism |
|----------|----------|--------|--------|
| Maturity | v42, production since 2019 | v4.x, production since 2019 | v1.x, higher-level wrapper |
| Backing | Bytecode Alliance (Mozilla, Intel, Fastly) | Wasmer Inc. (VC-backed startup) | Built on wasmtime/wasmer |
| Component Model | Native, first-class | Partial (WASIX focus) | No |
| WIT / bindgen! | Yes, `bindgen!` macro | No native WIT tooling | No |
| Fuel metering | Yes, built-in | Yes (metering middleware) | Partial |
| Async host imports | Yes (fiber-based) | Limited | No |
| License | Apache 2.0 | MIT/Apache | BSD-3 |
| Release cadence | Monthly major, breaks API | Quarterly major | Infrequent |
| LTS support | Every 12 versions, 24 months | No formal LTS | N/A |
| Rust integration | Tight, first-party Rust API | Good | Good |
| Stack compatibility | smol-compatible async | smol-compatible | N/A |
| Binary size (libwasmtime) | ~12 MB (with Cranelift) | ~8-15 MB | Smaller (uses runtime) |
| WASI support | Full WASIp2 (preview 2) | WASIX (diverged from standard) | Subset |

**Sources**: Bytecode Alliance blog, docs.wasmtime.dev, wasmer.io, extism.org

### Detailed analysis

**Wasmtime**: The only runtime with first-class Component Model and WIT support. The `bindgen!` macro
eliminates the need to write manual type translation glue. Async host imports via `wasmtime-fiber` are
production-ready. The monthly breaking-change cadence is a real maintenance burden — pinning to an LTS
version (every 12th release) mitigates this substantially.

**Wasmer**: A mature alternative but it has diverged from the WASI standard (adopting "WASIX") and
lacks native Component Model support. WIT tooling would need to be written manually. Not recommended
unless the project specifically needs the Wasmer Cloud deployment model.

**Extism**: A plugin-system library built on top of wasmtime or wasmer. It provides a simpler API but
sacrifices the Component Model, type safety, and async. Appropriate for simple scripting plugins; too
limited for ArcTerm's needs (async pane reads, event streaming).

---

## Topic 3: Crate Structure

### Adding a new crate to the workspace

The workspace root `Cargo.toml` uses the pattern:

```toml
[workspace]
members = [
  "bidi",
  "wezterm-gui",
  # ...
]
resolver = "2"
```

To add `arcterm-wasm-plugin`:
1. Create the directory `arcterm-wasm-plugin/` with its own `Cargo.toml` and `src/lib.rs`.
2. Add `"arcterm-wasm-plugin"` to the `members` array in the root `Cargo.toml`.
3. Add a workspace dependency entry:
   ```toml
   # In [workspace.dependencies]
   arcterm-wasm-plugin = { path = "arcterm-wasm-plugin" }
   ```
4. Declare it as a dependency in `wezterm-gui/Cargo.toml` and/or `env-bootstrap/Cargo.toml`.

The ArcTerm `CLAUDE.md` convention is: "Keep ArcTerm-specific code in dedicated `arcterm-*` crates
to minimize merge conflicts." Using `arcterm-wasm-plugin` as the crate name (not `wasm-plugin`) is
the correct convention.

### Should the crate depend on `config` or be standalone?

The `config` crate is the integration point for the Lua system. An `arcterm-wasm-plugin` crate
needs to:

1. Register itself via `config::lua::add_context_setup_func` to expose a Lua-level API for loading
   WASM plugins from config.
2. Read plugin declarations from the `Config` struct.
3. Subscribe to `mux::MuxNotification` to receive pane events and dispatch them to WASM modules.

**Recommendation**: Depend on `config` and `mux`, following the exact pattern of the existing
`lua-api-crates/mux/` crate. Do not make it standalone — the registration hook into `SETUP_FUNCS`
requires a `config` dependency, and event dispatch requires a `mux` dependency.

Do **not** put `arcterm-wasm-plugin` inside `lua-api-crates/` because it is architecturally a peer
of the Lua system, not a sub-module of it.

### Wiring into `wezterm-gui`

Following the existing pattern in `env-bootstrap/src/lib.rs` lines 190-206, one line is added to
`register_lua_modules()`:

```rust
config::lua::add_context_setup_func(arcterm_wasm_plugin::register_lua_api);
```

And separately, after `Mux::new()` is called in `wezterm-gui/src/main.rs`, the plugin manager
subscribes to `MuxNotification`:

```rust
arcterm_wasm_plugin::start_plugin_manager(&mux);
```

This matches how `GuiFrontEnd` subscribes to `MuxNotification` in `wezterm-gui/src/frontend.rs`
lines 52-80. The WASM plugin manager would be a parallel subscriber that dispatches relevant
notifications to loaded plugin WASM instances.

---

## Topic 4: Terminal State API Surface

### `Pane` trait (`mux/src/pane.rs`)

The `Pane` trait (line 167, `#[async_trait(?Send)]`) is the primary abstraction for terminal state.
Methods directly useful for a WASM host API:

| Method signature | Purpose |
|---|---|
| `fn pane_id(&self) -> PaneId` | Identity |
| `fn get_cursor_position(&self) -> StableCursorPosition` | Cursor location, shape, visibility |
| `fn get_current_seqno(&self) -> SequenceNo` | Change sequence number for dirty tracking |
| `fn get_changed_since(lines: Range<StableRowIndex>, seqno: SequenceNo) -> RangeSet` | Dirty line tracking since last read |
| `fn get_lines(lines: Range<StableRowIndex>) -> (StableRowIndex, Vec<Line>)` | Bulk line read; returns actual first row (may differ from requested due to scrollback) |
| `fn get_logical_lines(lines: Range<StableRowIndex>) -> Vec<LogicalLine>` | Wrapped-line-aware read; `LogicalLine` contains both physical and logical views |
| `fn get_dimensions(&self) -> RenderableDimensions` | viewport_rows, physical_top, scrollback_rows, cols, dpi |
| `fn get_title(&self) -> String` | Current terminal title |
| `fn get_current_working_dir(policy) -> Option<Url>` | CWD via OSC 7 |
| `fn get_foreground_process_name(policy) -> Option<String>` | Process name |
| `fn get_foreground_process_info(policy) -> Option<LocalProcessInfo>` | Full process tree info |
| `fn copy_user_vars() -> HashMap<String, String>` | OSC 1337 user vars |
| `fn get_semantic_zones() -> anyhow::Result<Vec<SemanticZone>>` | Shell-integration semantic zones (prompt, input, output) |
| `fn get_metadata(&self) -> Value` | Pane-type-specific metadata (dynamic value) |
| `fn send_paste(&self, text: &str)` | Send text as paste |
| `fn writer(&self) -> MappedMutexGuard<dyn Write>` | Raw PTY write access |
| `fn perform_actions(&self, actions: Vec<Action>)` | Inject parsed terminal actions |
| `fn is_dead(&self) -> bool` | Whether pane process has exited |
| `fn is_mouse_grabbed(&self) -> bool` | Mouse capture state |
| `fn is_alt_screen_active(&self) -> bool` | Alternate screen state |
| `async fn search(pattern, range, limit) -> Vec<SearchResult>` | Scrollback search |

### `Screen` struct (`term/src/screen.rs`)

The `Screen` holds terminal lines as a `VecDeque<Line>` with `stable_row_index_offset` for
translating between stable and physical indices. Key public fields: `physical_rows`, `physical_cols`,
`dpi`, `saved_cursor`. The screen is accessed through the `Terminal` struct which is behind
`LocalPane`'s mutex — the `Pane` trait methods are the correct public interface; direct `Screen`
access is not exposed and would require holding a lock.

### `MuxNotification` events (`mux/src/lib.rs` lines 57-98)

These are the event hooks available for a WASM plugin dispatcher:

| Variant | Trigger |
|---|---|
| `PaneOutput(PaneId)` | New output processed for a pane (high frequency) |
| `PaneAdded(PaneId)` | New pane created |
| `PaneRemoved(PaneId)` | Pane closed |
| `PaneFocused(PaneId)` | Pane received focus |
| `WindowCreated(WindowId)` | New window opened |
| `WindowRemoved(WindowId)` | Window closed |
| `WindowInvalidated(WindowId)` | Window needs repaint |
| `TabAddedToWindow { tab_id, window_id }` | Tab created |
| `TabResized(TabId)` | Tab dimensions changed |
| `TabTitleChanged { tab_id, title }` | Title updated |
| `Alert { pane_id, alert }` | Terminal alert (bell, notification) |
| `AssignClipboard { pane_id, selection, clipboard }` | Clipboard write |

`PaneOutput` fires after every batch of parsed PTY bytes and is the most frequent event. A WASM
plugin system must be careful not to invoke WASM for every `PaneOutput` without debouncing.

### `Domain` trait (`mux/src/domain.rs` lines 49-199)

The `Domain` trait is relevant for the future possibility of a "WASM sandbox domain" that itself
provides PTY-like panes. Key async methods: `spawn_pane()`, `spawn()`, `split_pane()`,
`attach()`, `detach()`. Implementing this trait allows the WASM plugin system to serve as a domain
that hosts purely WASM-driven panes (e.g., for an AI chat pane), which
aligns with the ARCHITECTURE.md observation that "new Domain types" is an identified extension point.

---

## Recommendation

**For the WASM runtime: Wasmtime v36.x (current LTS)**

Wasmtime is the only viable choice. No other runtime provides all three required capabilities
simultaneously: Component Model with WIT/bindgen!, async host imports, and production-quality
sandboxing. Wasmer was not chosen because it lacks native Component Model support and has diverged
from the WASI standard. Extism was not chosen because it lacks the async event model and type-safe
WIT interface generation required for a full plugin API.

Pin to the v36.x LTS series (the most recent LTS, version number divisible by 12 before v42).
This provides 24 months of security support without forced API migrations every month. Plan an
annual upgrade to the next LTS (v48.x when it releases).

**For crate structure: `arcterm-wasm-plugin` as a first-party workspace crate**

Follow the same two-phase registration used by all other lua-api-crates:
1. Call `config::lua::add_context_setup_func(arcterm_wasm_plugin::register_lua_api)` in
   `env-bootstrap/src/lib.rs` to expose the plugin-loading API to Lua.
2. Call `arcterm_wasm_plugin::start_plugin_manager()` after `Mux::new()` in `wezterm-gui/src/main.rs`
   to subscribe to `MuxNotification` and dispatch events to WASM instances.

**For the WIT host API surface: subset the existing Lua API**

The Lua API surface documented above is the proven, stable API surface. The initial WIT world should
expose a conservative subset — read-only pane state (`get-pane-text`, `get-cursor-position`,
`get-dimensions`, `get-title`, `get-cwd`) plus write operations (`send-text`, `send-paste`,
`inject-output`). Avoid exposing `spawn_pane`, `split`, or `Domain` management in v1 — these carry
significantly higher complexity and risk surface.

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Wasmtime monthly breaking changes disrupt the integration | High | Medium | Pin to LTS release (v36.x); schedule annual LTS upgrades. Use a thin adapter layer between arcterm-wasm-plugin and wasmtime's public API. |
| `PaneOutput` event storms overwhelm the WASM dispatcher | High | High | Gate WASM dispatch behind an opt-in subscription per-plugin; debounce with a minimum interval (e.g., 16ms). Never call WASM synchronously in the mux notification callback — post to a background task. |
| WASM plugin deadlocks on Pane mutex (Pane methods hold locks) | Medium | High | All host import implementations that call `Pane` methods must be invoked from a background task, never from the GUI main thread. Use `Mux::get_pane()` with Arc cloning to avoid holding the global pane lock during WASM execution. |
| Cranelift compiler bug breaks sandbox isolation | Low | Critical | Track Bytecode Alliance CVE disclosures. Enable `Config::cranelift_nan_canonicalization` and stack overflow detection. Consider limiting plugins to pre-compiled `.wasm` binaries (no JIT recompilation at load time would still occur, but restricts source footprint). |
| Async WASM host imports conflict with smol executor | Medium | High | Drive the wasmtime async `Store` from a dedicated `smol` task per plugin instance; never poll the WASM future from the GUI main thread. |
| Large plugin memory consumption | Medium | Medium | Set `wasmtime::Config::max_wasm_stack` and impose a linear memory limit via `ResourceLimiter` on the `Store`. Start with 64 MB linear memory cap per plugin. |
| WIT interface breaks backward compatibility as API evolves | Low | Medium | Version the WIT package (`arcterm:plugin@0.1.0`). Commit to not removing functions in v0.x; bump to v1.0.0 before declaring stability. |
| Plugin loading hangs on startup | Low | Medium | Load plugins asynchronously after the first window opens, not during config initialization. Enforce a per-plugin startup fuel limit. |

---

## Implementation Considerations

### Integration points with existing code

1. **`env-bootstrap/src/lib.rs`** (`register_lua_modules`): add one line to register the WASM
   plugin Lua API module. This is the lowest-impact touch point; it requires adding `arcterm-wasm-plugin`
   as a dependency of `env-bootstrap`.

2. **`wezterm-gui/src/main.rs`** (after `Mux::new()` and before `run_terminal_gui()`): add the
   `MuxNotification` subscriber for event dispatch.

3. **`config/src/config.rs`**: add a `wasm_plugins: Vec<WasmPluginConfig>` field to the `Config`
   struct to declare plugin paths. This follows the existing pattern for `serial_ports`, `wsl_domains`.

4. **No changes needed** to `mux/src/pane.rs`, `mux/src/lib.rs`, or any `term/` crates. The
   `arcterm-wasm-plugin` crate consumes these as read-only dependencies.

### Migration path

There is no existing WASM plugin system to migrate from. The Lua plugin system remains unchanged;
WASM plugins are additive. A user may use both simultaneously.

### Testing strategy

- Unit test each WIT host import implementation against a minimal `wat`-format WASM module (no
  toolchain required — `wat::parse_str()` from the `wat` crate).
- Integration test with a compiled Rust `wasm32-wasip2` test plugin that exercises the full API.
- Fuel metering tests: verify that a tight loop in WASM traps within expected fuel budget.
- Mutex/threading tests: verify that host imports called from a background smol task do not
  deadlock with the GUI main thread.

### Performance implications

- `PaneOutput` fires on every PTY read batch. Dispatching to WASM per-event is expensive (each
  WASM call crosses the host/guest boundary). Recommended approach: maintain a per-plugin dirty
  flag set in the `MuxNotification` handler; a dedicated background task polls these flags on a
  timer (e.g., 50ms) and batch-invokes the WASM callback.
- Wasmtime's Cranelift JIT means first-invocation cost (compilation) happens at plugin load time,
  not at call time. Subsequent calls are near-native speed.
- Linear memory for typical pane text queries (viewport content) is small. A 16 KB buffer covers
  a 240x80 viewport with room to spare.

---

## Sources

1. `config/src/lua.rs` — Lua context creation, `add_context_setup_func`, `register_event`,
   `emit_event` (observed directly)
2. `env-bootstrap/src/lib.rs` lines 190-206 — `register_lua_modules()` call pattern (observed directly)
3. `wezterm-gui/src/main.rs` lines 1205-1207 — GUI-specific module registration (observed directly)
4. `lua-api-crates/mux/src/pane.rs` — Full Lua pane API surface (observed directly)
5. `lua-api-crates/mux/src/lib.rs` — Mux module registration pattern (observed directly)
6. `mux/src/pane.rs` lines 167-340 — `Pane` trait definition (observed directly)
7. `mux/src/lib.rs` lines 57-98 — `MuxNotification` enum (observed directly)
8. `mux/src/domain.rs` lines 49-199 — `Domain` trait definition (observed directly)
9. `term/src/screen.rs` lines 1-55 — `Screen` struct definition (observed directly)
10. `Cargo.toml` — workspace members and workspace dependencies (observed directly)
11. [https://docs.rs/crate/wasmtime/latest](https://docs.rs/crate/wasmtime/latest) — wasmtime 42.0.1, released 2026-02-25
12. [https://docs.rs/wasmtime/latest/wasmtime/component/index.html](https://docs.rs/wasmtime/latest/wasmtime/component/index.html) — Component model, bindgen!, Linker, Store
13. [https://docs.wasmtime.dev/stability-release.html](https://docs.wasmtime.dev/stability-release.html) — Monthly release cadence, LTS policy
14. [https://docs.wasmtime.dev/security.html](https://docs.wasmtime.dev/security.html) — Sandbox security properties and remaining attack surface
15. [https://docs.wasmtime.dev/api/wasmtime/component/bindgen_examples/_7_async/index.html](https://docs.wasmtime.dev/api/wasmtime/component/bindgen_examples/_7_async/index.html) — Async bindgen examples
16. [https://bytecodealliance.org/articles/wasmtime-lts](https://bytecodealliance.org/articles/wasmtime-lts) — LTS release model
17. [https://github.com/bytecodealliance/wasmtime/releases/tag/v41.0.4](https://github.com/bytecodealliance/wasmtime/releases/tag/v41.0.4) — v41 breaking changes (anyhow::Error migration)
18. [https://github.com/bytecodealliance/wit-bindgen](https://github.com/bytecodealliance/wit-bindgen) — wit-bindgen toolchain

---

## Uncertainty Flags

- **Wasmtime version pinning vs. LTS**: The search results confirm v42.0.1 is latest as of
  2026-02-25, but the most recent LTS (divisible by 12) prior to 42 is v36.x. Whether v36.x crates.io
  versions are still available and compatible with Rust 1.71.0 (ArcTerm's minimum) was not verified.
  The minimum Rust version required by wasmtime 36.x should be checked before the implementation
  begins.

- **smol + wasmtime async interoperability**: No existing example of driving a wasmtime async store
  from a smol executor was found. This combination is theoretically sound (both use `std::future::Future`)
  but should be prototyped with a minimal test before committing to the architecture.

- **Wasmtime binary size impact**: The ~12 MB estimate for Cranelift is derived from general
  knowledge of the bytecodealliance ecosystem. Actual link-time impact on the `wezterm-gui` binary
  on macOS (current primary development target) was not measured. Wasmtime provides a `winch` backend
  (a fast, non-optimizing single-pass compiler) that could reduce binary size at the cost of plugin
  execution speed — this tradeoff was not researched.

- **Component Model maturity in wasmtime v36 vs. v42**: The research found that v41 added "minimal
  support for fixed-length lists in the component model." It is unclear which component model features
  were present in v36 (the LTS). If the LTS lacks required WIT features, the project would need to
  use a more recent non-LTS release and accept the 2-month support window, or contribute patches
  upstream to the LTS.

- **cargo-deny license check**: `wasmtime` is Apache 2.0. ArcTerm is MIT. The existing `deny.toml`
  license configuration was not inspected to confirm whether Apache 2.0 dependencies are allowed.
  This should be verified before adding wasmtime to the workspace.
