# SUMMARY-1.1: Add alacritty_terminal Dependency and Define Bridge Types

**Plan:** PLAN-1.1
**Branch:** phase-12-engine-swap
**Date:** 2026-03-16
**Status:** Complete — all tasks executed, all acceptance criteria met

---

## Tasks Executed

### Task 1: Add alacritty_terminal to workspace dependencies

**Action taken:**
- Added `alacritty_terminal = "0.25"` to `[workspace.dependencies]` in root `Cargo.toml`
- Added `alacritty_terminal.workspace = true` to `[dependencies]` in `arcterm-app/Cargo.toml`

**Verification result:**
- `cargo check -p arcterm-app` — zero errors, zero new warnings
- `alacritty_terminal v0.25.1` appears in `Cargo.lock`
- No version conflicts with existing workspace dependencies (vte 0.15, base64 0.22, log 0.4)

**Commit:** `976f41e shipyard(phase-12): add alacritty_terminal 0.25 workspace dependency`

---

### Task 2: Relocate ContentType enum to arcterm-render

**Action taken:**
1. Defined new `ContentType` enum in `arcterm-render/src/structured.rs` with all 8 variants: `CodeBlock`, `Diff`, `Plan`, `Markdown`, `Json`, `Error`, `Progress`, `Image`. Added `#[derive(Debug, Clone, PartialEq, Eq)]` and `///` doc comments on each variant.
2. Removed `use arcterm_vt::ContentType;` from `structured.rs`.
3. Updated `arcterm-render/src/renderer.rs` line 217 from `use arcterm_vt::ContentType` to `use crate::structured::ContentType`.
4. Removed `arcterm-vt = { path = "../arcterm-vt" }` from `arcterm-render/Cargo.toml`.
5. Added `ContentType` to the `pub use` re-exports in `arcterm-render/src/lib.rs`.
6. Updated `arcterm-app/src/main.rs`: moved `ContentType` into the existing `arcterm_render` import group, removing the standalone `use arcterm_vt::ContentType`.
7. Updated `arcterm-app/src/detect.rs`: changed `use arcterm_vt::ContentType` to `use arcterm_render::ContentType`.

**Deviation — type mismatch at main.rs call site:**
`StructuredContentAccumulator` (still sourced from `arcterm_vt`) carries `arcterm_vt::ContentType`, but `StructuredBlock::block_type` and `render_block()` now expect `arcterm_render::ContentType`. A temporary `vt_ct_to_render_ct` bridge function was added to `main.rs` to perform the variant-by-variant conversion. This bridge was removed in Task 3 once the local `osc7770::StructuredContentAccumulator` (carrying `arcterm_render::ContentType`) replaced the arcterm-vt type at the call site.

**Verification result:**
- `cargo check -p arcterm-render` — zero errors (arcterm-render no longer depends on arcterm-vt)
- `cargo check -p arcterm-app` — zero errors
- `cargo test -p arcterm-render` — 41 passed, 0 failed

**Commit:** `33cf57a shipyard(phase-12): relocate ContentType enum to arcterm-render`

---

### Task 3: Relocate Kitty types and OSC 7770 accumulator to arcterm-app

**Action taken:**
1. Created `arcterm-app/src/kitty_types.rs` containing: `KittyAction`, `KittyFormat`, `KittyCommand`, `KittyChunkAssembler`, and `parse_kitty_command`. All types copied from `arcterm-vt/src/kitty.rs` with `//!` module-level doc comment and `///` doc comments on all public items.
2. Created `arcterm-app/src/osc7770.rs` containing `StructuredContentAccumulator` copied from `arcterm-vt/src/handler.rs`. Imports `ContentType` from `arcterm_render` rather than `arcterm_vt`, so no conversion is needed at call sites.
3. Updated `arcterm-app/src/terminal.rs`:
   - Split the single `use arcterm_vt::{...}` line into `use arcterm_vt::{ApcScanner, GridState}` (types staying in arcterm-vt) and `use crate::kitty_types::{...}` and `use crate::osc7770::StructuredContentAccumulator`.
   - Added `use arcterm_render::ContentType` for the conversion helper.
   - Updated the test block to use `crate::kitty_types::{KittyAction, KittyCommand, KittyFormat}`.
   - Updated `take_completed_blocks()` to convert from `arcterm_vt::StructuredContentAccumulator` to the local type by mapping `vt_content_type_to_render` over each element.
   - Added `vt_content_type_to_render` helper function before the test module.
4. Registered `mod kitty_types;` and `mod osc7770;` in `arcterm-app/src/main.rs`.
5. Removed the temporary `vt_ct_to_render_ct` bridge from `main.rs` and simplified the call site to use `acc.content_type` directly (now `arcterm_render::ContentType`).

**Verification result:**
- `cargo check -p arcterm-app` — zero errors
- `cargo test -p arcterm-app` — 318 passed, 0 failed
- `arcterm-render/Cargo.toml` contains no reference to `arcterm-vt` (confirmed by grep)
- Full workspace check: `cargo check --workspace && cargo test -p arcterm-render -p arcterm-app` — all pass

**Commit:** `29a7fa0 shipyard(phase-12): relocate Kitty types and OSC7770 accumulator to arcterm-app`

---

## Final State

| Acceptance Criterion | Result |
|---|---|
| `cargo check -p arcterm-app` zero errors | PASS |
| `alacritty_terminal` in `Cargo.lock` | PASS |
| No version conflicts | PASS |
| `cargo check -p arcterm-render` zero errors | PASS |
| `arcterm-render` no longer depends on `arcterm-vt` | PASS |
| `cargo test -p arcterm-render` passes | PASS (41 tests) |
| `cargo check -p arcterm-app` with new modules | PASS |
| `cargo test -p arcterm-app` passes | PASS (318 tests) |
| Full verification: `cargo check --workspace && cargo test -p arcterm-render -p arcterm-app` | PASS |

## Key Files Modified or Created

- `/arcterm-app/Cargo.toml` — added `alacritty_terminal.workspace = true`
- `Cargo.toml` (workspace root) — added `alacritty_terminal = "0.25"`
- `arcterm-render/Cargo.toml` — removed `arcterm-vt` dependency
- `arcterm-render/src/structured.rs` — defined `ContentType` enum, removed `arcterm_vt` import
- `arcterm-render/src/renderer.rs` — updated import to `crate::structured::ContentType`
- `arcterm-render/src/lib.rs` — added `ContentType` to pub re-exports
- `arcterm-app/src/main.rs` — updated imports, registered new modules, removed bridge function
- `arcterm-app/src/detect.rs` — updated `ContentType` import source
- `arcterm-app/src/terminal.rs` — updated imports, added conversion helper, updated `take_completed_blocks`
- `arcterm-app/src/kitty_types.rs` — NEW: Kitty types and parse function
- `arcterm-app/src/osc7770.rs` — NEW: StructuredContentAccumulator with render ContentType
