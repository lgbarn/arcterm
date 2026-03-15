# REVIEW-1.1 — Workspace Scaffold and Core Types

**Reviewer:** Claude Code (Senior Review Agent)
**Date:** 2026-03-15
**Plan:** PLAN-1.1
**Verdict:** PASS — MINOR_ISSUES

---

## Stage 1: Spec Compliance

**Verdict:** PASS

### Task 1: Create Cargo Workspace

- Status: PASS
- Evidence: `/Users/lgbarn/Personal/myterm/Cargo.toml` contains all five members (`arcterm-core`, `arcterm-vt`, `arcterm-pty`, `arcterm-render`, `arcterm-app`). `[workspace.package]` sets `edition = "2024"`, `version = "0.1.0"`, `license = "MIT"`. `rust-toolchain.toml` pins `channel = "stable"`. `.gitignore` contains `/target` and does not list `Cargo.lock` (lock file is tracked). `cargo check --workspace` passes. `cargo metadata --no-deps` lists all five crate names. Stub `src/lib.rs` files exist with doc comments for vt/pty/render. `arcterm-app/src/main.rs` prints "arcterm" and the binary executes.
- Notes: The plan lists `glyphon = "0.9"` in workspace dependencies; the implementation uses `0.10`. This deviation is fully documented in the SUMMARY under Decisions Made, with valid technical justification (wgpu 28 compatibility). The change is correct — see Task 3 notes below.

### Task 2: Implement arcterm-core Types (TDD)

- Status: PASS
- Evidence: All specified types are present and correctly derived:
  - `Color` enum with `Default`, `Indexed(u8)`, `Rgb(u8, u8, u8)` — derives `Clone, Copy, Debug, PartialEq, Eq, Default` at `/Users/lgbarn/Personal/myterm/arcterm-core/src/cell.rs:4-13`.
  - `CellAttrs` struct with all six fields at `cell.rs:16-24`. Derives correct.
  - `Cell` struct at `cell.rs:27-32`. `Default` impl at `cell.rs:34-42`. `reset()` and `set_char()` at `cell.rs:44-54`.
  - `CursorPos` and `GridSize` structs with `GridSize::new()` at `grid.rs:6-23`.
  - `Grid` with all seven specified methods at `grid.rs:33-109`.
  - `InputEvent`, `KeyCode`, `Modifiers` at `input.rs:6-83`. `BitOr` impl present.
  - `lib.rs` re-exports all public types as `arcterm_core::Cell`, `arcterm_core::Grid`, etc.
- Test coverage: 28 tests across three modules (6 cell, 11 grid, 11 input). All pass. Tests cover all scenarios specified in the plan: cell default/set_char/reset, grid dimensions/access/resize/clear/mark_clean, modifier bitflag composition and queries. `cargo doc --package arcterm-core --no-deps` generates with zero warnings.
- Notes: The plan specifies `has_ctrl(&self)` with a shared reference receiver; the implementation uses `has_ctrl(self)` with copy semantics (`Modifiers` is `Copy`). This is a strictly better choice and not a deviation.

### Task 3: Verify Dependency Resolution

- Status: PASS
- Evidence: `cargo build --workspace` finishes cleanly. `target/debug/arcterm-app` exists and outputs "arcterm". Dependency tree shows single versions of `wgpu v28.0.0`, `winit v0.30.13`, `glyphon v0.10.0`, `cosmic-text v0.15.0`. All stub crate `Cargo.toml` files declare their exact specified dependencies using `workspace = true` references.
- Notes: `bitflags v1.3.2` and `bitflags v2.11.0` both appear in the dependency tree, but they arrive transitively from `winit` (via `core-graphics`) and `wgpu`/`cosmic-text` respectively. This is an ecosystem artifact, not a build error, and the plan's done criteria specifically mentions wgpu, winit, and cosmic-text — not bitflags.

---

## Stage 2: Code Quality

### Critical

None.

### Important

- **`rust-toolchain.toml` does not pin a specific version** at `/Users/lgbarn/Personal/myterm/rust-toolchain.toml:2`.
  - The plan states "pinning to stable channel (1.85+, needed for edition 2024)." The file contains only `channel = "stable"` with no `version` key. The current compiler is 1.92.0 (Homebrew). Without a pinned version, a future `rustup update` or a different developer's toolchain could silently change compiler behavior — edition 2024 has known behavioral differences from 1.85 in several areas.
  - Remediation: Add `channel = "1.92.0"` (or whichever specific stable version the team standardizes on) to `rust-toolchain.toml`. Example:
    ```toml
    [toolchain]
    channel = "1.92.0"
    ```

- **`Grid::cell()` and `Grid::cell_mut()` panic on out-of-bounds access** at `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs:48-56`.
  - The spec says "bounds-checked access," but both methods delegate directly to `Vec` indexing (`self.cells[row][col]`), which panics with an index-out-of-bounds. The spec does not explicitly require `Option` or `Result` return types, so a panic is a defensible choice — however, it means callers (arcterm-vt in particular) must validate coordinates before calling, with no compiler enforcement. A panic in the VT parser on a malformed escape sequence would crash the process.
  - Remediation: Either (a) return `Option<&Cell>` / `Option<&mut Cell>` and let callers decide how to handle, or (b) add an explicit bounds check with a descriptive `panic!` message so debugging is easier. Option (a) is preferred since VT parsers regularly encounter malformed sequences. This decision should be made before arcterm-vt consumes these methods in Wave 2.

- **`Modifiers` constants are `pub u8` rather than `pub const Modifiers`** at `/Users/lgbarn/Personal/myterm/arcterm-core/src/input.rs:40-43`.
  - `Modifiers::SHIFT`, `CTRL`, `ALT`, `SUPER` are exposed as raw `u8` values. Downstream code can write `Modifiers(Modifiers::SHIFT | Modifiers::CTRL)` directly using the inner `u8`, bypassing the `BitOr` impl. This leaks the representation and creates two APIs for the same operation.
  - Remediation: Make the constants `pub(crate)` or `pub(super)`, or convert them to `pub const` of type `Modifiers`:
    ```rust
    pub const SHIFT: Modifiers = Modifiers(0b0001);
    pub const CTRL: Modifiers  = Modifiers(0b0010);
    pub const ALT: Modifiers   = Modifiers(0b0100);
    pub const SUPER: Modifiers = Modifiers(0b1000);
    ```
    This allows `Modifiers::SHIFT | Modifiers::CTRL` via the existing `BitOr` impl and removes the raw `u8` exposure.

- **`Grid` does not derive or implement `Debug`, `Clone`, or `PartialEq`** at `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs:26`.
  - The spec does not explicitly require these derives on `Grid`, but all fields (`Vec<Vec<Cell>>`, `GridSize`, `CursorPos`, `bool`) are `Debug` + `Clone` + `PartialEq`. Without these, test assertions on `Grid` values require field-by-field comparison, and downstream consumers cannot easily log or snapshot grid state. The existing test at `grid.rs:190` manually checks `g.cursor` rather than the whole struct.
  - Remediation: Add `#[derive(Debug, Clone, PartialEq)]` above `pub struct Grid`. This is a non-breaking addition.

### Suggestions

- **`Cell::reset()` re-runs `Default` allocation on every call** at `/Users/lgbarn/Personal/myterm/arcterm-core/src/cell.rs:46-48`.
  - The implementation uses `*self = Cell::default()`. For `Cell` this is fine (all fields are primitives, no heap allocation). Leaving this note as confirmation that the approach is correct and intentional, not an oversight.

- **No test for `Grid::resize()` with a zero-dimension** at `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs`.
  - Resizing to `GridSize::new(0, 0)` is valid Rust (produces an empty `Vec`) but produces a grid where any `cell()` call panics. The cursor clamp uses `saturating_sub(1)` which returns 0 for a 0-row grid, leaving `cursor.row = 0` pointing outside the empty grid. A test documenting the expected behavior (or a guard rejecting zero dimensions in `GridSize::new`) would prevent surprises in the resize path.

- **`arcterm-app/Cargo.toml` does not declare `arcterm-core` via `workspace = true`** at `/Users/lgbarn/Personal/myterm/arcterm-app/Cargo.toml:12`.
  - All four workspace crates (`arcterm-vt`, `arcterm-pty`, `arcterm-render`, `arcterm-app`) declare `arcterm-core` as a path dependency directly: `arcterm-core = { path = "../arcterm-core" }`. The root `Cargo.toml` already lists `arcterm-core = { path = "arcterm-core" }` in `[workspace.dependencies]`, so these could use `arcterm-core.workspace = true`. This is a consistency issue, not a functional one — both forms resolve to the same crate. Worth standardizing before the workspace grows.

---

## Summary

**Verdict:** PASS — MINOR_ISSUES

All three plan tasks are correctly implemented. The workspace compiles cleanly, all 28 tests pass, the binary produces the expected output, and the glyphon version deviation is technically correct and properly documented. The implementation is clean, idiomatic Rust with good test coverage and zero doc warnings.

The most important finding to resolve before Wave 2 begins is the `Grid::cell()` / `Grid::cell_mut()` panicking interface: the arcterm-vt parser will be the primary consumer of these methods and must handle malformed terminal sequences without crashing.

**Critical:** 0 | **Important:** 4 | **Suggestions:** 3
