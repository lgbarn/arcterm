# Architecture

## Overview

Arcterm is a GPU-accelerated, AI-native terminal emulator built as a Cargo workspace of six focused crates. The architecture follows a strict layered decomposition: shared types at the bottom, protocol/I/O layers above, a GPU rendering layer beside them, an extensibility layer (WASM plugins), and a single application crate at the top that wires everything together using a `winit` event loop and a `tokio` multi-thread runtime. A custom OSC escape-sequence protocol (`OSC 7770`) enables structured AI↔terminal communication without breaking standard terminal compatibility.

---

## Findings

### Overarching Pattern: Layered Crate Decomposition

The project is a Cargo workspace with six member crates arranged into a strict dependency DAG. Higher layers depend on lower layers; no crate imports a peer at the same level or a crate above it.

- Evidence: `Cargo.toml:2-9` — workspace members listed bottom-up: `arcterm-core`, `arcterm-vt`, `arcterm-pty`, `arcterm-render`, `arcterm-app`, `arcterm-plugin`.

```
┌─────────────────────────────────────────────────────────────────┐
│                         arcterm-app                             │  ← binary, event loop, AppState
│  (winit + tokio + wgpu + clap + all workspace crates)          │
└────────┬──────────┬───────────┬──────────────┬─────────────────┘
         │          │           │              │
    ┌────▼────┐ ┌───▼────┐ ┌───▼──────┐ ┌────▼──────────────────┐
    │arcterm- │ │arcterm-│ │arcterm-  │ │   arcterm-plugin       │
    │   vt    │ │  pty   │ │  render  │ │  (wasmtime + WIT)      │
    └────┬────┘ └───┬────┘ └──────────┘ └───────────────────────┘
         │          │
    ┌────▼──────────▼────────────────────┐
    │            arcterm-core            │  ← shared types only
    │   (Cell, Grid, InputEvent, etc.)   │
    └────────────────────────────────────┘
```

- Evidence: `arcterm-vt/Cargo.toml`, `arcterm-pty/Cargo.toml`, `arcterm-render/Cargo.toml` — each imports `arcterm-core` but not each other.

---

### Layer 1 — Shared Types (`arcterm-core`)

Provides the in-memory terminal model with no I/O or rendering dependencies.

- **`Cell` / `CellAttrs` / `Color`**: single character cell with SGR attributes.
  - Evidence: `arcterm-core/src/cell.rs`
- **`Grid` / `GridSize` / `CursorPos` / `TermModes`**: 2D grid of cells, scrollback buffer (`VecDeque`), terminal mode flags (alt-screen, mouse reporting, bracketed paste, etc.).
  - Evidence: `arcterm-core/src/grid.rs:1-50`
- **`InputEvent` / `KeyCode` / `Modifiers`**: platform-agnostic input event types.
  - Evidence: `arcterm-core/src/input.rs`, `arcterm-core/src/lib.rs`

---

### Layer 2a — VT Parser (`arcterm-vt`)

Translates raw PTY bytes into `Grid` mutations.

- **`Processor` / `ApcScanner`**: wraps the `vte` crate VT parser; `ApcScanner` routes APC (Application Program Command) sequences — used by the Kitty graphics protocol — to a side-channel while passing everything else through the standard VT handler.
  - Evidence: `arcterm-vt/src/processor.rs`, `arcterm-vt/src/lib.rs:9`
- **`Handler` / `GridState`**: implements the `vte::Perform` trait; `GridState` holds the live `Grid`, scroll bounds, mode state, and side-channel queues (Kitty payloads, OSC 7770 blocks, tool queries, exit codes).
  - Evidence: `arcterm-vt/src/handler.rs` (exported from `arcterm-vt/src/lib.rs:8`)
- **`KittyChunkAssembler` / `KittyCommand`**: reassembles multi-chunk Kitty graphics APC payloads into complete image blobs.
  - Evidence: `arcterm-vt/src/kitty.rs`, `arcterm-vt/src/lib.rs:9`
- **OSC 7770 structured protocol**: custom escape sequence carrying structured JSON blocks (context queries, tool calls, error blocks). Parsed in `GridState` and surfaced to the app layer via `take_*` drain methods.
  - Evidence: `arcterm-vt/src/handler.rs` (re-exported as `ContentType`, `StructuredContentAccumulator`)

---

### Layer 2b — PTY I/O (`arcterm-pty`)

Manages shell spawning and bidirectional byte I/O.

- **`PtySession`**: wraps `portable-pty`'s `NativePtySystem`. Spawns the child shell (resolving `$SHELL` → `/bin/bash` fallback), sets `TERM=xterm-256color`, captures the child PID, and drops the slave end to propagate EOF correctly.
  - Evidence: `arcterm-pty/src/session.rs:86-196`
- **Async read loop**: a dedicated OS thread performs blocking reads from the PTY master into a 16 KiB buffer and sends chunks via `tokio::sync::mpsc::channel(64)` to the application layer. The receiver is returned from `PtySession::new()`, deliberately kept separate so `AppState` owns it.
  - Evidence: `arcterm-pty/src/session.rs:167-185`
- **CWD introspection**: platform-conditional implementations — `proc_pidinfo` on macOS, `/proc/<pid>/cwd` symlink on Linux.
  - Evidence: `arcterm-pty/src/session.rs:38-83`

---

### Layer 3 — GPU Renderer (`arcterm-render`)

GPU rendering via `wgpu` + `glyphon` text shaping, with no knowledge of PTY or VT state.

- **`GpuState`**: wgpu device/queue/surface/config initialization tied to a `winit::Window`.
  - Evidence: `arcterm-render/src/gpu.rs`, `arcterm-render/src/lib.rs:11`
- **`QuadRenderer`**: instanced colored-rectangle pipeline (borders, backgrounds, selection quads, overlays). Rendered via `wgpu` render pass.
  - Evidence: `arcterm-render/src/quad.rs`, `arcterm-render/src/lib.rs:14`
- **`TextRenderer` / `ClipRect`**: glyphon-backed glyph atlas for terminal text. Accepts cell data, applies clip rectangles per-pane.
  - Evidence: `arcterm-render/src/text.rs`, `arcterm-render/src/lib.rs:19`
- **`ImageQuadRenderer` / `ImageTexture`**: textured quad pipeline for Kitty inline graphics. Images are uploaded to GPU and cached by `image_id` in `Renderer.image_store`.
  - Evidence: `arcterm-render/src/image_quad.rs`, `arcterm-render/src/renderer.rs:71-73`
- **`HighlightEngine` / `StructuredBlock` / `RenderedLine`**: syntect-powered syntax highlighting for structured content blocks (code output, AI responses).
  - Evidence: `arcterm-render/src/structured.rs`, `arcterm-render/src/lib.rs:20`
- **`Renderer`**: top-level composition — owns `GpuState`, `TextRenderer`, `QuadRenderer`, `ImageQuadRenderer`, and the active `RenderPalette`. Accepts `PaneRenderInfo` (terminal grid + rect) and `PluginPaneRenderInfo` (WASM-rendered lines + rect).
  - Evidence: `arcterm-render/src/renderer.rs:60-100`
- **Color pipeline**: sRGB-to-linear conversion applied before passing colors to wgpu (fixed in commit `5a44897`).
  - Evidence: git log `5a44897 fix: convert sRGB to linear for wgpu color pipeline`

---

### Layer 4 — Plugin System (`arcterm-plugin`)

WASM component model extensibility via wasmtime.

- **WIT contract** (`arcterm-plugin/wit/arcterm.wit`): defines the `arcterm-plugin` world. Host imports: `log`, `render-text`, `subscribe-event`, `get-config`, `register-mcp-tool`. Guest exports: `load`, `update(event) → bool`, `render() → list<styled-line>`.
  - Evidence: `arcterm-plugin/wit/arcterm.wit:62-76`
- **`PluginHostData`**: per-plugin store-threaded state — WASI context (no filesystem/network by default), draw buffer, subscribed events, registered MCP tool schemas, config map, permission flags. Memory capped at 10 MB via `StoreLimitsBuilder`.
  - Evidence: `arcterm-plugin/src/host.rs:21-56`
- **Permission model**: `PaneAccess` enum (`None` / `Read` / `Write`) gates `render_text` calls; `permissions.ai` gates `register_mcp_tool` calls. Denied calls silently drop with a warning.
  - Evidence: `arcterm-plugin/src/host.rs:117-158`, `arcterm-plugin/src/manifest.rs:17-53`
- **`PluginManager`**: owns a `PluginRuntime` (wasmtime Engine + Linker), a `HashMap<PluginId, LoadedPlugin>`, and a `tokio::sync::broadcast::Sender<PluginEvent>` event bus (channel depth 256). On plugin load, spawns a per-plugin `tokio::task::spawn_blocking` loop that receives broadcast events and calls `update()` + `render()`.
  - Evidence: `arcterm-plugin/src/manager.rs:138-271`
- **Event bus**: lifecycle events (`PaneOpened`, `PaneClosed`, `CommandExecuted`, `WorkspaceSwitched`) broadcast to all plugins. Key-input events bypass the bus and are delivered directly to the focused plugin pane via `send_key_input()`.
  - Evidence: `arcterm-plugin/src/manager.rs:39-92`, `arcterm-plugin/src/manager.rs:388-446`
- **Draw buffer**: `Arc<Mutex<Vec<StyledLine>>>` written by the event task after `render()` returns, read by the main thread during `RedrawRequested` via `take_draw_buffer()`.
  - Evidence: `arcterm-plugin/src/manager.rs:108-112`, `arcterm-plugin/src/manager.rs:330-347`
- **MCP tool registry**: plugins register `ToolSchema` records via `register_mcp_tool`. `PluginManager::list_tools()` aggregates all registered schemas; `call_tool()` returns a stub JSON (full WASM invocation deferred to Phase 8).
  - Evidence: `arcterm-plugin/src/manager.rs:349-386`

---

### Layer 5 — Application (`arcterm-app`)

Single binary; wires all crates into a `winit` event loop.

#### Entry Point & Initialization Flow

1. `main()` parses CLI via `clap` derive — subcommands handled before event loop.
2. Non-GUI subcommands (`plugin install/list/remove`, `config flatten`, `list`) exit early.
3. Tokio multi-thread runtime created: `tokio::runtime::Builder::new_multi_thread()`.
4. `winit::EventLoop::run_app()` drives the `App` struct (implements `ApplicationHandler`).
5. On `Resumed` (first window creation): `AppState::new()` initializes renderer, panes, plugin manager, plan watcher, config file-system watcher.
6. On `AboutToWait`: polls PTY channels (non-blocking `try_recv`), processes PTY output through `Terminal::process_pty_output()`, triggers `RedrawRequested`.
7. On `RedrawRequested`: calls `Renderer::render_frame()` with current pane data, plugin draw buffers, overlay state.

  - Evidence: `arcterm-app/src/main.rs:370-531`

#### `AppState` — Central Controller

`AppState` owns all mutable application state (fields documented inline at `arcterm-app/src/main.rs:537-654`):

| Field group | Key fields |
|---|---|
| Multiplexer | `panes: HashMap<PaneId, Terminal>`, `pty_channels`, `tab_manager`, `tab_layouts: Vec<PaneNode>` |
| Input | `keymap: KeymapHandler`, `modifiers`, `selection`, `clipboard` |
| Rendering | `renderer: Renderer`, `highlight_engine`, `structured_blocks`, `palette_mode`, `overlay_review` |
| Config | `config: ArctermConfig`, `config_rx` (fs-watcher channel) |
| AI features | `ai_states`, `pane_contexts`, `last_ai_pane`, `pending_errors` |
| Plugin system | `plugin_manager: Option<PluginManager>`, `plugin_event_tx` |
| Plan strip | `plan_strip`, `plan_view`, `plan_watcher`, `plan_watcher_rx` |

#### `Terminal` — PTY+VT Integration

Thin wrapper in `arcterm-app/src/terminal.rs` that composes `PtySession` + `ApcScanner` + `GridState` + `KittyChunkAssembler`. Exposes drain methods (`take_pending_replies`, `take_pending_images`, `take_tool_queries`, `take_context_queries`, `take_exit_codes`) that the app layer polls after each PTY batch.

- Evidence: `arcterm-app/src/terminal.rs:26-233`

#### Pane Layout Engine (`arcterm-app/src/layout.rs`)

`PaneNode` is a recursive binary tree (`Leaf | PluginPane | HSplit | VSplit`) with a `ratio: f32` at each split node. Key operations:
- `compute_rects()` — recursive pixel-rect computation
- `split()` / `close()` — tree mutation (sibling promotion on close)
- `focus_in_direction()` — spatial navigation using centre-point geometry
- `compute_border_quads()` — generates colored `BorderQuad` instances for interior edges
- `compute_zoomed_rect()` — single-pane zoom (all others get zero-sized rect)

Per-tab layout stored as `tab_layouts: Vec<PaneNode>` in `AppState`. `TabManager` tracks the active tab index and per-tab focused `PaneId`.

- Evidence: `arcterm-app/src/layout.rs:100-644`

#### AI Integration Protocol (OSC 7770)

A proprietary escape sequence protocol enabling bidirectional structured communication between AI agents and the terminal:

- **`context/query`**: AI pane requests sibling pane context (CWD, last command, exit code). Response: JSON array via `format_context_osc7770()`.
- **`tools/list`**: AI pane requests MCP tool schemas from loaded plugins. Response: base64-encoded JSON.
- **`tools/call`**: AI pane invokes an MCP tool by name with JSON args.
- **`start/end` blocks**: structured content (errors, AI responses) injected into pane PTY input via `format_error_osc7770()`.

  - Evidence: `arcterm-app/src/context.rs:130-222`, `arcterm-app/src/terminal.rs:128-163`

#### AI Detection (`arcterm-app/src/ai_detect.rs`)

Heuristic process-name matching (reading `comm` via `proc_pidinfo` / `/proc`). Recognizes Claude Code (`claude`), Codex CLI (`codex`), Gemini CLI (`gemini`), Aider (Python interpreter + arg check), Cursor, Copilot. Results cached per-pane with a 5-second TTL.

- Evidence: `arcterm-app/src/ai_detect.rs:37-47`

---

### Data Flow: PTY Output → Screen

```
Shell process (child)
       │  raw bytes (16 KiB chunks)
       ▼
  OS thread "pty-reader"
       │  tokio::mpsc::channel(64)
       ▼
AppState::about_to_wait()  [non-blocking try_recv]
       │  &[u8]
       ▼
Terminal::process_pty_output()
       │
       ├─ ApcScanner::advance() → GridState (VT handler)
       │      ├─ Grid mutations (cells, cursor, scroll)
       │      └─ Side queues: Kitty payloads, OSC 7770 blocks, tool queries
       │
       └─ KittyChunkAssembler → image::load_from_memory → PendingImage
              │
              ▼
AppState::drain terminal queues
  (take_pending_images, take_tool_queries, take_context_queries, take_exit_codes)
       │
       ▼
winit RedrawRequested
       │
       ▼
Renderer::render_frame()
  ├─ QuadRenderer  — background, borders, selection, overlays
  ├─ TextRenderer  — terminal cells (glyphon glyph atlas)
  ├─ ImageQuadRenderer — Kitty inline images (GPU texture cache)
  └─ StructuredBlock highlighting (syntect)
       │
       ▼
wgpu surface.present()
```

---

### Data Flow: Plugin Event → Render

```
AppState detects event (pane open, workspace switch, etc.)
       │
       ▼
plugin_manager.broadcast_event(PluginEvent)
       │  tokio::sync::broadcast channel
       ▼
Per-plugin tokio task (spawn_blocking)
       │
       ├─ PluginInstance::call_update(WitPluginEvent) → bool
       └─ if true: PluginInstance::call_render() → Vec<StyledLine>
              │  write to Arc<Mutex<DrawBuffer>>
              ▼
AppState::RedrawRequested
  PluginManager::take_draw_buffer(id) → Vec<StyledLine>
       │
       ▼
Renderer → PluginPaneRenderInfo → TextRenderer
```

---

### Session / Workspace Persistence

- Workspace files: TOML at `~/.config/arcterm/workspaces/<name>.toml`. Schema: `[workspace]`, `[layout]` (recursive `type = "hsplit" | "vsplit" | "leaf"`), `[environment]`.
- Session auto-save: on `CloseRequested`, writes `_last_session.toml`; on next launch with no args, restores and deletes it.
- `WorkspaceFile` / `WorkspacePaneNode` parsed in `arcterm-app/src/workspace.rs`, converted to `PaneNode` via `to_pane_tree()`.
  - Evidence: `arcterm-app/src/main.rs:380-509`, `arcterm-app/src/workspace.rs` [Inferred from usage in main.rs]

---

### Async / Threading Model

| Thread / Task | Role |
|---|---|
| Main OS thread | `winit` event loop + `AppState` mutation + wgpu render |
| `pty-reader` OS thread (one per pane) | Blocking `read()` from PTY master → `mpsc::send` |
| Tokio thread pool | Plugin event listener tasks (`spawn_blocking` for wasmtime calls) |
| Config watcher thread | `notify` fs-watcher, sends on `std::sync::mpsc` |
| Plan watcher thread | `notify` fs-watcher for `.shipyard/` / `PLAN.md` |

The main event loop is intentionally synchronous with respect to rendering. PTY data is collected via non-blocking `try_recv` in `about_to_wait`, preventing the render loop from blocking on I/O.

- Evidence: `arcterm-pty/src/session.rs:167-185` (reader thread), `arcterm-plugin/src/manager.rs:466-535` (event tasks), `arcterm-app/src/main.rs:515-519` (tokio runtime)

---

### Key Abstractions and Interfaces

| Abstraction | Location | Role |
|---|---|---|
| `Cell` | `arcterm-core/src/cell.rs` | Atomic unit of the terminal grid |
| `Grid` | `arcterm-core/src/grid.rs` | 2D cell array + scrollback VecDeque |
| `PtySession` | `arcterm-pty/src/session.rs` | Shell lifetime + I/O + CWD |
| `ApcScanner` / `GridState` | `arcterm-vt/src/processor.rs`, `handler.rs` | VT state machine |
| `Terminal` | `arcterm-app/src/terminal.rs` | Composes PTY + VT + Grid |
| `PaneNode` | `arcterm-app/src/layout.rs` | Recursive binary layout tree |
| `AppState` | `arcterm-app/src/main.rs:537` | Central mutable application state |
| `Renderer` | `arcterm-render/src/renderer.rs` | GPU pipeline composition |
| `PluginManager` | `arcterm-plugin/src/manager.rs` | Plugin lifecycle + event bus |
| `PluginHostData` | `arcterm-plugin/src/host.rs` | Per-plugin WASM store data |

---

## Summary Table

| Item | Detail | Confidence |
|---|---|---|
| Architectural pattern | Layered crate decomposition (strict DAG) | Observed |
| Primary event loop | `winit::ApplicationHandler` on main OS thread | Observed |
| Async runtime | `tokio` multi-thread, entered from main | Observed |
| PTY I/O model | OS thread per pane → `mpsc` channel → non-blocking drain | Observed |
| Plugin runtime | `wasmtime` component model, WIT interface | Observed |
| Plugin sandboxing | WASI no-fs/no-net by default, 10 MB memory cap | Observed |
| AI protocol | OSC 7770 custom escape sequences | Observed |
| Layout engine | Recursive binary pane tree with ratio splits | Observed |
| Session persistence | TOML workspace files + `_last_session.toml` auto-save | Observed |
| Color space | sRGB-to-linear conversion before wgpu | Observed |
| Syntax highlighting | `syntect` for structured content blocks | Observed |
| Image rendering | Kitty graphics protocol → GPU texture cache | Observed |

---

## Open Questions

- How is `AppState` initialized (the `new()` constructor body) — specifically, what order plugins, config, and plan watchers are set up. File too large to read fully (`arcterm-app/src/main.rs` is 39,304 tokens).
- `workspace.rs` `to_pane_tree()` conversion logic — inferred from usage in `main.rs` but not directly read.
- Whether `arcterm-render` can be driven independently (the `arcterm-render/examples/window.rs` standalone example suggests yes — [Inferred]).
- Full OSC 133 shell integration surface (prompt start/end, command boundaries) — referenced in code comments but handler details not fully read.
