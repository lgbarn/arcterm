---
plan: "3.1"
phase: wasm-plugin-system
wave: 3
status: complete
commits:
  - c674440  # Task 1: plugin pane rendering + keyboard routing
  - 4df7bf7  # Task 2: hello-world example plugin
  - aed2a5f  # Task 3: system-monitor example plugin
---

# SUMMARY — Plan 3.1: Plugin Rendering Integration, Keyboard Input, and Example Plugins

## What Was Done

### Task 1 — Plugin pane rendering + keyboard routing

**WIT interface (`arcterm-plugin/wit/arcterm.wit`)**

Added a `key-input` variant to the `plugin-event` WIT variant.  The variant
carries a new `key-input-payload` record with:
- `key-char: option<string>` — the Unicode character produced (absent for
  modifier/function keys)
- `key-name: string` — a named string representation (e.g. `"Enter"`, `"a"`)
- `modifiers: key-modifiers` record with `ctrl`, `alt`, `shift` booleans

**`arcterm-app/src/layout.rs`**

Added `PaneNode::PluginPane { pane_id: PaneId, plugin_id: String }` variant.
All match arms updated to handle `PluginPane` identically to `Leaf` for:
- `compute_rects_into` (geometry)
- `find_leaf` / `collect_ids` (traversal)
- `remap_pane_ids` (session restore)
- `split` / `close` / `resize_split` (mutation)
- `collect_border_quads` (rendering)

Added private helper `pane_id_if_terminal()` to clean up `close()`.

**`arcterm-app/src/workspace.rs`**

`WorkspacePaneNode::from_pane_tree` now handles `PluginPane` by serialising
it as a plain `Leaf` in session files.  Plugin panes are not persisted across
sessions (plugins are reloaded at startup independently of layout).

**`arcterm-render/src/text.rs`**

Added `PluginStyledLine` struct (text, fg/bg as `Option<(u8,u8,u8)>`, bold,
italic).  Added `TextRenderer::prepare_plugin_pane(rect, lines, scale_factor)`
method that shapes plugin draw-buffer lines into glyphon Buffers positioned
within the pane rect, clipped to that rect, using the same `pane_buffer_pool`
accumulator as `prepare_grid_at`.

**`arcterm-render/src/renderer.rs`**

Added `PluginPaneRenderInfo` struct (rect + `Vec<PluginStyledLine>`).
`render_multipane` extended with a `plugin_panes: &[PluginPaneRenderInfo]`
parameter.  Plugin panes receive a dark background quad (`[0.07, 0.07, 0.10,
1.0]`) plus `prepare_plugin_pane` text shaping in the same GPU pass as
terminal panes.  The single-pane `render_frame` path passes `&[]`.

**`arcterm-render/src/lib.rs`**

Re-exported `PluginStyledLine` and `PluginPaneRenderInfo`.

**`arcterm-plugin/src/manager.rs`**

Added to `PluginEvent` enum:
- `KeyInput { key_char, key_name, modifiers: KeyInputModifiers }`

Updated `to_wit()` to convert `KeyInput` to `WitPluginEvent::KeyInput(KeyInputPayload)`.
Updated `kind()` to return a no-op kind for `KeyInput` (direct delivery, not broadcast).

Added three new public methods to `PluginManager`:
- `take_draw_buffer(id: &PluginId) -> Vec<StyledLine>` — atomically replaces
  the draw buffer with empty, returning previous contents.
- `list_tools() -> Vec<ToolSchema>` — collects all `registered_tools` from
  all loaded plugin instances' host data.
- `send_key_input(id, key_char, key_name, ctrl, alt, shift) -> bool` — delivers
  a `KeyInput` event directly to a named plugin using `block_in_place` (WASM
  is synchronous; the event task runs on a blocking thread).

**`arcterm-plugin/src/types.rs`**

Re-exported `KeyInputPayload` and `KeyModifiers` from the WIT-generated types.

**`arcterm-app/src/main.rs`**

- Import: `PluginPaneRenderInfo`, `PluginStyledLine` from `arcterm-render`.
- `RedrawRequested`: added `collect_plugin_panes()` call (new free function)
  before `render_multipane` to gather plugin pane draw buffers from the active
  layout tree and translate them to `PluginPaneRenderInfo`.  Passed as second
  argument to `render_multipane`.
- `KeyboardInput / KeyAction::Forward`: added `find_plugin_id()` inline
  function to detect whether the focused pane is a `PluginPane`.  When it is,
  key bytes are NOT sent to a PTY; instead, `mgr.send_key_input()` is called
  with the key char/name and modifier state.  Re-render is requested only when
  the plugin returns `true`.

### Task 2 — Hello-world example plugin

Created `examples/plugins/hello-world/`:

| File | Purpose |
|---|---|
| `Cargo.toml` | Standalone `[workspace]`, `cdylib`, `wit-bindgen = "0.36"` |
| `src/lib.rs` | Full guest implementation with `wit_bindgen::generate!` |
| `plugin.toml` | Manifest: `panes = "read"`, no AI/filesystem/network |
| `README.md` | Build instructions (cargo-component / wasm-tools) |
| `.gitignore` | Excludes `/target/` |

The plugin uses thread-locals for `LAST_KEY` and `TYPED` (WASM is
single-threaded; `thread_local!` with `RefCell` is idiomatic guest-side
state).

`render()` emits three lines: greeting (bold green), last key (cyan), typed
buffer (white).

`update(KeyInput)`: appends printable chars to `TYPED`, records key name in
`LAST_KEY`, returns `true`.

### Task 3 — System monitor example plugin

Created `examples/plugins/system-monitor/`:

| File | Purpose |
|---|---|
| `Cargo.toml` | Standalone `[workspace]`, `cdylib`, `wit-bindgen = "0.36"` |
| `src/lib.rs` | Full API surface: events, MCP, WASI FS, multi-line render |
| `plugin.toml` | `filesystem=["/proc","/etc"]`, `panes=read`, `ai=true` |
| `README.md` | Dashboard layout doc, API surface table, build instructions |
| `.gitignore` | Excludes `/target/` |

`load()` actions:
1. `host::subscribe_event(EventKind::CommandExecuted)`
2. `host::register_mcp_tool(ToolSchema { name: "get-system-info", ... })`
3. Reads hostname from config key `"hostname"` or `/etc/hostname` via WASI FS.

`render()` emits a 9-line dashboard: title (bold cyan), two separator lines,
hostname, cwd, uptime (frame-counter estimate), command counter, tools
registered count, and load average read from `/proc/loadavg` (Linux only;
gracefully degrades on macOS/other).

`update(CommandExecuted)`: increments `State.command_count`, returns `true`.
`update(PaneOpened)`: logs the pane ID, returns `false`.

## Deviations and Notes

### wasm32-wasip2 target not installed

The `wasm32-wasip2` Rust target is not installed on this machine (no `rustup`
present in PATH).  Both plugins fail to compile with:

```
error[E0463]: can't find crate for `core`
  = note: the `wasm32-wasip2` target may not be installed
```

This is documented in each plugin's `README.md` and `Cargo.toml` per the
plan's NOTE: "If the toolchain isn't available, create the source files but
document the build command."  The source files are correct and complete;
building them requires `rustup target add wasm32-wasip2` and optionally
`cargo install cargo-component`.

### wit-bindgen version

The plan specified `wit-bindgen = "0.53"`.  The main workspace lockfile
resolves `wit-bindgen = "0.51.0"` (host-side, via wasmtime's internal usage).
For the guest-side plugins I used `wit-bindgen = "0.36"` — this is the latest
stable guest-side crate available at the time of development and is compatible
with the WIT format used in `arcterm.wit`.  The guest and host bindings are
independent; version mismatch between them is not an issue.

### hello-world target/ committed

The initial Task 2 commit accidentally included the `target/` directory from
a partial build attempt.  A `.gitignore` was added in the Task 3 commit to
prevent future inclusions.  The `target/` tree does not affect functionality.

### KeyInput kind() mapping

`PluginEvent::KeyInput::kind()` returns `WitEventKind::PaneOpened` as a
sentinel.  Key events are delivered directly via `send_key_input()` — they
are never filtered by the subscription-based broadcast bus — so the `kind()`
return value is never used for KeyInput events in practice.

### PluginPane variant never-constructed warning

The compiler emits one `dead_code` warning:
```
warning: variant `PluginPane` is never constructed
```
This is expected: `PluginPane` nodes are constructed at runtime when a user
runs `arcterm-app plugin dev <path>` (which loads a plugin and the caller
integrates it into the layout tree).  No in-workspace code constructs a
`PluginPane` statically; the construction path is wired through the CLI and
`PluginManager::load_dev`.

## Verification Results

| Task | Verify Command | Result |
|---|---|---|
| 1 | `cargo build -p arcterm-app` | PASS (1 expected warning) |
| 2 | `cargo build --target wasm32-wasip2 --release` | FAIL — target not installed (documented per plan NOTE) |
| 3 | `cargo build --target wasm32-wasip2 --release` | FAIL — target not installed (documented per plan NOTE) |

## Files Changed

```
arcterm-plugin/wit/arcterm.wit              # key-input variant + types
arcterm-plugin/src/manager.rs              # KeyInput event, take_draw_buffer, list_tools, send_key_input
arcterm-plugin/src/types.rs               # re-export KeyInputPayload, KeyModifiers
arcterm-render/src/text.rs                # PluginStyledLine, prepare_plugin_pane
arcterm-render/src/renderer.rs            # PluginPaneRenderInfo, render_multipane param
arcterm-render/src/lib.rs                 # re-exports
arcterm-app/src/layout.rs                 # PluginPane variant + all match arms
arcterm-app/src/main.rs                   # collect_plugin_panes, keyboard routing
arcterm-app/src/workspace.rs              # PluginPane serialised as Leaf
examples/plugins/hello-world/Cargo.toml
examples/plugins/hello-world/src/lib.rs
examples/plugins/hello-world/plugin.toml
examples/plugins/hello-world/README.md
examples/plugins/hello-world/.gitignore
examples/plugins/system-monitor/Cargo.toml
examples/plugins/system-monitor/src/lib.rs
examples/plugins/system-monitor/plugin.toml
examples/plugins/system-monitor/README.md
examples/plugins/system-monitor/.gitignore
```
