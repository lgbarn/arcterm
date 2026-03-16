# STACK.md

## Overview
Arcterm is a GPU-accelerated terminal emulator written entirely in Rust (edition 2024) using a Cargo workspace of six crates. The rendering pipeline is built on wgpu/winit, the async runtime is Tokio, and the plugin system runs sandboxed WebAssembly Components via wasmtime. There are no JavaScript, Python, or other language files in the host application; plugins are also written in Rust and compiled to `wasm32-wasip2`.

## Findings

### Language and Runtime

- **Primary language**: Rust, edition 2024 (workspace-wide)
  - Evidence: `Cargo.toml` line 13 — `edition = "2024"`
- **Toolchain pin**: Stable Rust channel
  - Evidence: `rust-toolchain.toml` — `channel = "stable"`
- **Async runtime**: Tokio 1 (full feature set)
  - Evidence: `Cargo.toml` line 26 — `tokio = { version = "1", features = ["full"] }`
- **Blocking executor** (used for GPU init): pollster 0.4
  - Evidence: `Cargo.toml` line 27 — `pollster = "0.4"`

### Workspace Structure

The project is a Cargo workspace resolver v2 with six member crates:

| Crate | Role |
|---|---|
| `arcterm-core` | Shared data types: `Cell`, `Grid`, `Color`, `CursorPos` |
| `arcterm-vt` | VT/ANSI escape sequence parser (wraps `vte`) |
| `arcterm-pty` | Cross-platform PTY management |
| `arcterm-render` | wgpu renderer, glyph atlas, structured text |
| `arcterm-app` | Application binary — event loop, multiplexer, AI detection |
| `arcterm-plugin` | Plugin host runtime (wasmtime) and WIT interface |

Evidence: `Cargo.toml` lines 2-9

### GPU Rendering Stack

- **GPU API**: wgpu 28
  - Evidence: `Cargo.toml` line 23 — `wgpu = "28"`
  - Evidence: `arcterm-render/src/lib.rs` — modules `gpu`, `renderer`, `quad`, `text`
- **Windowing**: winit 0.30
  - Evidence: `Cargo.toml` line 24 — `winit = "0.30"`
- **Text/glyph rendering**: glyphon 0.10
  - Evidence: `Cargo.toml` line 25 — `glyphon = "0.10"`
- **GPU buffer casting utility**: bytemuck 1 (with `derive` feature)
  - Evidence: `arcterm-render/Cargo.toml` line 19 — `bytemuck = { version = "1", features = ["derive"] }`
- **GPU tests**: Mesa software Vulkan renderer used in CI (`WGPU_BACKEND=vulkan`)
  - Evidence: `.github/workflows/ci.yml` lines 35-42

### Terminal Emulation

- **VT parser**: vte 0.15
  - Evidence: `Cargo.toml` line 21 — `vte = "0.15"`
- **PTY (cross-platform)**: portable-pty 0.9
  - Evidence: `Cargo.toml` line 22 — `portable-pty = "0.9"`
- **System call bindings**: libc 0.2 (used in `arcterm-pty` and `arcterm-app`)
  - Evidence: `arcterm-pty/Cargo.toml` line 12; `arcterm-app/Cargo.toml` line 33

### Plugin System

- **WASM runtime**: wasmtime 42, with Component Model and Cranelift JIT enabled
  - Evidence: `Cargo.toml` line 39 — `wasmtime = { version = "42", features = ["component-model", "cranelift"] }`
- **WASI p2 host functions**: wasmtime-wasi 42
  - Evidence: `Cargo.toml` line 40 — `wasmtime-wasi = "42"`
  - Evidence: `arcterm-plugin/src/runtime.rs` — `add_to_linker_sync(&mut linker)?`
- **Plugin interface language**: WIT (WebAssembly Interface Types)
  - Evidence: `arcterm-plugin/wit/arcterm.wit` — defines `world arcterm-plugin` with `load`, `update`, `render` exports
- **Plugin compilation target**: `wasm32-wasip2`
  - Evidence: `examples/plugins/hello-world/Cargo.toml` — `[lib] crate-type = ["cdylib"]`; build instructions reference `--target wasm32-wasip2`
- **Guest-side binding generator**: wit-bindgen 0.36 (used by plugin authors)
  - Evidence: `examples/plugins/hello-world/Cargo.toml` — `wit-bindgen = "0.36"`
- **Optional plugin build toolchain**: cargo-component (produces WASM Component; not required)
  - Evidence: `examples/plugins/hello-world/Cargo.toml` — build instructions

### Serialization and Data Formats

- **JSON**: serde_json 1
  - Evidence: `Cargo.toml` line 37 — `serde_json = "1"`
- **TOML**: toml 1 (config file parsing)
  - Evidence: `arcterm-app/Cargo.toml` line 29 — `toml = "1"`
- **Serde**: serde 1 with `derive` feature
  - Evidence: `arcterm-app/Cargo.toml` line 30
- **MessagePack**: rmpv 1 (Neovim RPC protocol)
  - Evidence: `Cargo.toml` line 31 — `rmpv = "1"`
- **Base64**: base64 0.22
  - Evidence: `Cargo.toml` line 35 — `base64 = "0.22"`

### Rich Content and Text Processing

- **Syntax highlighting**: syntect 5
  - Evidence: `Cargo.toml` line 33 — `syntect = "5"`
- **Markdown parsing**: pulldown-cmark 0.12
  - Evidence: `Cargo.toml` line 34 — `pulldown-cmark = "0.12"`
- **Image loading**: image 0.25
  - Evidence: `Cargo.toml` line 35 — `image = "0.25"`
- **Regex**: regex 1
  - Evidence: `Cargo.toml` line 36 — `regex = "1"`
- **Diff/similar text**: similar 2
  - Evidence: `arcterm-app/Cargo.toml` line 40 — `similar = "2"`

### Application Utilities

- **CLI argument parsing**: clap 4 (with `derive`)
  - Evidence: `arcterm-app/Cargo.toml` line 38
- **Clipboard access**: arboard 3
  - Evidence: `arcterm-app/Cargo.toml` line 28 — `arboard = "3"`
- **Platform config/data directories**: dirs 6
  - Evidence: `arcterm-app/Cargo.toml` line 31; `arcterm-plugin/Cargo.toml` line 16
- **Filesystem watching** (config hot-reload): notify 8
  - Evidence: `arcterm-app/Cargo.toml` line 32 — `notify = "8"`
  - Evidence: `arcterm-app/src/config.rs` lines 11-12 — `watch_config` background thread
- **Error handling**: anyhow 1
  - Evidence: `Cargo.toml` line 41 — `anyhow = "1"`
- **Logging**: log 0.4 + env_logger 0.11
  - Evidence: `Cargo.toml` lines 28-29

### Build Tools

- **Primary build tool**: Cargo (standard Rust)
- **Release distribution**: cargo-dist 0.31.0
  - Evidence: `dist-workspace.toml` line 7 — `cargo-dist-version = "0.31.0"`
  - Evidence: `.github/workflows/release.yml` — autogenerated by cargo-dist
- **Manpage generation** (build dep): clap_mangen 0.2
  - Evidence: `arcterm-app/Cargo.toml` line 43 — `[build-dependencies] clap_mangen = "0.2"`
- **Cargo aliases** (developer shortcuts):
  - `xt` → test core/vt/pty; `xr` → run app; `xc` → clippy workspace
  - Evidence: `.cargo/config.toml` lines 2-4
- **Linker override** (x86_64 macOS): lld
  - Evidence: `.cargo/config.toml` lines 8-9 — `-fuse-ld=lld`

### CI/CD

- **Platform**: GitHub Actions
- **CI workflow** (`.github/workflows/ci.yml`): format check, clippy (`-D warnings`), build, unit tests, GPU tests with Mesa software rendering
  - Matrix: `ubuntu-latest`, `macos-latest`, `windows-latest`
  - Evidence: `.github/workflows/ci.yml` lines 8, 17-24
- **Release workflow** (`.github/workflows/release.yml`): triggered on SemVer git tags; uses cargo-dist to build artifacts and publish GitHub Releases
  - Evidence: `.github/workflows/release.yml` lines 41-45, 279
- **Rust caching**: `Swatinem/rust-cache@v2`
  - Evidence: `.github/workflows/ci.yml` line 15

### Release Targets

| Target Triple | Platform |
|---|---|
| `aarch64-apple-darwin` | macOS Apple Silicon |
| `x86_64-apple-darwin` | macOS Intel |
| `x86_64-unknown-linux-gnu` | Linux x86_64 |
| `x86_64-pc-windows-msvc` | Windows x86_64 |

Evidence: `dist-workspace.toml` lines 13-18

Installers generated: shell script (Unix) and PowerShell (Windows).
Evidence: `dist-workspace.toml` line 11 — `installers = ["shell", "powershell"]`

### Configuration System

- **Format**: TOML, located at `~/.config/arcterm/config.toml`
  - Evidence: `arcterm-app/src/config.rs` lines 7-8
- **Hot-reload**: background thread using `notify` watcher; sends fresh config over `std::sync::mpsc` channel
  - Evidence: `arcterm-app/src/config.rs` — `watch_config` function
- **Config overlays**: documented in `examples/config/overlay-font.toml`, `examples/config/overlay-colors.toml`

### Development Dependencies

- **tempfile 3** — used in `arcterm-app` and `arcterm-plugin` test suites
  - Evidence: `arcterm-app/Cargo.toml` line 47; `arcterm-plugin/Cargo.toml` line 19

## Summary Table

| Item | Detail | Confidence |
|---|---|---|
| Language | Rust, edition 2024 | Observed |
| Toolchain | Stable channel | Observed |
| Async runtime | Tokio 1 (full) | Observed |
| GPU API | wgpu 28 | Observed |
| Windowing | winit 0.30 | Observed |
| Text rendering | glyphon 0.10 | Observed |
| VT parser | vte 0.15 | Observed |
| PTY | portable-pty 0.9 | Observed |
| Plugin VM | wasmtime 42 (Component Model + Cranelift) | Observed |
| Plugin interface | WIT 0.1.0 | Observed |
| Plugin target | wasm32-wasip2 | Observed |
| Config format | TOML | Observed |
| CI | GitHub Actions (3-platform matrix) | Observed |
| Release tool | cargo-dist 0.31.0 | Observed |
| Release targets | 4 platforms (macOS arm/x86, Linux, Windows) | Observed |
| Linker (x86 macOS) | lld | Observed |

## Open Questions

- The `arcterm-core/Cargo.toml` has no `[dependencies]` section — it appears to be a pure types crate with no external deps. Needs confirmation if a `[dependencies]` section is intentionally absent or was never added.
- No `Cargo.lock` was inspected (excluded from glob results due to size); exact transitive dependency versions are not documented here.
- The `latency-trace` feature in `arcterm-app` (`arcterm-app/Cargo.toml` line 16) enables fine-grained timestamp logging. Whether this is used in production builds is [Inferred] to be disabled by default.
