# CONVENTIONS.md

## Overview
Arcterm is a Rust workspace with strong, consistent conventions throughout. All six crates share a common pattern: module-level `//!` doc comments, `///` doc comments on every public item, `snake_case` naming, `anyhow` or domain-specific error enums for error handling, and `#[serde(default)]` on all config structs. The codebase reads as though it was written by a single author or enforced through careful review.

---

## Findings

### Naming Conventions

- **Types, enums, traits**: `PascalCase`
  - Evidence: `arcterm-core/src/cell.rs:5` — `pub enum Color`, `pub struct CellAttrs`, `pub struct Cell`
  - Evidence: `arcterm-plugin/src/manager.rs:39` — `pub enum PluginEvent`, `pub type PluginId`, `pub type DrawBuffer`

- **Functions, methods, variables, modules, files**: `snake_case`
  - Evidence: `arcterm-core/src/grid.rs:123` — `pub fn cell_opt`, `pub fn cell_mut`, `pub fn set_cursor`
  - Evidence: file naming: `cell.rs`, `grid.rs`, `input.rs`, `ai_detect.rs`, `neovim.rs`

- **Constants**: `SCREAMING_SNAKE_CASE`
  - Evidence: `arcterm-plugin/src/manifest.rs:14` — `pub const SUPPORTED_API_VERSION: &str = "0.1";`

- **Type aliases**: `PascalCase`
  - Evidence: `arcterm-plugin/src/manager.rs:102,112` — `pub type PluginId = String;`, `pub type DrawBuffer = Arc<Mutex<...>>;`

### Module and File Organization

- **Crate lib.rs pattern**: Every library crate follows: `//!` module-level doc → `pub mod` declarations → `pub use` re-exports
  - Evidence: `arcterm-core/src/lib.rs`:
    ```rust
    //! arcterm-core — shared types for the arcterm terminal emulator.

    pub mod cell;
    pub mod grid;
    pub mod input;

    pub use cell::{Cell, CellAttrs, Color};
    pub use grid::{CursorPos, Grid, GridSize, TermModes};
    pub use input::{InputEvent, KeyCode, Modifiers};
    ```
  - Evidence: `arcterm-vt/src/lib.rs:1-10` — same pattern
  - Evidence: `arcterm-plugin/src/lib.rs` — same pattern

- **Visual section separators**: Long dash-lines separate logical sections within a file. Two styles used:
  - Evidence: `arcterm-app/src/config.rs:14` — `// ---------------------------------------------------------------------------`
  - Evidence: `arcterm-plugin/src/manager.rs:22` — `// ──────────────────────────────────────────────────────────────────`
  - [Inferred] Both styles appear to be interchangeable; no single canonical style enforced

- **Public fields, no getter/setter boilerplate**: Data structs expose fields directly
  - Evidence: `arcterm-core/src/cell.rs:17-24` — `pub struct CellAttrs { pub fg: Color, pub bg: Color, pub bold: bool, ... }`
  - Evidence: `arcterm-core/src/grid.rs:67-95` — all `Grid` fields are `pub`

### Documentation Conventions

- **Module-level `//!` on every file**: Every source file opens with a `//!` crate/module doc comment
  - Evidence: `arcterm-core/src/cell.rs:1` — `//! Terminal cell types.`
  - Evidence: `arcterm-core/src/grid.rs:1` — `//! Terminal grid and cursor types.`
  - Evidence: `arcterm-plugin/src/manifest.rs:1` — `//! Plugin manifest parsing and permission enforcement.`
  - Evidence: `arcterm-app/src/config.rs:1-8` — multi-line `//!` with full usage description

- **`///` doc comments on every public item**: Types, methods, fields, constants all documented
  - Evidence: `arcterm-core/src/grid.rs:122-136`:
    ```rust
    /// Immutable cell access. Returns None on out-of-bounds.
    pub fn cell_opt(&self, row: usize, col: usize) -> Option<&Cell> { ... }

    /// Immutable cell access (panics on out-of-bounds — kept for existing tests).
    pub fn cell(&self, row: usize, col: usize) -> &Cell { ... }

    /// Mutable cell access; marks the grid dirty.
    pub fn cell_mut(&mut self, row: usize, col: usize) -> &mut Cell { ... }
    ```

- **Inline `//` comments explain non-obvious logic**: Algorithmic reasoning documented inline
  - Evidence: `arcterm-core/src/grid.rs:330-354` — extensive inline comments explaining scrollback viewport ordering

- **`// SAFETY:` blocks for all unsafe code**
  - Evidence: `arcterm-pty/src/session.rs:44-46`:
    ```rust
    // SAFETY: vnode_pathinfo is a plain C struct with no invariants beyond
    // being zeroed before passing to proc_pidinfo.
    let mut info: libc::proc_vnodepathinfo = unsafe { mem::zeroed() };
    ```

### Derive Conventions

- **Value types**: `#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]` — consistent derive set
  - Evidence: `arcterm-core/src/cell.rs:4` — `#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]` on `Color`
  - Evidence: `arcterm-core/src/cell.rs:16` — same set on `CellAttrs`
  - Evidence: `arcterm-core/src/grid.rs:46,53,66` — same pattern on `CursorPos`, `GridSize`, `Grid`

- **Config/serde types**: `#[derive(Debug, Clone, Deserialize, Serialize)]` + `#[serde(default)]`
  - Evidence: `arcterm-app/src/config.rs:20-21`:
    ```rust
    #[derive(Debug, Clone, Deserialize, Serialize)]
    #[serde(default)]
    pub struct ArctermConfig { ... }
    ```
  - All sub-config structs follow the same pattern (lines 74, 102, 130)

- **`#[default]` on enum variants**: Used on default enum variants
  - Evidence: `arcterm-core/src/cell.rs:7-8` — `#[default] Default` on `Color::Default`
  - Evidence: `arcterm-plugin/src/manifest.rs:21-22` — `#[default] None` on `PaneAccess::None`

### Error Handling

- **Domain-specific error enum for arcterm-pty**: Full `Error` trait impl
  - Evidence: `arcterm-pty/src/session.rs:10-33`:
    ```rust
    pub enum PtyError {
        SpawnFailed(String),
        IoError(io::Error),
        ResizeFailed(String),
    }
    impl From<io::Error> for PtyError { ... }
    impl std::fmt::Display for PtyError { ... }
    impl std::error::Error for PtyError {}
    ```

- **`anyhow::Result` for plugin subsystem**: Manager and runtime functions return `anyhow::Result<T>`
  - Evidence: `arcterm-plugin/src/manager.rs:151,189,232` — `pub fn new() -> anyhow::Result<Self>`, `pub fn install(...) -> anyhow::Result<PluginId>`, etc.

- **Graceful degradation for config load**: Falls back to defaults; logs warn but does not panic
  - Evidence: `arcterm-app/src/config.rs:175-201` — `Ok(cfg)` on success, `Self::default()` on any error, `log::warn!` for non-fatal I/O errors

- **Logging levels consistently applied**:
  - `log::info!` — successful lifecycle events (plugin loaded, config loaded)
  - `log::warn!` — recoverable errors (invalid config TOML, failed watcher setup, bad overlay path)
  - `log::error!` — severe errors (poisoned mutex, task panic)
  - `log::debug!` — trace/diagnostic info (skipped paths, bus close)
  - Evidence: `arcterm-plugin/src/manager.rs:269,312,499-501,517-519`

### Feature Flags

- **One feature flag in the binary crate**:
  - Evidence: `arcterm-app/Cargo.toml:15-17`:
    ```toml
    [features]
    latency-trace = []
    ```
  - Purpose: enables fine-grained timestamp logging for latency measurement

### Temporary Suppression Conventions

- **`#![allow(dead_code)]` with explanation comment** — used for code awaiting integration:
  - Evidence: `arcterm-app/src/detect.rs:1` — `#![allow(dead_code)] // Wired in PLAN-3.1 integration`
  - Evidence: `arcterm-app/src/workspace.rs:13` — `#![allow(dead_code)]`
  - Evidence: `arcterm-app/src/config.rs:49` — `#[allow(dead_code)] // read by Wave-2 keymap and tab-bar rendering`

- **`TODO(phase-N):` comments** for known deferred work:
  - Evidence: `arcterm-app/src/terminal.rs:37` — `// TODO(phase-5): move PNG/JPEG decoding to a background thread`
  - Evidence: `arcterm-app/src/main.rs:2155` — `// TODO(phase-5): implement proper placement tracking per image_id.`

### Import Conventions

- **Workspace-managed imports**: `use workspace::path` style, imports fully qualified
  - Evidence: `arcterm-render/src/renderer.rs:6-14` — grouped stdlib → workspace crates → local crate modules
  - Standard pattern: `use std::...`, then external crates, then `use crate::...`

### Cargo Aliases

- **`cargo xt`** — runs tests on core/vt/pty packages (`arcterm-core`, `arcterm-vt`, `arcterm-pty`)
- **`cargo xr`** — runs the app binary
- **`cargo xc`** — runs clippy workspace-wide with `-D warnings`
- Evidence: `.cargo/config.toml`:
  ```toml
  [alias]
  xt = "test --package arcterm-core --package arcterm-vt --package arcterm-pty"
  xr = "run --package arcterm-app"
  xc = "clippy --workspace --all-targets -- -D warnings"
  ```

---

## Summary Table

| Convention | Detail | Confidence |
|-----------|--------|------------|
| Type naming | `PascalCase` for all types, `snake_case` for functions/modules | Observed |
| Constants | `SCREAMING_SNAKE_CASE` | Observed |
| Module doc | `//!` comment on every source file | Observed |
| Public API docs | `///` doc on every public item | Observed |
| Value derives | `Clone, Copy, Debug, PartialEq, Eq, Default` | Observed |
| Config derives | `Debug, Clone, Deserialize, Serialize` + `#[serde(default)]` | Observed |
| Error handling — PTY | Domain-specific `PtyError` enum with `From`, `Display`, `Error` | Observed |
| Error handling — plugin | `anyhow::Result` throughout | Observed |
| Config errors | Silent fallback to defaults + `log::warn!` | Observed |
| Unsafe safety docs | `// SAFETY:` on every `unsafe` block | Observed |
| Dead code suppress | `#![allow(dead_code)]` + comment explaining phase | Observed |
| TODO format | `// TODO(phase-N): ...` | Observed |
| Section separators | Long dash lines (`---` or `──`) delimiting logical groups | Observed |
| Clippy enforcement | `-D warnings` on all targets | Observed |
| No rustfmt.toml | Default `rustfmt` settings used | Observed |

---

## Open Questions

- Is there a `deny.toml` (cargo-deny) for supply-chain checking? No evidence found.
- Are there pre-commit hooks enforcing `cargo fmt` / `cargo clippy`? Not configured in the repo; [Inferred] enforced manually via `cargo xc`.
- `arcterm-app/src/workspace.rs` has `#![allow(dead_code)]` with no explanatory comment — unclear if this is intentional long-term suppression or a forgotten TODO.
