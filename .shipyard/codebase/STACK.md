# STACK.md

## Overview

ArcTerm is a Rust workspace of approximately 60 crates, forked from WezTerm. The primary language is Rust (minimum 1.71.0, Lua 5.4 embedded via mlua). The project targets macOS, Linux (X11 and Wayland), and Windows and renders terminal output through a dual GPU backend (Glium/OpenGL and wgpu/WebGPU). Configuration and scripting are driven by an embedded Lua 5.4 runtime. Two ArcTerm-specific crates — `arcterm-ai` and `arcterm-wasm-plugin` — add LLM integration and a WASM sandbox layer on top of the upstream WezTerm base. The `arcterm-structured-output` crate has been removed from the workspace.

---

## Findings

### Primary Language

- **Rust**: The sole systems implementation language. All crates use Rust editions 2018 or 2021.
  - Evidence: `wezterm-gui/Cargo.toml` (edition = "2018"), `wezterm-font/Cargo.toml` (edition = "2021")
  - Minimum supported version: **1.71.0**
  - Evidence: `ci/check-rust-version.sh` (`min_rust="1.71.0"`)

### Lua Scripting Runtime

- **mlua v0.9** (vendored Lua 5.4): Embedded Lua interpreter for user configuration and plugin scripting. Compiled with `features=["vendored", "lua54", "async", "send", "serialize"]`.
  - Evidence: `config/Cargo.toml` line 27 (`mlua = {workspace=true, features=["vendored", "lua54", "async", "send", "serialize"]}`)
  - The `config` crate drives config loading; `wezterm-gui` also links mlua for GUI event emission.
  - Config file searched as `arcterm.lua` (or `wezterm.lua` for backwards compatibility).
  - Evidence: `wezterm-gui/src/main.rs` line 74 (`/// Skip loading arcterm.lua (or wezterm.lua for backwards compatibility)`)

### Build System and Toolchain

- **Cargo**: Standard Rust workspace build tool with resolver v2.
  - Evidence: `Cargo.toml` lines 1-20 (`[workspace]`, `resolver = "2"`)
- **Makefile**: Thin wrapper around `cargo nextest run`, `cargo check`, `cargo build`, `cargo fmt`.
  - Evidence: `Makefile` lines 1-24
- **cargo-nextest**: Used for test execution (see Makefile `test:` target).
- **cargo-deny**: License and advisory scanning config present.
  - Evidence: `deny.toml` (references `cargo-deny` checks for advisories, licenses, bans, sources)
- **cc crate** (v1.0, parallel features): Used for compiling C/C++ native dependencies.
  - Evidence: `Cargo.toml` line 59 (`cc = {version="1.0", features = ["parallel"]}`)
- **embed-resource** (v1.7): Embeds Windows resources (icons, manifests) at build time.
  - Evidence: `Cargo.toml` line 83; `wezterm-gui/Cargo.toml` line 33
- **gl_generator** (v0.14): Generates OpenGL bindings at build time.
  - Evidence: `window/Cargo.toml` line 15

### Release Build Profile

- **opt-level = 3** for release builds; debug info is commented out.
  - Evidence: `Cargo.toml` lines 26-28

### Package Manager

- **Cargo** (crates.io, workspace path dependencies, one git dependency).
  - Evidence: `Cargo.toml` entire `[workspace.dependencies]` section
- One patch override: `cairo-sys-rs` points to `deps/cairo` (vendored minimal Cairo).
  - Evidence: `Cargo.toml` lines 279-283

---

## Findings: ArcTerm-Specific Crates

### arcterm-ai (LLM Integration Layer)

- **Crate**: `arcterm-ai` v0.1.0, Rust edition 2021.
  - Evidence: `arcterm-ai/Cargo.toml` lines 1-5
- **Dependencies** (crate-local, not workspace-declared):
  - `ureq` v2 with `json` feature — synchronous HTTP client used for both Ollama and Claude API calls. The response body is returned as a streaming `Box<dyn Read + Send>` (NDJSON lines).
    - Evidence: `arcterm-ai/Cargo.toml` line 14; `arcterm-ai/src/backend/ollama.rs` line 34; `arcterm-ai/src/backend/claude.rs` line 44
  - `serde_json` (workspace) — request body construction and agent step parsing.
    - Evidence: `arcterm-ai/src/agent.rs` lines 183-209
  - `anyhow`, `log`, `serde` (workspace) — error handling, logging, serialization.
  - `config` (path dep `../config`), `mux` (path dep `../mux`) — access to configuration types and pane context.
    - Evidence: `arcterm-ai/Cargo.toml` lines 9, 11
- **Module structure** (`arcterm-ai/src/`):
  - `backend/mod.rs` — `LlmBackend` trait (`chat`, `generate`, `is_available`, `name`); `create_backend` factory.
  - `backend/ollama.rs` — `OllamaBackend`: POSTs to `{endpoint}/api/chat` (default `http://localhost:11434`). Availability check via `{endpoint}/api/tags` with 2-second timeout.
  - `backend/claude.rs` — `ClaudeBackend`: POSTs to `https://api.anthropic.com/v1/messages` with `anthropic-version: 2023-06-01` header. Availability is purely key-presence check.
  - `config.rs` — `AiConfig` struct (backend selection, endpoint, model, api_key, context_lines). Default: Ollama at `http://localhost:11434`, model `qwen2.5-coder:7b`, 30 scrollback lines.
  - `context.rs` — `PaneContext` struct (scrollback, cwd, foreground process, dimensions).
  - `prompts.rs` — `AI_PANE_SYSTEM_PROMPT`, `COMMAND_OVERLAY_SYSTEM_PROMPT` constants; `format_context_message`.
  - `suggestions.rs` — `SuggestionConfig` (debounce 300ms, accept key Tab); `is_at_shell_prompt`, `build_suggestion_query`, `clean_suggestion`.
  - `agent.rs` — `AgentSession` state machine (Planning → Reviewing → Executing → Completed/StepFailed/Aborted); `parse_steps` JSON response parser; `build_agent_query`.
  - `destructive.rs` — `is_destructive` heuristic pattern matcher; `maybe_warn`.
- Evidence: `arcterm-ai/src/lib.rs` (module declarations); full source files above.

### arcterm-wasm-plugin (WASM Sandbox Layer)

- **Crate**: `arcterm-wasm-plugin` v0.1.0, Rust edition 2021.
  - Evidence: `arcterm-wasm-plugin/Cargo.toml` lines 1-5
- **WASM runtime**: **wasmtime v36** with `component-model` feature — the only wasmtime version in the workspace.
  - Evidence: `arcterm-wasm-plugin/Cargo.toml` line 14 (`wasmtime = { version = "36", features = ["component-model"] }`)
- **Other dependencies**: `anyhow`, `log`, `thiserror` (workspace); `lazy_static` (workspace, for global plugin registry); `config`, `mux` (path deps); `tempfile` (dev-dep for loader tests).
  - Evidence: `arcterm-wasm-plugin/Cargo.toml` lines 8-17
- **Module structure** (`arcterm-wasm-plugin/src/`):
  - `capability.rs` — `Capability` parser (format: `resource:operation[:target]`); `CapabilitySet` with path-prefix enforcement for `fs:*` and exact host:port match for `net:connect`. Path traversal (`..` components) blocked explicitly. `terminal:read` is always granted by default.
  - `config.rs` — `WasmPluginConfig` (name, path, capabilities Vec<String>, memory_limit_mb default 64, fuel_per_callback default 1_000_000, enabled); global `REGISTERED_PLUGINS` Mutex registry populated from Lua config.
  - `loader.rs` — `create_engine` (fuel + Component Model enabled); `load_plugin` (reads `.wasm` bytes → compiles `Component` → creates `Store<PluginStoreData>` with `StoreLimitsBuilder` memory cap and initial fuel); `refuel_store` (resets per-callback fuel budget before each dispatch).
  - `host_api.rs` — Four WIT interface groups registered into a `Linker<PluginStoreData>`: `arcterm:plugin/log` (info/warn/error), `arcterm:plugin/filesystem` (read-file/write-file), `arcterm:plugin/network` (http-get/http-post — **placeholder, not yet implemented**), `arcterm:plugin/terminal` (send-text/inject-output — **placeholder, logs only**). Every privileged function checks `CapabilitySet::check()` before acting.
  - `lifecycle.rs` — `PluginState` enum (Loading → Initializing → Running → Stopping → Stopped / Failed); `PluginManager::load_all` (loads each plugin independently, failures don't cascade); `shutdown_all`.
  - `event_router.rs` — `EventRouter` dispatches `PluginEvent` (OutputChanged, Bell, FocusChanged, KeyBindingTriggered) to subscriber lists by plugin name.
- Evidence: `arcterm-wasm-plugin/src/lib.rs`; individual source files above.

### Removed: arcterm-structured-output

- The `arcterm-structured-output` crate (OSC 7770 protocol, `syntect`-based syntax highlighting) has been removed from the workspace. No reference to it remains in any `Cargo.toml` file.
  - Evidence: `Cargo.toml` `[workspace] members` list (lines 2-21) — no `arcterm-structured-output` entry; grep of all `.toml` files for `syntect` and `arcterm-structured-output` returned zero matches.

---

## Findings: Key Dependency Categories

### Rendering and GPU

| Dependency | Version | Role |
|---|---|---|
| `wgpu` | 25.0.2 | WebGPU rendering backend (primary path) |
| `glium` | 0.35 | OpenGL rendering backend (legacy/fallback) |
| `tiny-skia` | 0.11 | CPU-side 2D rasterization |
| `euclid` | 0.22 | Geometry primitives |
| `guillotiere` | 0.6 | Texture atlas allocation |
| `bytemuck` | 1.4 | Safe byte casting for GPU buffers |

Evidence: `wezterm-gui/Cargo.toml` lines 39, 88, 107; `Cargo.toml` lines 263, 222, 91

Shader files present: `wezterm-gui/src/glyph-frag.glsl`, `wezterm-gui/src/glyph-vertex.glsl`, `wezterm-gui/src/shader.wgsl`.

Both backends are selectable at runtime. `RenderContext` enum dispatches to either Glium or WebGpu paths.
- Evidence: `wezterm-gui/src/renderstate.rs` lines 23-31

### Font Rendering Pipeline

| Dependency | Version | Role |
|---|---|---|
| `freetype` | path dep | Font rasterization (vendored `deps/freetype`) |
| `harfbuzz` | path dep | Text shaping (vendored `deps/harfbuzz`) |
| `fontconfig` | path dep | Font discovery on Linux/Android |
| `core-text` | =21.0.0 | Font discovery on macOS |
| `dwrote` | 0.11 | Font discovery on Windows (DirectWrite) |
| `cairo-rs` | 0.18 | Cairo graphics (vendored `deps/cairo`) |

Evidence: `wezterm-font/Cargo.toml` lines 21-28, 43-55

Locator, shaper, and rasterizer are all trait-abstracted with platform-specific implementations.
- Evidence: `wezterm-font/src/locator/` (font_config.rs, core_text.rs, gdi.rs), `wezterm-font/src/shaper/` (harfbuzz.rs), `wezterm-font/src/rasterizer/` (freetype.rs, harfbuzz.rs)

Bundled fonts (behind feature flags):
- Nerd Font Symbols, JetBrains Mono, Roboto, Noto Emoji
- Evidence: `wezterm-gui/Cargo.toml` lines 16-25 (vendored-fonts feature group)

### Windowing and Platform Abstraction

| Dependency | Version | Platform |
|---|---|---|
| `xcb` | 1.3 | Linux/X11 window management |
| `xcb-imdkit` | git rev 212330f | X11 input method |
| `xkbcommon` | 0.7.0 | Keyboard handling (X11/Wayland) |
| `wayland-client` | 0.31 | Wayland client |
| `smithay-client-toolkit` | 0.19 | Wayland toolkit |
| `cocoa` / `objc` | =0.25.0 / 0.2 | macOS window management |
| `cgl` | 0.3 | macOS OpenGL context |
| `winapi` | 0.3.9 | Windows API |
| `windows` | 0.33.0 | Windows COM/WinRT |

Evidence: `window/Cargo.toml` lines 50-99

### Async Runtime

- **smol** (v2.0): Primary async executor used throughout the codebase.
  - Evidence: `Cargo.toml` line 204; `mux/Cargo.toml` line 41; `config/Cargo.toml` line 34
- **async-executor**, **async-io**, **async-channel**, **async-task**: smol ecosystem components.
  - Evidence: `Cargo.toml` lines 39-42
- **tokio** (v1.43): Present as a workspace dependency but not prominently used in core crates. [Inferred] Likely used by `reqwest` or transitive dependencies.
  - Evidence: `Cargo.toml` line 223
- **Note**: `arcterm-ai` uses `ureq` (synchronous, blocking) rather than an async HTTP client, which avoids a tokio runtime dependency in the AI crate itself.
  - Evidence: `arcterm-ai/Cargo.toml` line 14

### Networking and Cryptography

| Dependency | Version | Role |
|---|---|---|
| `openssl` | 0.10.57 | TLS (OpenSSL bindings) |
| `async_ossl` | path | Async OpenSSL wrapper (local crate) |
| `rcgen` | 0.12 | TLS certificate generation |
| `sha2` | 0.10 | Hashing |
| `base64` | 0.22 | Encoding |
| `reqwest` | 0.12 | HTTP client (workspace dep; transitive use) |
| `http_req` | 0.11 | Lightweight HTTP client (update checks) |
| `ureq` | 2 (json feature) | Synchronous HTTP client (`arcterm-ai` only) |

Evidence: `Cargo.toml` lines 108, 159, 183-184; `wezterm-gui/src/update.rs` lines 4-5; `arcterm-ai/Cargo.toml` line 14

### Serialization

| Dependency | Version | Role |
|---|---|---|
| `serde` | 1.0 | De/serialization framework |
| `serde_json` | 1.0 | JSON |
| `serde_yaml` | 0.9 | YAML |
| `toml` | 0.8 | TOML config parsing |
| `varbincode` | 0.1 | Variable-length binary encoding for mux PDUs |

Evidence: `Cargo.toml` lines 188-194, 235; `codec/src/lib.rs` lines 293, 326

### Terminal Parsing

| Dependency | Version | Role |
|---|---|---|
| `vtparse` | 0.7 (path) | VT escape sequence parser |
| `terminfo` | 0.9 | Terminfo database |
| `pest` | 2.7 | PEG parser (tmux cc grammar) |
| `fancy-regex` | 0.14 | Regex (no-std compatible) |
| `encoding_rs` | 0.8 | Character encoding |
| `unicode-segmentation` | 1.12 | Unicode text segmentation |

Evidence: `Cargo.toml` lines 164-165, 235; `termwiz/Cargo.toml` lines 14-39

### Persistence

- **rusqlite** (v0.32): SQLite via rusqlite for font database and other persistent data.
  - Evidence: `Cargo.toml` line 187
- **sqlite-cache** (v0.1.4): SQLite-backed cache abstraction.
  - Evidence: `Cargo.toml` line 202

### Image Handling

- **image** (v0.25): Image decoding/encoding (used for inline image protocol, backgrounds).
  - Evidence: `Cargo.toml` line 122; `term/Cargo.toml` line 23
- **tiny-skia** (v0.11): Pure-Rust 2D graphics for CPU-side rendering.
  - Evidence: `Cargo.toml` line 222

### IPC and Process

| Dependency | Version | Role |
|---|---|---|
| `portable-pty` | path | Cross-platform PTY abstraction |
| `procinfo` | path | Process info queries |
| `serial2` | 0.2 | Serial port access |
| `filedescriptor` | 0.8.3 | File descriptor utilities |
| `mio` | 0.8 | I/O event loop (Linux) |
| `signal-hook` | 0.3 | Unix signal handling |

Evidence: `Cargo.toml` lines 92, 175, 193; `mux/Cargo.toml` lines 35, 39

### Metrics and Profiling

- **metrics** (v0.23): Metrics facade used for performance counters.
  - Evidence: `Cargo.toml` line 141; `wezterm-gui/Cargo.toml` line 63
- **hdrhistogram** (v7.1): High-dynamic-range latency histograms.
  - Evidence: `wezterm-gui/Cargo.toml` line 59
- **dhat** (v0.3): Optional heap profiler (behind `dhat-heap` / `dhat-ad-hoc` features).
  - Evidence: `wezterm-gui/Cargo.toml` lines 26-27; `wezterm-gui/src/main.rs` lines 61-63

---

## Findings: Platform Support

| Platform | Status | Notes |
|---|---|---|
| macOS | Supported | CGL/Cocoa, Core Text, objc2 bindings |
| Linux X11 | Supported | XCB, xkbcommon, fontconfig |
| Linux Wayland | Supported (optional feature) | smithay-client-toolkit, wayland-client 0.31 |
| Windows | Supported | DirectWrite, WinAPI, COM/WinRT |
| WSL | Supported | Auto-detected WSL distros via `wsl.exe -l -v` |

Evidence: `window/Cargo.toml` (platform-conditional deps); `config/src/wsl.rs`

---

## Summary Table

| Item | Detail | Confidence |
|---|---|---|
| Primary language | Rust | Observed |
| Minimum Rust version | 1.71.0 | Observed (`ci/check-rust-version.sh`) |
| Rust editions in use | 2018, 2021 (mixed) | Observed |
| Scripting language | Lua 5.4 (vendored via mlua 0.9) | Observed |
| Build tool | Cargo (workspace resolver v2) | Observed |
| Test runner | cargo-nextest | Observed (`Makefile`) |
| Dependency scanner | cargo-deny | Observed (`deny.toml`) |
| Primary GPU backend | wgpu 25.0.2 (WebGPU) | Observed |
| Legacy GPU backend | glium 0.35 (OpenGL) | Observed |
| Font shaper | HarfBuzz (vendored) | Observed |
| Font rasterizer | FreeType (vendored) | Observed |
| Async executor | smol 2.0 | Observed |
| TLS | OpenSSL 0.10.57 + rcgen 0.12 | Observed |
| Persistence | rusqlite 0.32 (SQLite) | Observed |
| Platforms | macOS, Linux (X11+Wayland), Windows, WSL | Observed |
| WASM runtime | wasmtime 36 (component-model) | Observed (`arcterm-wasm-plugin/Cargo.toml`) |
| AI HTTP client | ureq 2 (synchronous, json feature) | Observed (`arcterm-ai/Cargo.toml`) |
| AI LLM backends | Ollama (default), Claude API | Observed (`arcterm-ai/src/backend/`) |
| Removed: syntect | Not present in any Cargo.toml | Observed |
| Removed: arcterm-structured-output | Not a workspace member | Observed |

## Open Questions

- No `rust-toolchain.toml` file is present. The minimum version script requires 1.71.0 but the actual nightly channel used for formatting (`cargo +nightly fmt`) is unspecified. What nightly version is expected for development?
- `tokio` v1.43 is declared as a workspace dependency but its actual consumers among the non-target crates were not fully enumerated. Which crate pulls it in as a first-party consumer?
- The `wgpu` version (25.0.2) is a very recent release. Has compatibility with all three platform OpenGL/Metal/Vulkan backends been validated?
- `arcterm-ai` uses `ureq` (synchronous, blocking) for LLM HTTP calls. When the AI pane performs streaming completions, blocking occurs on whichever thread calls `chat()`. Is this intentional (the call is made from a dedicated thread), or will it need to be made async in a future integration pass?
- The `arcterm-wasm-plugin` network API (`http-get`, `http-post`) is stubbed and returns `Err("network not yet implemented")`. What HTTP client will be used for plugin network calls — `ureq` (already in `arcterm-ai`) or something else?
