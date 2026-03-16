# Plan 1.1: Add alacritty_terminal Dependency and Define Bridge Types

## Context

Before any migration work can begin, the workspace must have `alacritty_terminal` available and the types that will outlive the deleted crates must be relocated. Currently, `ContentType` and `StructuredContentAccumulator` live in `arcterm-vt`, but they are needed by `arcterm-render` (structured.rs) and `arcterm-app` (main.rs, detect.rs) after `arcterm-vt` is deleted. Similarly, `KittyCommand`, `KittyChunkAssembler`, and the Kitty types are needed for the image pipeline.

This plan adds the dependency and moves the surviving types into `arcterm-render` (for ContentType) and `arcterm-app` (for Kitty/OSC types), so that later waves can delete `arcterm-vt` without losing them.

## Dependencies

None — this is Wave 1.

## Tasks

### Task 1: Add alacritty_terminal to workspace dependencies
**Files:** `Cargo.toml` (workspace root), `arcterm-app/Cargo.toml`
**Action:** modify
**Description:**
1. Add `alacritty_terminal = "0.25"` to `[workspace.dependencies]` in the root `Cargo.toml`.
2. Add `alacritty_terminal.workspace = true` to `[dependencies]` in `arcterm-app/Cargo.toml`.
3. Run `cargo check -p arcterm-app` to verify the dependency resolves without conflicts.

No existing code is modified — the dependency is additive.

**Acceptance Criteria:**
- `cargo check -p arcterm-app` succeeds with zero errors
- `alacritty_terminal` appears in `Cargo.lock`
- No version conflicts with existing workspace dependencies (vte 0.15, base64 0.22, log 0.4)

### Task 2: Relocate ContentType enum to arcterm-render
**Files:** `arcterm-render/src/structured.rs`, `arcterm-render/src/renderer.rs`
**Action:** modify
**Description:**
1. In `arcterm-render/src/structured.rs`, define a new `ContentType` enum with the same variants as `arcterm_vt::ContentType`: `CodeBlock`, `Diff`, `Plan`, `Markdown`, `Json`, `Error`, `Progress`, `Image`. Add `#[derive(Debug, Clone, PartialEq, Eq)]`.
2. Remove `use arcterm_vt::ContentType;` from `structured.rs`.
3. In `arcterm-render/src/renderer.rs`, change `use arcterm_vt::ContentType;` (line 217) to `use crate::structured::ContentType;`.
4. In `arcterm-render/Cargo.toml`, remove the `arcterm-vt` dependency entirely.
5. Export `ContentType` from `arcterm-render`'s `lib.rs` so that `arcterm-app` can import it as `arcterm_render::ContentType`.
6. In `arcterm-app/src/main.rs` and `arcterm-app/src/detect.rs`, change `use arcterm_vt::ContentType` to `use arcterm_render::ContentType`.
7. Update `arcterm-app/src/main.rs` references to `StructuredContentAccumulator` — these will be replaced in Wave 2, but for now they still compile because `arcterm-vt` is still a dependency of `arcterm-app`.

**Acceptance Criteria:**
- `cargo check -p arcterm-render` succeeds (arcterm-render no longer depends on arcterm-vt)
- `cargo check -p arcterm-app` succeeds (arcterm-app still compiles using ContentType from arcterm-render)
- `cargo test -p arcterm-render` passes (structured.rs tests still work)

### Task 3: Relocate Kitty types and OSC 7770 accumulator to arcterm-app
**Files:** `arcterm-app/src/kitty_types.rs` (new), `arcterm-app/src/osc7770.rs` (new), `arcterm-app/src/terminal.rs`, `arcterm-app/src/main.rs`
**Action:** create + modify
**Description:**
1. Create `arcterm-app/src/kitty_types.rs` containing copies of: `KittyAction`, `KittyFormat`, `KittyCommand`, `KittyChunkAssembler`, and `parse_kitty_command` from `arcterm-vt`. These are needed for the Kitty image pipeline which survives the migration.
2. Create `arcterm-app/src/osc7770.rs` containing a copy of `StructuredContentAccumulator` from `arcterm-vt/src/handler.rs`. Import `ContentType` from `arcterm_render::ContentType`. This type is needed for the pre-filter's output channel.
3. In `arcterm-app/src/terminal.rs`, change `use arcterm_vt::{..., KittyCommand, ...}` to use the new local modules. The terminal.rs file will be fully rewritten in Wave 2, but keeping it compiling now avoids breaking the build.
4. Register both new modules in `arcterm-app/src/main.rs` (or lib.rs).

Note: The old `arcterm-vt` imports remain in `arcterm-app/Cargo.toml` until Wave 4 cleanup. This task adds parallel definitions that will be the sole survivors.

**Acceptance Criteria:**
- `cargo check -p arcterm-app` succeeds
- `cargo test -p arcterm-app` passes (existing tests use the types from the new modules or the old crate interchangeably)
- `arcterm-render/Cargo.toml` no longer contains `arcterm-vt`

## Verification

```bash
cargo check --workspace && cargo test -p arcterm-render -p arcterm-app
```

All workspace crates compile. Render and app tests pass. The `arcterm-render` crate has zero dependency on `arcterm-vt`. The `alacritty_terminal` crate is available for use in subsequent plans.
