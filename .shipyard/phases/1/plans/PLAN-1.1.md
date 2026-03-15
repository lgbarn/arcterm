---
phase: foundation
plan: "1.1"
wave: 1
dependencies: []
must_haves:
  - Cargo workspace compiles with all five crates
  - arcterm-core exports Cell, Color, CursorPos, GridSize, InputEvent types
  - cargo test passes for arcterm-core
files_touched:
  - Cargo.toml
  - arcterm-core/Cargo.toml
  - arcterm-core/src/lib.rs
  - arcterm-core/src/cell.rs
  - arcterm-core/src/grid.rs
  - arcterm-core/src/input.rs
  - arcterm-vt/Cargo.toml
  - arcterm-vt/src/lib.rs
  - arcterm-pty/Cargo.toml
  - arcterm-pty/src/lib.rs
  - arcterm-render/Cargo.toml
  - arcterm-render/src/lib.rs
  - arcterm-app/Cargo.toml
  - arcterm-app/src/main.rs
  - .gitignore
  - rust-toolchain.toml
tdd: true
---

# Plan 1.1 -- Workspace Scaffold and Core Types

**Wave 1** | No dependencies | Foundation for all subsequent plans

## Goal

Create the Cargo workspace with all five crates compiling (stub crates for vt, pty, render, app) and a fully implemented `arcterm-core` crate containing the shared type system that every other crate depends on.

---

<task id="1" files="Cargo.toml, .gitignore, rust-toolchain.toml, arcterm-vt/Cargo.toml, arcterm-vt/src/lib.rs, arcterm-pty/Cargo.toml, arcterm-pty/src/lib.rs, arcterm-render/Cargo.toml, arcterm-render/src/lib.rs, arcterm-app/Cargo.toml, arcterm-app/src/main.rs" tdd="false">
  <action>
    Create the root `Cargo.toml` workspace manifest listing five members: `arcterm-core`, `arcterm-vt`, `arcterm-pty`, `arcterm-render`, `arcterm-app`. Set workspace-level `[workspace.package]` with edition = "2024", version = "0.1.0", license = "MIT". Create `rust-toolchain.toml` pinning to stable channel (1.85+, needed for edition 2024). Create `.gitignore` with `/target` and `Cargo.lock` excluded from ignore (lock file should be committed for binary projects).

    Create stub crates for `arcterm-vt`, `arcterm-pty`, `arcterm-render`, and `arcterm-app`:
    - Each has its own `Cargo.toml` inheriting `package.edition` and `package.version` from workspace.
    - `arcterm-vt/src/lib.rs`: empty lib with a doc comment.
    - `arcterm-pty/src/lib.rs`: empty lib with a doc comment.
    - `arcterm-render/src/lib.rs`: empty lib with a doc comment.
    - `arcterm-app/src/main.rs`: `fn main() { println!("arcterm"); }` placeholder.

    Add external dependency versions in `[workspace.dependencies]`:
    ```toml
    vte = "0.15"
    portable-pty = "0.9"
    wgpu = "28"
    winit = "0.30"
    glyphon = "0.9"
    tokio = { version = "1", features = ["full"] }
    pollster = "0.4"
    log = "0.4"
    env_logger = "0.11"
    ```

    Each stub crate should list its known dependencies (from the crate dependency graph) in its `Cargo.toml` using `workspace = true` references, but the source files remain stubs. This ensures dependency resolution is validated now, not later.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo check --workspace 2>&1 | tail -5</verify>
  <done>`cargo check --workspace` succeeds with zero errors. All five crates are listed in `cargo metadata --no-deps --format-version 1 | jq '.packages[].name'`.</done>
</task>

<task id="2" files="arcterm-core/Cargo.toml, arcterm-core/src/lib.rs, arcterm-core/src/cell.rs, arcterm-core/src/grid.rs, arcterm-core/src/input.rs" tdd="true">
  <action>
    Implement `arcterm-core` with the following types. Write tests first for each type.

    **`cell.rs`:**
    - `Color` enum: `Default`, `Indexed(u8)`, `Rgb(u8, u8, u8)`. Derive `Clone, Copy, Debug, PartialEq, Eq, Default`.
    - `CellAttrs` struct: `fg: Color`, `bg: Color`, `bold: bool`, `italic: bool`, `underline: bool`, `reverse: bool`. Derive `Clone, Copy, Debug, PartialEq, Eq, Default`.
    - `Cell` struct: `c: char` (default ' '), `attrs: CellAttrs`, `dirty: bool` (default true). Derive `Clone, Debug, PartialEq`.
    - Implement `Default` for `Cell` with space character and default attrs, dirty=true.
    - Implement `Cell::reset(&mut self)` that resets to defaults and marks dirty.
    - Implement `Cell::set_char(&mut self, c: char)` that sets char and marks dirty.

    **`grid.rs`:**
    - `CursorPos` struct: `row: usize`, `col: usize`. Derive `Clone, Copy, Debug, PartialEq, Eq, Default`.
    - `GridSize` struct: `rows: usize`, `cols: usize`. With `new(rows, cols)` constructor.
    - `Grid` struct: `cells: Vec<Vec<Cell>>`, `size: GridSize`, `cursor: CursorPos`, `dirty: bool`.
    - `Grid::new(size: GridSize) -> Grid` -- allocates rows x cols cells with defaults, cursor at (0,0), dirty=true.
    - `Grid::cell(&self, row: usize, col: usize) -> &Cell` -- bounds-checked access.
    - `Grid::cell_mut(&mut self, row: usize, col: usize) -> &mut Cell` -- bounds-checked mutable access, marks grid dirty.
    - `Grid::resize(&mut self, new_size: GridSize)` -- resizes grid, preserving existing content where possible, fills new cells with defaults.
    - `Grid::clear(&mut self)` -- resets all cells and cursor to defaults, marks dirty.
    - `Grid::mark_clean(&mut self)` -- sets dirty=false on grid and all cells.
    - `Grid::rows(&self) -> &[Vec<Cell>]` -- returns slice of all rows.

    **`input.rs`:**
    - `InputEvent` enum: `Key(KeyCode, Modifiers)`, `Resize(GridSize)`, `Paste(String)`.
    - `KeyCode` enum: `Char(char)`, `Enter`, `Backspace`, `Tab`, `Escape`, `Up`, `Down`, `Left`, `Right`, `Home`, `End`, `PageUp`, `PageDown`, `Delete`, `F(u8)`.
    - `Modifiers` struct with bitflags: `SHIFT`, `CTRL`, `ALT`, `SUPER`. Use a simple u8 bitmask (no external bitflags crate needed for Phase 1).
    - `Modifiers::none()`, `Modifiers::ctrl()`, `Modifiers::shift()`, `Modifiers::alt()` constructors.
    - `Modifiers::has_ctrl(&self) -> bool`, `has_shift`, `has_alt`, `has_super` query methods.

    **`lib.rs`:** Re-export all public types from submodules.

    **Tests (in each module or a tests/ directory):**
    - Cell: default is space, set_char marks dirty, reset restores defaults.
    - Grid: new() creates correct dimensions, cell access works, resize preserves content, clear resets all.
    - Modifiers: bitflag composition and query methods work correctly.
    - GridSize: construction and field access.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-core 2>&1 | tail -10</verify>
  <done>All arcterm-core tests pass. `cargo doc --package arcterm-core --no-deps` generates documentation without warnings. Types are importable as `arcterm_core::Cell`, `arcterm_core::Grid`, etc.</done>
</task>

<task id="3" files="Cargo.toml, arcterm-vt/Cargo.toml, arcterm-pty/Cargo.toml, arcterm-render/Cargo.toml, arcterm-app/Cargo.toml" tdd="false">
  <action>
    Verify the full workspace dependency graph resolves correctly with all external crates. Each stub crate should have its dependencies declared (even though the source is a stub):

    - `arcterm-vt/Cargo.toml`: depends on `vte.workspace = true` and `arcterm-core = { path = "../arcterm-core" }`.
    - `arcterm-pty/Cargo.toml`: depends on `portable-pty.workspace = true`, `tokio.workspace = true`, and `arcterm-core = { path = "../arcterm-core" }`.
    - `arcterm-render/Cargo.toml`: depends on `wgpu.workspace = true`, `winit.workspace = true`, `glyphon.workspace = true`, `pollster.workspace = true`, `log.workspace = true`, and `arcterm-core = { path = "../arcterm-core" }`.
    - `arcterm-app/Cargo.toml`: depends on `arcterm-core`, `arcterm-vt`, `arcterm-pty`, `arcterm-render`, `tokio.workspace = true`, `winit.workspace = true`, `log.workspace = true`, `env_logger.workspace = true`.

    Run `cargo build --workspace` to verify all external crates download and compile. This surfaces any version incompatibilities between glyphon, wgpu, and cosmic-text before Wave 2 begins.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --workspace 2>&1 | tail -5</verify>
  <done>`cargo build --workspace` succeeds. `cargo tree --workspace` shows no duplicate versions of wgpu, winit, or cosmic-text (no version conflicts). The binary `target/debug/arcterm-app` exists and runs (prints "arcterm").</done>
</task>
