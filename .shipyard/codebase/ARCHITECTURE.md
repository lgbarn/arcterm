# ARCHITECTURE.md

## Overview

ArcTerm is a fork of WezTerm structured as a Rust workspace. Its architecture is a **layered GUI terminal emulator**: a native windowing layer drives a GPU renderer, which displays the output of an in-process terminal model fed by a multiplexer (mux) that manages PTY subprocesses. The mux layer also exposes a Unix socket server enabling multi-client attach/detach sessions. Two ArcTerm-specific crates (`arcterm-ai` and `arcterm-wasm-plugin`) now exist and are wired into `wezterm-gui` as a dependency layer above the mux layer, though their integration into the GUI event loop is partially complete.

---

## Findings

### High-Level Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│  wezterm-gui  (binary: wezterm-gui)                                     │
│  ┌──────────┐  ┌──────────────┐  ┌─────────────────────────────────┐   │
│  │ frontend │  │  TermWindow  │  │  Renderer (Glium / WebGPU)      │   │
│  │GuiFrontEnd│ │ (event loop) │  │  GlyphCache, ShapeCache         │   │
│  └────┬─────┘  └──────┬───────┘  └──────────────┬──────────────────┘   │
│       │               │                          │ quads/vertices        │
│  ┌────────────────────────────────────────────┐  │                       │
│  │  ArcTerm layers (in wezterm-gui)           │  │                       │
│  │  ┌─────────────┐  ┌───────────────────┐   │  │                       │
│  │  │  ai_pane.rs │  │ overlay/           │   │  │                       │
│  │  │ (AI split   │  │ ai_command_overlay │   │  │                       │
│  │  │  pane)      │  │ suggestion_overlay │   │  │                       │
│  │  └──────┬──────┘  └────────┬──────────┘   │  │                       │
│  └─────────│──────────────────│──────────────┘  │                       │
└────────────│──────────────────│─────────────────│───────────────────────┘
             │ arcterm_ai::     │ arcterm_ai::     │
             │ backend, prompts │ suggestions      │
             ▼                  ▼                  ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  arcterm-ai  (crate)                                                    │
│  ┌──────────┐ ┌─────────┐ ┌──────────┐ ┌──────────┐ ┌───────────────┐  │
│  │ backend  │ │ context │ │  agent   │ │  suggest │ │  destructive  │  │
│  │ (Ollama, │ │ (Pane   │ │ (multi-  │ │  ions    │ │  (command     │  │
│  │  Claude) │ │ Context)│ │  step)   │ │          │ │   detection)  │  │
│  └──────────┘ └─────────┘ └──────────┘ └──────────┘ └───────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│  arcterm-wasm-plugin  (crate)                                           │
│  ┌──────────┐ ┌─────────────┐ ┌───────────┐ ┌──────────┐ ┌──────────┐  │
│  │capability│ │  loader     │ │  lifecycle│ │host_api  │ │  event   │  │
│  │ (grants, │ │(wasmtime    │ │ (Plugin   │ │(linker   │ │  router  │  │
│  │  checks) │ │ Component)  │ │ Manager)  │ │ register)│ │          │  │
│  └──────────┘ └─────────────┘ └───────────┘ └──────────┘ └──────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
             ↕ (both depend on mux + config)
┌─────────────────────────────────────────────────────────────────────────┐
│  mux  (crate)                                                           │
│  ┌──────┐  ┌──────┐  ┌────────┐  ┌──────────────────────────────────┐  │
│  │  Mux │  │ Pane │  │  Tab   │  │  Domain (Local/SSH/Client)       │  │
│  │      │  │      │  │        │  │  LocalDomain → PTY               │  │
│  └──────┘  └──┬───┘  └────────┘  └──────────────────────────────────┘  │
│               │ reader thread                                            │
└───────────────│─────────────────────────────────────────────────────────┘
                │ raw PTY bytes
                ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  term  (crate: wezterm-term)                                            │
│  Terminal model: Screen, Scrollback, CellAttributes                     │
│  Escape sequence state machine (via vtparse / termwiz parser)           │
└─────────────────────────────────────────────────────────────────────────┘
                │ TerminalState::perform_actions
                ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  pty  (crate: portable-pty) + wezterm-ssh                               │
│  Platform PTY: Unix openpty / Windows ConPTY                            │
│  SSH PTY: libssh-rs or ssh2                                             │
└─────────────────────────────────────────────────────────────────────────┘
```

### Layer Boundaries

- **GUI layer** (`wezterm-gui`): window management, key/mouse event translation, font rendering, GPU compositing. Depends on `mux`, `window`, `wezterm-font`, `arcterm-ai`, `arcterm-wasm-plugin`.
  - Evidence: `wezterm-gui/Cargo.toml` lines 38 and 99 (`arcterm-ai` and `arcterm-wasm-plugin` declared as path dependencies)

- **AI layer** (`arcterm-ai`): LLM backend abstraction, pane context, system prompts, destructive command detection, inline suggestions, agent session management. Depends on `mux` and `config`.
  - Evidence: `arcterm-ai/Cargo.toml` lines 9-11; `arcterm-ai/src/lib.rs`

- **WASM plugin layer** (`arcterm-wasm-plugin`): capability-based sandboxed WASM plugin loading via wasmtime Component Model. Depends on `mux` and `config`.
  - Evidence: `arcterm-wasm-plugin/Cargo.toml` lines 8-14; `arcterm-wasm-plugin/src/lib.rs`

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

### Data Flow: AI Pane

1. User triggers the AI pane action (key assignment not yet wired into `KeyAssignment` enum or `commands.rs` — the overlay modules exist but are not yet bound to user-invocable commands).
   - Evidence: `wezterm-gui/src/main.rs` line 37 (`mod ai_pane` declared); `wezterm-gui/src/overlay/mod.rs` line 9 (`pub mod ai_command_overlay` declared); no `KeyAssignment` or `commands.rs` entry found for `OpenAiPane` or `ToggleCommandOverlay`.
   - [Inferred] The overlay dispatch mechanism would follow the existing `start_overlay` / `start_overlay_pane` pattern in `wezterm-gui/src/overlay/mod.rs`.
2. `open_ai_pane` acquires a `TermWizTerminal`, checks LLM availability via `arcterm_ai::backend::LlmBackend::is_available()`, then enters a blocking event loop.
   - Evidence: `wezterm-gui/src/ai_pane.rs` lines 24-63
3. On user input, messages are assembled as `Vec<Message>` (system + conversation history), sent to `LlmBackend::chat()`, and the streaming response is tokenized from NDJSON lines.
   - Evidence: `wezterm-gui/src/ai_pane.rs` lines 75-110; `arcterm-ai/src/backend/mod.rs` lines 39-58
4. Tokens are rendered to the `TermWizTerminal` surface via `term.render(&[Change::Text(...)])`.

### Data Flow: AI Command Overlay

1. `show_command_overlay` acquires a `TermWizTerminal`, displays a line editor prompt, collects a natural-language query.
   - Evidence: `wezterm-gui/src/overlay/ai_command_overlay.rs` lines 75-80
2. The query is sent to `LlmBackend::generate()` using `COMMAND_OVERLAY_SYSTEM_PROMPT` from `arcterm-ai/src/prompts.rs`.
3. The response is cleaned (markdown stripped) and optionally prefixed with the `WARNING_LABEL` from `arcterm_ai::destructive::maybe_warn`.
   - Evidence: `arcterm-ai/src/destructive.rs` lines 52-58

### Data Flow: Inline Suggestion (Ghost Text)

The `SuggestionState` struct in `wezterm-gui/src/suggestion_overlay.rs` manages debounce logic (300ms default), cookie-based invalidation, and Tab-to-accept / Escape-to-dismiss state. The key logic is complete and unit-tested, but the module is **not yet compiled into the binary**: the declaration is commented out in `main.rs`.

- Evidence: `wezterm-gui/src/main.rs` lines 39-40:
  ```rust
  // Suggestion overlay not yet wired — state management ready, Pane wrapper TODO
  // mod suggestion_overlay;
  ```
- Evidence: `arcterm-ai/src/suggestions.rs` `is_at_shell_prompt()` uses OSC 133 semantic zones as primary signal with a heuristic fallback (cursor on last row + shell-like process name).

### Data Flow: WASM Plugin System

1. During config evaluation, Lua calls `wezterm.plugin.register_wasm()` which calls `arcterm_wasm_plugin::config::register_plugin()`, appending a `WasmPluginConfig` to the global `REGISTERED_PLUGINS` `Mutex<Vec<...>>`.
   - Evidence: `arcterm-wasm-plugin/src/config.rs` lines 56-73
2. At startup, `take_registered_plugins()` drains the global list and `PluginManager::load_all()` iterates over them.
   - Evidence: `arcterm-wasm-plugin/src/lifecycle.rs` lines 81-124
3. For each plugin: `loader::load_plugin()` reads the `.wasm` bytes, compiles them into a `wasmtime::component::Component`, creates a `Store<PluginStoreData>` with memory limits (`StoreLimitsBuilder`) and a fuel budget, then transitions the plugin through `Loading → Initializing → Running`.
   - Evidence: `arcterm-wasm-plugin/src/loader.rs` lines 127-183
4. Before each guest callback, `loader::refuel_store()` resets the fuel budget to prevent cross-callback accumulation.
   - Evidence: `arcterm-wasm-plugin/src/loader.rs` lines 207-214 (documented rationale in doc comment)
5. Host functions are registered into a `wasmtime::component::Linker<PluginStoreData>` via `host_api::create_default_linker()`. Each function checks `ctx.data().capabilities.check()` before performing any privileged operation.
   - Evidence: `arcterm-wasm-plugin/src/host_api.rs` lines 354-364
6. [Inferred] The `PluginManager` is not yet instantiated anywhere in `wezterm-gui`. The `arcterm-wasm-plugin` crate is a dependency of `wezterm-gui` but no call site was found in `wezterm-gui/src/`.

### Capability Model (WASM Plugin Sandboxing)

Capability strings follow the format `<resource>:<operation>[:<target>]`. Recognized capabilities:

| Capability String | Effect |
|---|---|
| `terminal:read` | Always granted by default (built into `CapabilitySet::new`) |
| `terminal:write` | Required for `send-text`, `inject-output` |
| `fs:read:<path>` | Required for `read-file`; path-prefix enforced; `..` rejected |
| `fs:write:<path>` | Required for `write-file`; same enforcement |
| `net:connect:<host>:<port>` | Required for `http-get`, `http-post`; exact host:port match |
| `keybinding:register` | Required for `register-key-binding` |

- Evidence: `arcterm-wasm-plugin/src/capability.rs` lines 51-87 (parsing), lines 115-161 (enforcement including path-traversal block)

The WIT world `arcterm-plugin@1.0.0` defines the host-guest interface contract formally.
- Evidence: `arcterm-wasm-plugin/wit/plugin-host-api.wit` lines 112-127

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

Background threads (non-GUI):
- One thread per pane: read_from_pane_pty (blocking PTY reader)
- One thread per pane: parse_buffered_data (escape sequence parser)
- One thread: LocalListener::run (Unix socket acceptor)
- SSH sessions: async tasks on a smol executor
- AI queries (arcterm-ai): blocking HTTP via ureq, run on overlay threads
  via promise::spawn::spawn_into_new_thread (TermWizTerminal pattern)
```

### Cross-Platform Abstraction Layers

| Concern | Abstraction | Platform Impls |
|---------|-------------|----------------|
| Window / event loop | `window::Connection` + `ConnectionOps` trait | `window/src/os/macos/`, `x11/`, `wayland/`, `windows/` |
| PTY allocation | `portable_pty::PtySystem` trait | Unix `openpty`, Windows ConPTY |
| GPU rendering | `RenderContext` enum in `wezterm-gui/src/renderstate.rs` | Glium (OpenGL), WebGPU (wgpu) |
| Font loading | `wezterm-font::FontConfiguration` | FreeType, CoreText, DirectWrite |
| SSH | `wezterm-ssh::Session` | `ssh2` (libssh2), `libssh-rs` |
| LLM backend | `arcterm_ai::backend::LlmBackend` trait | `OllamaBackend`, `ClaudeBackend` |
| WASM sandbox | `wasmtime::Store<PluginStoreData>` | wasmtime v36 Component Model |

### ArcTerm-Specific Integration Status

| Feature | Crate | GUI wired? | Notes |
|---|---|---|---|
| AI pane (conversational) | `arcterm-ai` | Partially | `mod ai_pane` declared in `main.rs`; no `KeyAssignment` binding found |
| Command overlay | `arcterm-ai` | Partially | `pub mod ai_command_overlay` in `overlay/mod.rs`; no binding found |
| Inline suggestion (ghost text) | `arcterm-ai` | Not yet | `suggestion_overlay.rs` exists but `mod` is commented out in `main.rs` |
| WASM plugin loading | `arcterm-wasm-plugin` | Not yet | Crate is a dep of `wezterm-gui` but no call site found in `src/` |

Evidence: `wezterm-gui/src/main.rs` lines 37-40 for AI pane/suggestion status; `wezterm-gui/Cargo.toml` lines 38,99 for dependency declarations.

---

## Summary Table

| Aspect | Detail | Confidence |
|--------|--------|------------|
| Architectural pattern | Layered monolith (GUI → mux → term → PTY) | Observed |
| ArcTerm-specific crates | `arcterm-ai` and `arcterm-wasm-plugin` (both are workspace members) | Observed |
| arcterm-structured-output | Removed; not present in workspace or codebase | Observed |
| AI pane integration | Module compiled (`mod ai_pane`); no key binding wired yet | Observed |
| Inline suggestion integration | Module exists but commented out of `main.rs` | Observed |
| WASM plugin integration | Crate is a wezterm-gui dep; no instantiation in GUI found | Observed |
| Async executor (GUI) | Custom `SpawnQueue` integrated with native event loop (not Tokio) | Observed |
| Async executor (background) | `smol` / `async-executor` | Observed |
| GPU backends | Glium (OpenGL) and WebGPU (wgpu), selected via `RenderContext` enum | Observed |
| IPC protocol | Length-prefixed LEB128 PDUs over Unix socket | Observed |
| Plugin/scripting system | Lua via `mlua`, registered through `add_context_setup_func` | Observed |
| LLM HTTP client | `ureq` v2 (sync/blocking), runs on overlay threads | Observed |
| WASM runtime | `wasmtime` v36 with Component Model and fuel metering | Observed |
| PTY threading model | 2 threads per pane (reader + parser), plus GUI main thread | Observed |
| Cross-platform window backends | macOS (Cocoa/CF), X11, Wayland, Windows (Win32) | Observed |

## Open Questions

- The `KeyAssignment` enum (in `config/`) has no `OpenAiPane` or `ToggleCommandOverlay` variants; neither does `commands.rs` in `wezterm-gui`. The mechanism for triggering the AI pane and command overlay from a user key press is not yet implemented or is in a branch not visible here.
- `arcterm-wasm-plugin` is declared as a dependency of `wezterm-gui` but no startup call to `PluginManager::load_all` or `take_registered_plugins` was found in `wezterm-gui/src/`. The integration call site is missing.
- The `FrontEndSelection` config option (`config/src/config.rs`) implies an alternate frontend path; its full extent is not traced here.
- WebGPU (`wgpu`) is present as a render backend but its completeness relative to Glium is not verified from code inspection alone.
- The `wezterm.plugin.register_wasm()` Lua function (the config-side entry point for WASM plugin registration) is referenced in `arcterm-wasm-plugin/src/config.rs` doc comments but the actual Lua registration code in `lua-api-crates/` has not been verified.
