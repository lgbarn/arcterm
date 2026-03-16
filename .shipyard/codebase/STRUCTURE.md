# Structure

## Overview

Arcterm is organized as a Cargo workspace at the repo root with six member crates, each mapping to a single architectural layer. Directory names are self-describing and match the crate names. There is no `src/` at the workspace root — all code lives inside crate-specific subdirectories. Configuration examples, plugin examples, and build tooling live in top-level auxiliary directories.

---

## Findings

### Top-Level Layout

```
myterm/
├── Cargo.toml              # Workspace manifest: members, shared deps, profile.dist
├── Cargo.lock              # Locked dependency graph
├── rust-toolchain.toml     # Pinned Rust toolchain version
├── dist-workspace.toml     # cargo-dist release configuration
├── .cargo/config.toml      # Cargo build config (linker flags, target defaults)
│
├── arcterm-core/           # Layer 1: shared types
├── arcterm-vt/             # Layer 2a: VT parser + terminal state machine
├── arcterm-pty/            # Layer 2b: PTY I/O
├── arcterm-render/         # Layer 3: GPU renderer
├── arcterm-plugin/         # Layer 4: WASM plugin system
├── arcterm-app/            # Layer 5: application binary
│
├── examples/               # Developer resources
│   ├── config/             # Sample TOML overlay files
│   │   ├── base.toml
│   │   ├── overlay-font.toml
│   │   └── overlay-colors.toml
│   └── plugins/            # Example WASM plugin sources
│       ├── hello-world/src/lib.rs
│       └── system-monitor/src/lib.rs
│
└── target/                 # Build artifacts (git-ignored)
```

- Evidence: `Cargo.toml:2-9`, `ls /Users/lgbarn/Personal/myterm/`

---

### `arcterm-core/` — Shared Types

```
arcterm-core/
├── Cargo.toml    # no workspace deps; only std + serde (if any)
└── src/
    ├── lib.rs    # re-exports: Cell, CellAttrs, Color, Grid, GridSize,
    │             #             CursorPos, TermModes, InputEvent, KeyCode, Modifiers
    ├── cell.rs   # Cell struct, CellAttrs (SGR flags), Color enum
    ├── grid.rs   # Grid (2D Vec<Cell> + VecDeque scrollback), TermModes, CursorPos
    └── input.rs  # InputEvent, KeyCode, Modifiers — platform-agnostic input types
```

**Public interface**: everything re-exported from `lib.rs` is the crate's API surface. No binary, no integration tests. Downstream crates import `arcterm-core` and use these types directly.

- Evidence: `arcterm-core/src/lib.rs:1-9`

---

### `arcterm-vt/` — VT Parser and Terminal State Machine

```
arcterm-vt/
├── Cargo.toml    # deps: vte, arcterm-core
└── src/
    ├── lib.rs       # re-exports + inline unit tests for the handler
    ├── processor.rs # ApcScanner (wraps vte::Parser), Processor type
    ├── handler.rs   # Handler (implements vte::Perform), GridState
    │               # ContentType, StructuredContentAccumulator
    └── kitty.rs    # KittyAction, KittyChunkAssembler, KittyCommand,
                    # KittyFormat, parse_kitty_command
```

**Key design**: the `ApcScanner` splits the byte stream — APC sequences (Kitty graphics) go to a side-channel; everything else feeds the VT handler. `GridState` holds the live `Grid` plus drain queues for: Kitty image payloads, OSC 7770 blocks, tool queries/calls, context queries, OSC 133 exit codes. The `lib.rs` contains a large inline test module (`handler_tests`) covering grid mutation, cursor movement, ANSI sequences, and OSC 7770 handling.

- Evidence: `arcterm-vt/src/lib.rs:1-11`

---

### `arcterm-pty/` — PTY I/O

```
arcterm-pty/
├── Cargo.toml    # deps: portable-pty, tokio, libc, arcterm-core
└── src/
    ├── lib.rs      # pub mod session; pub use session::{PtyError, PtySession}
    └── session.rs  # PtySession, PtyError, platform CWD helpers (macOS/Linux)
```

**Key design**: `PtySession::new()` returns `(session, mpsc::Receiver<Vec<u8>>)` — the receiver is deliberately not stored inside `PtySession` so the application layer owns and polls it. A named OS thread (`"pty-reader"`) performs the blocking PTY read loop. CWD lookup is `#[cfg(target_os)]` conditional.

- Evidence: `arcterm-pty/src/session.rs:86-196`

---

### `arcterm-render/` — GPU Renderer

```
arcterm-render/
├── Cargo.toml    # deps: wgpu, winit, glyphon, syntect, image, arcterm-core
├── examples/
│   └── window.rs # Standalone renderer demo (can run without PTY)
└── src/
    ├── lib.rs        # re-exports all public types
    ├── gpu.rs        # GpuState: wgpu device/queue/surface/config init
    ├── quad.rs       # QuadRenderer, QuadInstance — instanced colored rectangles
    ├── text.rs       # TextRenderer, ClipRect, PluginStyledLine — glyphon text
    ├── image_quad.rs # ImageQuadRenderer, ImageTexture, ImageVertex — Kitty images
    ├── palette.rs    # RenderPalette: named color scheme (sRGB values)
    ├── structured.rs # HighlightEngine, RenderedLine, StructuredBlock, StyledSpan
    │               # (syntect syntax highlighting for code blocks / AI output)
    └── renderer.rs   # Renderer: top-level composition; PaneRenderInfo,
                    # PluginPaneRenderInfo, OverlayQuad, build_quad_instances_at,
                    # render_tab_bar_quads, tab_bar_height
```

**Key design**: `Renderer` is constructed from a `winit::Window` (`Arc<Window>`) and owns all GPU resources. It has no knowledge of PTY state — callers pass `PaneRenderInfo { grid, rect, structured_blocks }` slices. The `examples/window.rs` file proves the renderer can be exercised without the full application stack.

- Evidence: `arcterm-render/src/lib.rs`, `arcterm-render/src/renderer.rs:60-100`

---

### `arcterm-plugin/` — WASM Plugin System

```
arcterm-plugin/
├── Cargo.toml    # deps: wasmtime, wasmtime-wasi, tokio, arcterm-core, serde, dirs
├── wit/
│   └── arcterm.wit  # WIT world definition: host imports + guest exports
└── src/
    ├── lib.rs      # pub mod declarations
    ├── host.rs     # PluginHostData (Store<T> data), wasmtime bindgen! macro,
    │              # ArctermPluginImports impl (log, render_text, subscribe_event,
    │              # get_config, register_mcp_tool)
    ├── manager.rs  # PluginManager, PluginEvent enum, PluginId type, DrawBuffer,
    │              # event bus (broadcast::Sender), per-plugin listener tasks
    ├── manifest.rs # PluginManifest (plugin.toml schema), Permissions, PaneAccess,
    │              # build_wasi_ctx (constructs sandboxed WasiCtx from permissions)
    ├── runtime.rs  # PluginRuntime (wasmtime Engine + Linker), PluginInstance,
    │              # load_plugin_with_wasi, call_update, call_render
    └── types.rs    # Re-exports or supplemental types for the plugin system
tests/
└── runtime_test.rs  # Integration tests for plugin loading
```

**WIT interface** (`arcterm-plugin/wit/arcterm.wit`):
- Host imports: `log`, `render-text`, `subscribe-event`, `get-config`, `register-mcp-tool`
- Guest exports: `load()`, `update(plugin-event) → bool`, `render() → list<styled-line>`
- Plugin installed to: `~/.config/arcterm/plugins/<name>/` (directories scanned on startup)

- Evidence: `arcterm-plugin/wit/arcterm.wit`, `arcterm-plugin/src/manager.rs:138-165`

---

### `arcterm-app/` — Application Binary

```
arcterm-app/
├── Cargo.toml    # deps: all workspace crates + winit, clap, tokio, notify, dirs
├── build.rs      # Build script (e.g. embed version/git hash) [Inferred from presence]
└── src/
    ├── main.rs       # Binary entry point, CLI parsing (clap derive), AppState,
    │               # App (ApplicationHandler impl), event loop, startup/session restore
    │               # ~1300+ lines; contains inline doc-test checklists for each phase
    ├── terminal.rs   # Terminal struct: composes PtySession + ApcScanner + GridState
    │               # + KittyChunkAssembler; exposes drain methods
    ├── layout.rs     # PaneNode (recursive binary tree), PaneId, PixelRect,
    │               # Direction, Axis, BorderQuad — layout engine with full tests
    ├── tab.rs        # TabManager: per-tab focus PaneId, tab list, zoom state
    ├── config.rs     # ArctermConfig: TOML config loading, overlay merging,
    │               # fs-watcher integration (notify crate)
    ├── workspace.rs  # WorkspaceFile, WorkspacePaneNode: TOML serialize/deserialize,
    │               # session save/restore, workspaces_dir(), list_workspaces()
    ├── keymap.rs     # KeymapHandler, KeyAction — leader-key state machine
    ├── input.rs      # Input event translation (winit → arcterm-core InputEvent)
    ├── context.rs    # PaneContext, ErrorContext, SiblingContext; OSC 7770 formatters
    ├── ai_detect.rs  # AiAgentKind, AutoDetector — process-name heuristic detection
    ├── detect.rs     # AutoDetector wrapper (TTL-cached detection calls)
    ├── neovim.rs     # NeovimState — Neovim process detection with 2s TTL cache
    ├── proc.rs       # process_comm(), process_args() — OS-level process inspection
    ├── colors.rs     # ANSI color → RGB conversion helpers
    ├── palette.rs    # PaletteState, WorkspaceSwitcherState — command palette UI
    ├── overlay.rs    # OverlayReviewState — config overlay review UI
    ├── search.rs     # SearchOverlayState — cross-pane search UI
    ├── selection.rs  # Selection, SelectionMode, Clipboard, SelectionQuad,
    │               # generate_selection_quads, pixel_to_cell
    ├── plan.rs       # PlanStripState, PlanViewState — shipyard plan status bar
    └── overlay.rs    # (overlaps with overlay.rs above) [same file, re-checked]
```

**Module organization pattern**: each UI concern, feature, or subsystem gets its own module. `main.rs` declares all modules via `mod` and contains `AppState` and the `App`/`ApplicationHandler` impl. No sub-directories under `src/` — all modules are flat files.

- Evidence: `arcterm-app/src/main.rs:176-193` (mod declarations)

---

### Configuration File Locations

| File/Directory | Purpose |
|---|---|
| `~/.config/arcterm/` | User config root (`dirs::config_dir()`) |
| `~/.config/arcterm/workspaces/<name>.toml` | Named workspace files |
| `~/.config/arcterm/workspaces/_last_session.toml` | Auto-saved session (consumed on next launch) |
| `~/.config/arcterm/plugins/<name>/` | Installed plugin directories |
| `~/.config/arcterm/plugins/<name>/plugin.toml` | Plugin manifest |
| `~/.config/arcterm/plugins/<name>/<name>.wasm` | Plugin WASM binary |
| `examples/config/base.toml` | Example base configuration |
| `examples/config/overlay-*.toml` | Example overlay configuration |

- Evidence: `arcterm-app/src/main.rs:488-509`, `arcterm-plugin/src/manager.rs:151-157`

---

### Entry Points

| Entry Point | Location | Description |
|---|---|---|
| `arcterm-app` binary | `arcterm-app/src/main.rs:fn main()` | Primary user-facing terminal |
| Renderer demo | `arcterm-render/examples/window.rs` | Standalone GPU renderer test |
| Plugin examples | `examples/plugins/hello-world/src/lib.rs` | Minimal plugin template |
| Plugin examples | `examples/plugins/system-monitor/src/lib.rs` | System info plugin template |

---

### Module Dependency Graph (arcterm-app internal)

```
main.rs (AppState, App)
  ├── terminal.rs     → arcterm-pty, arcterm-vt
  ├── layout.rs       (self-contained, no crate deps)
  ├── tab.rs          → layout.rs (PaneId)
  ├── context.rs      → ai_detect.rs, layout.rs, terminal.rs
  ├── ai_detect.rs    → proc.rs
  ├── detect.rs       → ai_detect.rs
  ├── neovim.rs       → proc.rs
  ├── proc.rs         (OS syscalls, no crate deps)
  ├── workspace.rs    → layout.rs
  ├── keymap.rs       → layout.rs
  ├── input.rs        → arcterm-core (InputEvent)
  ├── selection.rs    → layout.rs
  ├── palette.rs      → workspace.rs
  ├── search.rs       → layout.rs
  ├── plan.rs         (reads .shipyard/ filesystem)
  ├── overlay.rs      → config.rs
  ├── colors.rs       (pure conversion, no deps)
  └── config.rs       (TOML file I/O, notify fs-watch)
```

---

## Summary Table

| Item | Detail | Confidence |
|---|---|---|
| Workspace root | `/Users/lgbarn/Personal/myterm/` | Observed |
| Number of crates | 6 (arcterm-core, -vt, -pty, -render, -plugin, -app) | Observed |
| Binary crate | `arcterm-app` | Observed |
| Library crates | `arcterm-core`, `-vt`, `-pty`, `-render`, `-plugin` | Observed |
| Rust edition | 2024 (`Cargo.toml:13`) | Observed |
| Config root | `~/.config/arcterm/` via `dirs::config_dir()` | Observed |
| Plugin install dir | `~/.config/arcterm/plugins/<name>/` | Observed |
| WIT file | `arcterm-plugin/wit/arcterm.wit` | Observed |
| Example plugins | `examples/plugins/hello-world/`, `examples/plugins/system-monitor/` | Observed |
| Module layout | Flat modules in `src/`, no subdirectories | Observed |
| `app/src` module count | ~20 modules in `arcterm-app/src/` | Observed |
| Build script | `arcterm-app/build.rs` | Observed (purpose inferred) |

---

## Open Questions

- `arcterm-app/build.rs` contents — not read; purpose unknown (likely embeds version info or generates code).
- Whether `arcterm-app/src/overlay.rs` and `arcterm-app/src/plan.rs` have been split into submodules in later commits — the flat list was inferred from the `mod` declarations in `main.rs`.
- `arcterm-render/src/palette.rs` color scheme enumeration — the default was changed to `cool-night` (Ghostty theme) per git log `dc0717c`; full list of supported palettes not read.
