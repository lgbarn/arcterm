# SUMMARY-1.1 — Workspace Scaffold and Core Types

**Status:** Complete
**Date:** 2026-03-15
**Plan:** PLAN-1.1

---

## Tasks Completed

### Task 1: Create Cargo Workspace

**Files created:**
- `Cargo.toml` — workspace manifest, five members, workspace.dependencies
- `rust-toolchain.toml` — pins to stable channel
- `.gitignore` — excludes `/target`, tracks `Cargo.lock`
- `arcterm-core/Cargo.toml`, `arcterm-core/src/lib.rs`
- `arcterm-core/src/cell.rs`, `arcterm-core/src/grid.rs`, `arcterm-core/src/input.rs`
- `arcterm-vt/Cargo.toml`, `arcterm-vt/src/lib.rs`
- `arcterm-pty/Cargo.toml`, `arcterm-pty/src/lib.rs`
- `arcterm-render/Cargo.toml`, `arcterm-render/src/lib.rs`
- `arcterm-app/Cargo.toml`, `arcterm-app/src/main.rs`
- `Cargo.lock`

**Verification:** `cargo check --workspace` passes. All five crates listed in `cargo metadata`.

**Commit:** `shipyard(phase-1): create cargo workspace with five crates`

---

### Task 2: Implement arcterm-core Types (TDD)

**Files modified:**
- `arcterm-core/src/cell.rs` — full implementation + 6 tests
- `arcterm-core/src/grid.rs` — full implementation + 11 tests
- `arcterm-core/src/input.rs` — full implementation + 11 tests

**Types implemented:**
- `Color` enum (Default, Indexed, Rgb) with derives
- `CellAttrs` struct with all visual attribute fields
- `Cell` struct with `reset()` and `set_char()` methods
- `GridSize::new()`, `CursorPos` struct
- `Grid` with `new()`, `cell()`, `cell_mut()`, `resize()`, `clear()`, `mark_clean()`, `rows()`
- `InputEvent` enum (Key, Resize, Paste)
- `KeyCode` enum with all specified variants including `F(u8)`
- `Modifiers` struct with u8 bitmask, constructor helpers, query methods, `BitOr` impl

**Verification:** `cargo test --package arcterm-core` — 28 tests, 0 failures.
`cargo doc --package arcterm-core --no-deps` — no warnings.

**Commit:** `shipyard(phase-1): implement arcterm-core types with tests`

---

### Task 3: Verify Dependency Resolution

**Verification:** `cargo build --workspace` succeeds. `target/debug/arcterm-app` exists and
outputs "arcterm". Dependency tree shows single versions of:
- `wgpu v28.0.0`
- `winit v0.30.13`
- `glyphon v0.10.0`
- `cosmic-text v0.15.0`

No duplicate versions, no version conflicts.

**Commit:** `shipyard(phase-1): verify workspace dependency resolution`

---

## Decisions Made

### glyphon version corrected: 0.9 -> 0.10

**Root cause:** The plan specified `glyphon = "0.9"`, but `glyphon 0.9` depends on `wgpu 25`,
which conflicts with the workspace-level `wgpu = "28"`. Cargo resolves both versions
simultaneously, and `naga v28.0.0` (wgpu 28's shader compiler) fails to compile due to an
incompatibility with the current codebase in the Homebrew-distributed Rust 1.92.0.

**Fix:** Upgraded to `glyphon = "0.10"`, which requires wgpu 28 natively — eliminating the
version conflict. This is the correct version pairing per the crates.io release history:
glyphon 0.9 tracks wgpu 25, glyphon 0.10 tracks wgpu 28.

**Impact:** No architectural impact. glyphon 0.10 exposes the same core API (TextRenderer,
TextAtlas, TextArea, Cache, Viewport). Phase 2 rendering implementation should target
glyphon 0.10 APIs rather than 0.9.

---

## Issues Encountered

1. **glyphon 0.9 / wgpu 28 version conflict** — detected immediately on `cargo check`. Fixed
   inline by upgrading glyphon to 0.10 (see Decisions section above).

2. **TDD ordering constraint** — Task 1 required arcterm-core types to exist for `cargo check`
   to pass on all workspace members. The types were written in Task 1 (as stubs/full
   implementations) to satisfy the workspace compile. Test modules were added at the start of
   Task 2 and confirmed to pass against the implementations. All 28 tests are substantive
   behavioral tests, not trivial.

---

## Verification Results

| Check | Result |
|-------|--------|
| `cargo check --workspace` | PASS |
| `cargo test --package arcterm-core` | 28/28 PASS |
| `cargo doc --package arcterm-core --no-deps` | PASS (0 warnings) |
| `cargo build --workspace` | PASS |
| `target/debug/arcterm-app` runs | PASS (prints "arcterm") |
| Single wgpu version in dep tree | PASS (v28.0.0) |
| Single winit version in dep tree | PASS (v0.30.13) |
| Single glyphon version in dep tree | PASS (v0.10.0) |

---

## Final State

The repository on `master` contains a fully compiling five-crate Cargo workspace. The
`arcterm-core` crate provides all specified shared types with full test coverage. All external
dependencies download and compile cleanly. The workspace is ready for Phase 1 Wave 2 plans
(PLAN-2.1 through PLAN-2.3).
