# REVIEW-3.1: Rewire Renderer to Read Alacritty's Grid

**Reviewer:** Claude (claude-sonnet-4-6)
**Date:** 2026-03-16
**Branch:** phase-12-engine-swap

---

## Stage 1: Spec Compliance

**Verdict:** PASS

### Task 1: Replace Grid/Cell types in renderer with alacritty equivalents

- Status: PASS
- Evidence:
  - `arcterm-render/src/snapshot.rs` exists and defines `SnapshotColor`, `SnapshotCell`, `RenderSnapshot`, and `snapshot_from_term` exactly as specified.
  - `arcterm-render/Cargo.toml` contains `alacritty_terminal.workspace = true` and no `arcterm-core` entry.
  - `arcterm-render/src/renderer.rs:22` — `PaneRenderInfo` uses `snapshot: &'a RenderSnapshot`.
  - `renderer.rs:382–434` — `build_quad_instances_at` iterates `0..snapshot.rows` / `0..snapshot.cols`, indexing `snapshot.cells[row_idx * snapshot.cols + col_idx]`.
  - `renderer.rs:437–446` — `term_color_to_f32` delegates to `ansi_color_to_glyphon` which matches on `SnapshotColor` variants.
  - `renderer.rs:363–373` — `grid_size_for_window` returns `(usize, usize)`.
  - `renderer.rs:126` — `render_frame` takes `&RenderSnapshot`.
  - `lib.rs:20` — `snapshot` module re-exported with all public types.
- Notes: The plan required `vte.workspace = true` as a named dependency. The implementation avoids needing it as a direct dependency by accessing `vte` through the `alacritty_terminal::vte` re-export path (`use alacritty_terminal::vte::ansi::Color as VteColor`). This is functionally correct and `cargo check` passes. The deviation from the literal spec action (not listing `vte` in `Cargo.toml`) is benign — the type is resolved transitively and the crate compiles cleanly.

### Task 2: Update TextRenderer for SnapshotCell

- Status: PASS
- Evidence:
  - `text.rs:3` — `use crate::snapshot::{RenderSnapshot, SnapshotCell, SnapshotColor}`; no `arcterm_core` import.
  - `text.rs:141–217` — `prepare_grid` takes `snapshot: &RenderSnapshot`, calls `snapshot.row(row_idx)`.
  - `text.rs:229–280` — `prepare_grid_at` takes `snapshot: &RenderSnapshot`, row extraction via `snapshot.row(row_idx)`.
  - `text.rs:666–698` — `shape_row_into_buffer` takes `&[SnapshotCell]`; reads `cell.fg`, `cell.bg`, `cell.inverse`.
  - `text.rs:643–657` — `substitute_cursor_char` takes `&[SnapshotCell]`.
  - `text.rs:752–776` — `hash_row` takes `(&[SnapshotCell], usize, Option<usize>)`.
  - `text.rs:729–742` — `ansi_color_to_glyphon` matches `SnapshotColor::Default`, `SnapshotColor::Rgb`, `SnapshotColor::Indexed`.
  - `text.rs:778–793` — `hash_snapshot_color` (renamed from `hash_color`) handles all three variants.
  - `text.rs:799–904` — 9 unit tests use `SnapshotCell::default()` and `SnapshotColor` variants; all pass (`41 passed, 0 failed`).
  - `arcterm-render/examples/window.rs:7` — imports `RenderSnapshot, SnapshotCell, SnapshotColor`; no `arcterm_core` reference.
- Notes: `shape_row_into_buffer` correctly handles bold and italic via `Attrs::weight(Weight::BOLD)` and `Style::Italic`; however, the plan's attribute mapping table included bold and italic but the current implementation does not apply them in the span attrs. See finding below.

### Task 3: Wire snapshot extraction into AppState render path

- Status: PASS
- Evidence:
  - `terminal.rs:596–598` — `lock_term()` implemented, returns `impl Deref<Target = Term<ArcTermEventListener>> + '_`.
  - `main.rs:1511–1514` — auto-detect path: brief lock, `snapshot_from_term`, unlock before GPU work.
  - `main.rs:2077–2079` — shell-exited banner path: `snapshot_from_term` replacing `to_arcterm_grid`.
  - `main.rs:2116–2119` — normal multi-pane render path: per-pane lock/snapshot/unlock before building `pane_frames`.
  - `main.rs:2402–2405` — Cmd+C clipboard copy path: `snapshot_from_term` replacing `to_arcterm_grid`.
  - `selection.rs:3,154,226` — `extract_text` takes `&RenderSnapshot`; `word_boundaries` takes `&[SnapshotCell]`.
  - `detect.rs:8,68` — `scan_rows` takes `&RenderSnapshot`.
  - No `arcterm_core` references found anywhere in `arcterm-app/src/`.
  - Verification: `cargo check --workspace` → 0 errors; `cargo test -p arcterm-render` → 41 passed; `cargo test -p arcterm-app` → 322 + 21 + 3 passed, 0 failed.
- Notes: The lock-hold pattern is correctly scoped — in all four call sites the `lock_term()` guard is a local binding inside a block expression `{ let term = ...; snapshot_from_term(...) }`, so it is released before GPU rendering begins.

---

## Stage 2: Code Quality

### Critical

No critical findings.

### Important

- **`shape_row_into_buffer` silently drops bold and italic attributes** — `arcterm-render/src/text.rs:666–698`
  - The plan (Task 2, item 2) maps `cell.bold` and `cell.italic` onto the span attrs. The implementation builds `span_strings: Vec<(String, Color)>` using only the foreground color; bold (`Weight::BOLD`) and italic (`Style::Italic`) are never applied to per-character `Attrs`. The attribute flags are stored correctly in `SnapshotCell` and hashed correctly in `hash_row`, but no glyphon `Weight` or `Style` is set per cell. All terminal bold and italic text will render at normal weight/upright, visually indistinguishable from plain text.
  - Remediation: Change the `span_strings` element type from `(String, Color)` to `(String, Attrs)`. Build `Attrs` per cell:
    ```rust
    let mut attrs = Attrs::new().family(Family::Monospace).color(fg);
    if cell.bold  { attrs = attrs.weight(Weight::BOLD); }
    if cell.italic { attrs = attrs.style(Style::Italic); }
    (s, attrs)
    ```
    Then in `buf.set_rich_text`, pass `(item.0.as_str(), item.1.clone())` instead of constructing a new `Attrs` per span.

- **`SHOW_CURSOR` TermMode not explicitly checked; comment overpromises** — `arcterm-render/src/snapshot.rs:95–97`, `183`
  - The doc comment on `cursor_visible` states it is `false` when `SHOW_CURSOR` mode is off. In practice, alacritty's `RenderableCursor::new` internally checks `TermMode::SHOW_CURSOR` and sets `cursor.shape = CursorShape::Hidden` when the mode is off; `snapshot_from_term` then maps this to `cursor_visible = false` correctly. This is functionally correct. However, the comment implies a direct mode check that the code does not perform, which will confuse future readers who search for `SHOW_CURSOR` in this codebase and find nothing. If alacritty ever changes the shape encoding, the indirection would silently break the comment's contract.
  - Remediation: Update the doc comment to accurately describe the mechanism: "This is `false` when `cursor.shape == CursorShape::Hidden`. Note: alacritty's `RenderableCursor::new` maps `!SHOW_CURSOR` mode → `CursorShape::Hidden` internally, so this field correctly reflects both the explicit hidden shape and the mode flag."

- **`prepare_grid_at` does not apply the dirty-row optimization** — `arcterm-render/src/text.rs:229–280`
  - `prepare_grid` (the single-pane path) uses `row_hashes` to skip re-shaping unchanged rows. `prepare_grid_at` (the multi-pane path, which is the actual hot path in production) re-shapes every row unconditionally on every frame. At 80 cols × 24 rows this is 1,920 cell iterations per pane per frame. For applications with static output (prompts, man pages), nearly all rows are reshaped wastefully. The `row_hashes` field is cleared on resize/palette change via `set_palette` and `resize`, making it safe to extend to the multi-pane path.
  - Remediation: This is a performance optimization gap, not a correctness bug. For the multi-pane path, extend `PaneSlot` or a parallel `Vec<u64>` to store per-pane row hashes, and apply the same `hash_row` skip logic as `prepare_grid`. Alternatively, carry the hash vec into `AppState` per pane and pass it through `prepare_grid_at`.

- **`reset_frame` truncates pool to 0 on every frame, discarding buffer allocations** — `arcterm-render/src/text.rs:129–135`
  - The comment at line 131 says "pane_buffer_pool already holds them; just clear the metadata." But line 134 calls `self.pane_buffer_pool.truncate(0)`, dropping all `Vec<Buffer>` entries. The intent (reuse allocations across frames) is not realized — every frame reallocates `num_rows` `Buffer` objects per pane. For a 2-pane layout at 24 rows each, this is 48 `Buffer` allocations per frame at 60fps = 2,880/second. The `Buffer` type allocates internal glyph storage on construction.
  - Remediation: Remove `self.pane_buffer_pool.truncate(0)` from `reset_frame`. Keep `self.pane_slots.clear()`. At the start of `prepare_grid_at`, use the existing `if slot_idx >= self.pane_buffer_pool.len()` push path to grow, but do not shrink on reset. The pool grows to the max pane count seen and stabilizes. Add a truncation only when `slot_idx < self.pane_buffer_pool.len()` is false after all panes are done — or accept the fixed max-pane-count watermark as acceptable.

### Suggestions

- **`row` helper on `RenderSnapshot` panics on out-of-bounds `row_idx`** — `arcterm-render/src/snapshot.rs:115–118`
  - `row()` computes `start = row_idx * self.cols` and slices `&self.cells[start..start + self.cols]` with no bounds check. If called with `row_idx >= self.rows`, the slice will either panic (debug) or produce a slice from unrelated memory (release, if cols happens to not overflow). The `cell()` helper at line 105 does have bounds checking. All current callers are loop-bounded by `0..snapshot.rows`, so this is not currently reachable. However the asymmetry between `cell()` (safe) and `row()` (panic) is a latent hazard.
  - Remediation: Add a bounds check to `row()`: `assert!(row_idx < self.rows, "row_idx {row_idx} out of bounds (rows={})", self.rows);` or return `Option<&[SnapshotCell]>` consistent with `cell()`.

- **Shell-exited banner mutates the snapshot directly via public field access** — `arcterm-app/src/main.rs:2084–2097`
  - The banner write path mutates `display.cells[row_start + col]` directly to overlay the exit message. This works because `cells` is `pub`, but it ties the banner rendering logic to the internal storage layout of `RenderSnapshot` and is semantically inconsistent — a "snapshot" should be immutable. A future refactor that changes cell storage (e.g., making it `pub(crate)`) would break this silently.
  - Remediation: Add a method `RenderSnapshot::write_row_text(row: usize, text: &str, fg: SnapshotColor, bg: SnapshotColor, bold: bool)` to `snapshot.rs` that encapsulates the row-overwrite logic. This moves the coupling to a named API rather than raw index arithmetic scattered in `main.rs`.

- **`hash_row` uses `DefaultHasher` which is not guaranteed stable across Rust versions** — `arcterm-render/src/text.rs:757–776`
  - `std::collections::hash_map::DefaultHasher` uses SipHash-1-3, which produces consistent results within a single process run. However, Rust does not guarantee hash stability across versions. Since `row_hashes` are only used for within-session dirty tracking (compared against values computed in the same binary), this is not a correctness concern. The comment and naming imply no persistence. No action required beyond noting the constraint.
  - Remediation: Add a doc comment to `hash_row` noting: "The hash value is only meaningful within a single process run; do not persist or compare across processes."

---

## Summary

**Verdict:** REQUEST CHANGES

The spec compliance check passes cleanly: all three tasks are implemented as specified, the `FairMutex` is held only during snapshot extraction, and 363 tests pass with zero failures. The architectural goal — decoupling the renderer from alacritty's lock semantics and internal cell types — is correctly achieved.

However, one important finding requires a fix before merge: bold and italic terminal attributes are not applied during text shaping (`shape_row_into_buffer` in `text.rs`), meaning all bold and italic terminal text renders at normal weight. This is a visible regression relative to the prior `arcterm_core::CellAttrs`-based path. Two additional Important findings (pool truncation on every frame, missing dirty-row optimization in the hot multi-pane path) are performance gaps that do not affect correctness but will compound in multi-pane sessions.

Critical: 0 | Important: 4 | Suggestions: 3
