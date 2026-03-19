# ARCHITECTURE.md

## Overview

ArcTerm is a fork of WezTerm structured as a Rust workspace. Its architecture is a **layered GUI terminal emulator**: a native windowing layer drives a GPU renderer, which displays the output of an in-process terminal model fed by a multiplexer (mux) that manages PTY subprocesses. The mux layer also exposes a Unix socket server enabling multi-client attach/detach sessions. ArcTerm-specific extensions (WASM plugins, AI integration) are planned as additive crates above the mux layer.

---

## Findings

### High-Level Component Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│  wezterm-gui  (binary: wezterm-gui)                             │
│  ┌──────────┐  ┌──────────────┐  ┌───────────────────────────┐  │
│  │ frontend │  │  TermWindow  │  │  Renderer (Glium / WebGPU)│  │
│  │GuiFrontEnd│ │ (event loop) │  │  GlyphCache, ShapeCache   │  │
│  └────┬─────┘  └──────┬───────┘  └──────────────┬────────────┘  │
└───────│───────────────│─────────────────────────│───────────────┘
        │ MuxNotification│                          │ quads/vertices
        ▼                ▼                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  mux  (crate)                                                   │
│  ┌──────┐  ┌──────┐  ┌────────┐  ┌──────────────────────────┐  │
│  │  Mux │  │ Pane │  │  Tab   │  │  Domain (Local/SSH/Client)│  │
│  │      │  │      │  │        │  │  LocalDomain → PTY        │  │
│  └──────┘  └──┬───┘  └────────┘  └──────────────────────────┘  │
│               │ reader thread                                    │
└───────────────│─────────────────────────────────────────────────┘
                │ raw PTY bytes
                ▼
┌─────────────────────────────────────────────────────────────────┐
│  term  (crate: wezterm-term)                                    │
│  Terminal model: Screen, Scrollback, CellAttributes             │
│  Escape sequence state machine (via vtparse / termwiz parser)   │
└─────────────────────────────────────────────────────────────────┘
                │ TerminalState::perform_actions
                ▼
┌─────────────────────────────────────────────────────────────────┐
│  pty  (crate: portable-pty) + wezterm-ssh                       │
│  Platform PTY: Unix openpty / Windows ConPTY                    │
│  SSH PTY: libssh-rs or ssh2                                     │
└─────────────────────────────────────────────────────────────────┘
```

### Layer Boundaries

- **GUI layer** (`wezterm-gui`): window management, key/mouse event translation, font rendering, GPU compositing. Depends on `mux`, `window`, `wezterm-font`.
  - Evidence: `wezterm-gui/src/main.rs` (imports `mux`, `window`, `wezterm-font`)

- **Mux layer** (`mux`): multiplexing abstraction over PTYs. Owns all live tabs, panes, windows, and workspace state. Communicates upward via the `MuxNotification` pub/sub system.
  - Evidence: `mux/src/lib.rs` lines 57-98 (`MuxNotification` enum) and lines 692-699 (`Mux::subscribe`)

- **Terminal model layer** (`term`): pure VT state machine. No GUI dependency. The `Terminal` struct holds `Screen`, cursor state, and scrollback.
  - Evidence: `term/src/lib.rs` lines 1-17 (doc comment explicitly states "no gui, nor does it directly manage a PTY")

- **PTY layer** (`pty`/`portable-pty`): cross-platform pseudoterminal allocation. Used by `LocalDomain` in `mux/src/domain.rs`.

- **Window / OS layer** (`window`): cross-platform native window creation, event loop, OpenGL/WebGPU surface. Backends in `window/src/os/{macos,x11,wayland,windows}`.
  - Evidence: `window/src/os/` directory listing

### Data Flow: Keystroke to Terminal Output

1. **Native event** arrives at the platform window loop (e.g., `NSEvent` on macOS, `xcb_key_press_event_t` on X11).
2. **`window` crate** translates it to `wezterm_input_types::KeyEvent` and delivers it to `TermWindow` via `WindowOps`.
3. **`TermWindow::key_event`** (in `wezterm-gui/src/termwindow/keyevent.rs`) consults the `InputMap` / `KeyTableState` stack to resolve a `KeyAssignment` or raw key sequence.
4. If raw input: the encoded bytes are written to **`LocalPane::writer`**, which is the master side of the PTY via `portable_pty::MasterPty::write`.
5. The **child shell** receives the bytes, produces output, which the PTY returns as bytes on the master read side.
6. **`read_from_pane_pty`** thread (in `mux/src/lib.rs` lines 279-364) reads raw bytes from the PTY master and writes them to one side of a `socketpair`.
7. A second thread, **`parse_buffered_data`** (lines 140-243), reads from the socket, feeds bytes to `termwiz::escape::parser::Parser`, and dispatches parsed `Action` values via `send_actions_to_mux`.
8. `send_actions_to_mux` calls **`pane.perform_actions`**, which applies the actions to the `wezterm_term::Terminal` state machine, then fires `MuxNotification::PaneOutput`.

### Data Flow: Terminal Output to Pixels

1. `MuxNotification::PaneOutput(pane_id)` is received by `GuiFrontEnd`'s mux subscriber.
2. The subscriber calls `promise::spawn::spawn_into_main_thread` to schedule a window invalidation.
   - Evidence: `wezterm-gui/src/frontend.rs` lines 52-80
3. The **`window` crate** platform message loop delivers the repaint request to `TermWindow::paint_impl`.
4. **`paint_impl`** (in `wezterm-gui/src/termwindow/render/paint.rs` lines 17-60) calls `paint_pass()` in a retry loop (to handle texture atlas growth).
5. **`paint_pane`** (in `wezterm-gui/src/termwindow/render/pane.rs` line 32) iterates over the visible lines of the pane.
6. **`render_screen_line`** (in `wezterm-gui/src/termwindow/render/screen_line.rs` line 26) maps each cell to a quad: looks up glyph textures from `GlyphCache`, resolves colors, and writes vertices into the `TripleLayerQuadAllocator`.
7. The quad buffer is submitted to **Glium** (OpenGL) or **WebGPU** (wgpu) for GPU compositing.
   - Evidence: `wezterm-gui/src/renderstate.rs` lines 22-31 (`RenderContext` enum with `Glium` and `WebGpu` variants)

### IPC / RPC Between Components

- **Unix socket mux server**: on startup, `wezterm-gui` spawns a `LocalListener` thread that accepts connections on a Unix socket at `$RUNTIME_DIR/gui-sock-<pid>`.
  - Evidence: `wezterm-gui/src/main.rs` lines 651-674 (`spawn_mux_server`) and `wezterm-mux-server-impl/src/local.rs` lines 1-39
- **PDU codec**: all mux RPC messages are encoded as length-prefixed PDUs using LEB128 variable-length integers and serde serialization over the socket.
  - Evidence: `codec/src/lib.rs` header comment ("encode and decode the frames for the mux protocol")
- **`wezterm-client`**: client-side logic for connecting to a running GUI instance; used when `wezterm start` detects an existing socket via `wezterm_client::discovery::resolve_gui_sock_path`.
  - Evidence: `wezterm-gui/src/main.rs` lines 520-525
- **`SessionHandler`** (`wezterm-mux-server-impl/src/sessionhandler.rs`): server-side handler per connected client; tracks per-pane cursor state, seqno, and pane render changes, sends `GetPaneRenderChangesResponse` PDUs to remote clients.

### Async Runtime

ArcTerm does **not** use Tokio or a global async executor in the GUI path. Instead, it uses a custom scheduler integrated with the native GUI event loop:

- `promise::spawn::set_schedulers` registers two callbacks (high- and low-priority) that feed the platform's `SpawnQueue`.
  - Evidence: `promise/src/spawn.rs` lines 46-50 and comment on lines 39-44 ("Why this and not 'just tokio'?")
- `window/src/spawn.rs` implements `SpawnQueue` per-platform (macOS: CF RunLoop, X11/Wayland: a pipe-based wakeup, Windows: a Win32 event handle).
  - Evidence: `window/src/spawn.rs` lines 24-35
- Background work (PTY reading, SSH, async_executor tasks): uses `smol` / `async_executor` for non-GUI threads.
  - Evidence: `wezterm-gui/src/termwindow/mod.rs` line 54 (`use smol::channel::Sender`) and `wezterm-mux-server-impl/src/sessionhandler.rs` (smol io usage)
- `promise::spawn_into_main_thread` posts a closure into the `SpawnQueue`, which is drained inside the platform message loop tick.

### Event Loop Architecture

```
main thread (GUI thread)
    │
    ▼
Connection::run_message_loop()   ← platform native loop
    │
    ├─ drain SpawnQueue (high-pri first, then low-pri)
    ├─ handle native window events → TermWindow callbacks
    └─ trigger repaints → TermWindow::paint_impl
         │
         └─ submits quads to GPU
```

Background threads (non-GUI):
- One thread per pane: `read_from_pane_pty` (blocking PTY reader)
- One thread per pane: `parse_buffered_data` (escape sequence parser)
- One thread: `LocalListener::run` (Unix socket acceptor)
- SSH sessions: async tasks on a `smol` executor

### Cross-Platform Abstraction Layers

| Concern | Abstraction | Platform Impls |
|---------|-------------|----------------|
| Window / event loop | `window::Connection` + `ConnectionOps` trait | `window/src/os/macos/`, `x11/`, `wayland/`, `windows/` |
| PTY allocation | `portable_pty::PtySystem` trait | Unix `openpty`, Windows ConPTY |
| GPU rendering | `RenderContext` enum in `wezterm-gui/src/renderstate.rs` | Glium (OpenGL), WebGPU (wgpu) |
| Font loading | `wezterm-font::FontConfiguration` | FreeType, CoreText, DirectWrite |
| SSH | `wezterm-ssh::Session` | `ssh2` (libssh2), `libssh-rs` |

### Extension Points for New Features

The following locations are the minimal-conflict integration points for planned ArcTerm features:

1. **AI Integration (`arcterm-ai` crate)**
   - [Inferred] Best attached as a `MuxNotification` subscriber in `mux/src/lib.rs` or as a new `Pane` trait implementation. `MuxNotification::PaneOutput` provides a stream of parsed terminal output at the right abstraction level.
   - The `wezterm-gui/src/scripting/` module provides a Lua scripting bridge; an AI crate could expose itself there similarly to existing `lua-api-crates/`.

2. **WASM Plugin System (`arcterm-wasm-plugin` crate)**
   - [Inferred] Should integrate at the same level as Lua API crates in `lua-api-crates/`. The `config/src/lua.rs` `add_context_setup_func` mechanism (called at `wezterm-gui/src/main.rs` lines 1205-1207) is the registration path.

3. **Structured Output / OSC 7770**
   - The escape sequence parser in `mux/src/lib.rs` (`parse_buffered_data`, line 160) calls `parser.parse(...)`. New OSC sequences can be handled by extending `termwiz::escape::parser::Parser` or intercepting at the `Action` dispatch level in `term`.

4. **New Domain types** (e.g., WASM sandbox domain)
   - Implement the `Domain` trait (`mux/src/domain.rs` lines 49-80) and register via `Mux::add_domain`. This is already the pattern for `LocalDomain`, `RemoteSshDomain`, `ClientDomain`.

---

## Summary Table

| Aspect | Detail | Confidence |
|--------|--------|------------|
| Architectural pattern | Layered monolith (GUI → mux → term → PTY) | Observed |
| Async executor (GUI) | Custom `SpawnQueue` integrated with native event loop (not Tokio) | Observed |
| Async executor (background) | `smol` / `async-executor` | Observed |
| GPU backends | Glium (OpenGL) and WebGPU (wgpu), selected via `RenderContext` enum | Observed |
| IPC protocol | Length-prefixed LEB128 PDUs over Unix socket | Observed |
| Plugin/scripting system | Lua via `mlua`, registered through `add_context_setup_func` | Observed |
| PTY threading model | 2 threads per pane (reader + parser), plus GUI main thread | Observed |
| Cross-platform window backends | macOS (Cocoa/CF), X11, Wayland, Windows (Win32) | Observed |
| ArcTerm-specific code today | Rebrand strings only; no new crates yet | Observed |

## Open Questions

- The `arcterm-wasm-plugin` and `arcterm-ai` crates mentioned in `CLAUDE.md` do not yet exist in the repository. It is unclear what interface contracts they will expose.
- The `FrontEndSelection` config option (`config/src/config.rs`) implies an alternate frontend path; its full extent is not traced here.
- WebGPU (`wgpu`) is present as a render backend but its completeness relative to Glium is not verified from code inspection alone.
