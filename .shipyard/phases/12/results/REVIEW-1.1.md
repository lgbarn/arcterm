# Review: Plan 1.1

## Verdict: PASS

---

## Stage 1: Spec Compliance

### Task 1: Add alacritty_terminal to workspace dependencies

- Status: PASS
- Evidence:
  - `Cargo.toml` (workspace root) line 21: `alacritty_terminal = "0.25"` present in `[workspace.dependencies]`.
  - `arcterm-app/Cargo.toml` line 19: `alacritty_terminal.workspace = true` present in `[dependencies]`.
  - SUMMARY-1.1 reports `alacritty_terminal v0.25.1` in `Cargo.lock` and zero conflicts with vte 0.15, base64 0.22, log 0.4.
- Notes: Implementation is purely additive. No existing code was touched.

### Task 2: Relocate ContentType enum to arcterm-render

- Status: PASS
- Evidence:
  - `arcterm-render/src/structured.rs` lines 12–31: `ContentType` enum defined with all 8 required variants (`CodeBlock`, `Diff`, `Plan`, `Markdown`, `Json`, `Error`, `Progress`, `Image`) and `#[derive(Debug, Clone, PartialEq, Eq)]`. Each variant has a `///` doc comment.
  - No `use arcterm_vt::ContentType` present anywhere in `arcterm-render/src/` (Grep confirmed zero matches).
  - `arcterm-render/src/renderer.rs` uses `crate::structured::ContentType` (per SUMMARY; no `arcterm_vt` references found in the render source tree).
  - `arcterm-render/Cargo.toml`: no `arcterm-vt` entry anywhere in the file (Grep confirmed).
  - `arcterm-render/src/lib.rs` line 20: `ContentType` exported via `pub use structured::{ContentType, HighlightEngine, RenderedLine, StructuredBlock, StyledSpan}`.
  - `arcterm-app/src/detect.rs` line 10: `use arcterm_render::ContentType`.
  - `arcterm-app/src/main.rs` line 208: `ContentType` imported from `arcterm_render` in the existing render import group.
  - SUMMARY-1.1 reports `cargo check -p arcterm-render` zero errors and `cargo test -p arcterm-render` 41 passed.
- Notes: The temporary `vt_ct_to_render_ct` bridge in `main.rs` was added and then removed in the same plan (Task 3), so it does not appear in the final diff. The deviation from the task description (needing the bridge at all) was handled correctly within scope.

### Task 3: Relocate Kitty types and OSC 7770 accumulator to arcterm-app

- Status: PASS
- Evidence:
  - `arcterm-app/src/kitty_types.rs`: new file, `//!` module-level doc comment present (line 1). Contains `KittyAction` (lines 14–27), `KittyFormat` (lines 29–40), `KittyCommand` (lines 46–69), `parse_kitty_command` (lines 75–136), and `KittyChunkAssembler` (lines 141–216). All public items have `///` doc comments. `Default` impl provided for `KittyChunkAssembler`.
  - `arcterm-app/src/osc7770.rs`: new file, `//!` module-level doc comment present (line 1). Contains `StructuredContentAccumulator` (lines 22–41) importing `ContentType` from `arcterm_render` (line 11). No `arcterm_vt::ContentType` reference.
  - `arcterm-app/src/terminal.rs` lines 5–8: imports split correctly — `arcterm_vt::{ApcScanner, GridState}` for the types remaining in that crate, `crate::kitty_types::{KittyChunkAssembler, KittyCommand, parse_kitty_command}`, and `crate::osc7770::StructuredContentAccumulator`. Test block uses `crate::kitty_types::{KittyAction, KittyCommand, KittyFormat}` (line 299).
  - `take_completed_blocks()` (lines 154–166) converts `arcterm_vt::StructuredContentAccumulator` elements to the local type via `vt_content_type_to_render` (defined lines 279–290). Helper is exhaustive over all 8 variants. The bridge is correctly located in the private implementation section and marked with a clear doc comment explaining it will be removed in Wave 2.
  - `arcterm-app/src/main.rs` lines 183 and 186: `mod kitty_types;` and `mod osc7770;` registered.
  - SUMMARY-1.1 reports `cargo test -p arcterm-app` 318 passed; full workspace verification passed.
- Notes: The plan note — "arcterm-vt imports remain in arcterm-app/Cargo.toml until Wave 4 cleanup" — is correctly observed. `arcterm-vt = { path = "../arcterm-vt" }` is still present in `arcterm-app/Cargo.toml`, which is intentional per spec.

---

## Stage 2: Code Quality

### Critical

None.

### Minor

- **`vt_content_type_to_render` will silently fail to compile if a new variant is added to `arcterm_vt::ContentType` without being added here.**
  - File: `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs` lines 279–290.
  - This is inherent to any two-enum bridge and the compiler enforces exhaustiveness, so adding a variant to one enum without updating the other will produce a compile error — not a silent bug. The bridge is correctly exhaustive over the current 8 variants. No action required now; this finding is a heads-up for the Wave 4 cleanup task that removes the bridge.

- **`osc7770.rs` defines only `StructuredContentAccumulator`; there is no `push_char` or mutation method.**
  - File: `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/osc7770.rs` lines 32–41.
  - Per the plan, this is a copy of the `arcterm-vt` type. In the original `arcterm-vt/src/handler.rs`, `StructuredContentAccumulator` may have had in-place mutation driven by the VT handler. If the buffer is assembled externally (e.g. by `GridState`) and the local type is only used as a data-transfer object, the stripped-down definition is appropriate. This is fine as a Wave 1 placeholder but should be revisited in Wave 2 when the accumulator is wired to the new state machine, to ensure the `buffer` field is populated before callers read it.

### Positive

- The `KittyChunkAssembler::receive_chunk` implementation includes a hard cap (`MAX_CHUNK_BUFFER_BYTES = 64 MiB`) against OOM via crafted escape sequences. This security concern was not in the plan but was correctly anticipated and defended — good defensive posture.
- Module-level `//!` docs and per-item `///` docs are present on all new public types in both `kitty_types.rs` and `osc7770.rs`, meeting the project's documentation convention.
- The bridge function `vt_content_type_to_render` is private, clearly named, and carries a doc comment explaining both its purpose and its planned removal. This makes the Wave 4 cleanup task easy to locate.
- `ContentType` placement in `arcterm-render` (not `arcterm-core` or `arcterm-app`) is architecturally correct: the type is consumed by the render layer and should be owned there.
- `Default` impl on `KittyChunkAssembler` delegates to `new()`, consistent with the project's existing derive/impl pattern.

---

## Findings

### Critical

None.

### Minor

- The `StructuredContentAccumulator` copy in `osc7770.rs` exposes only a constructor; its `buffer` field accumulation is driven externally by `GridState`. If Wave 2 moves accumulation to the new local type, ensure the field-population path is covered by tests before the `arcterm_vt` dependency is removed.
