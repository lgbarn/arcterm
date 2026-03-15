# SUMMARY-3.1.md — Plan 3.1: Color Schemes (Built-in and Custom)

**Phase:** 2
**Plan:** 3.1
**Branch:** master
**Date:** 2026-03-15
**Status:** COMPLETE — all 3 tasks committed, 61 unit tests passing, clippy -D warnings clean.

---

## Task 1: Color schemes module (TDD)

**Commit:** `9af4ec6` — `shipyard(phase-2): add 8 built-in color schemes`

**What was done:**

Created `/Users/lgbarn/Personal/myterm/arcterm-app/src/colors.rs` containing:

- `ColorPalette` struct with `foreground`, `background`, `cursor` (all `(u8,u8,u8)`) and `ansi: [(u8,u8,u8); 16]`.
- `by_name(name: &str) -> Option<Self>` for 8 schemes: `catppuccin-mocha`, `dracula`, `solarized-dark`, `solarized-light`, `nord`, `tokyo-night`, `gruvbox-dark`, `one-dark`.
- `with_overrides(self, &ColorOverrides) -> Self` — parses `"#rrggbb"` hex strings; invalid strings are silently ignored (palette slot unchanged).
- `Default` implementation returns catppuccin-mocha.
- `mod colors;` added to `main.rs`.

**Tests written (10, all passing):**
- `by_name_known_schemes_return_some` — all 8 names return `Some`.
- `by_name_unknown_returns_none` — empty string, unknown name, wrong case return `None`.
- `default_is_catppuccin_mocha` — `Default::default()` equals `by_name("catppuccin-mocha")`.
- `all_eight_palettes_are_distinct` — all 28 pairs differ.
- `overrides_apply_foreground_background_cursor` — hex overrides applied correctly.
- `overrides_apply_ansi_slots` — red (slot 1) and bright_white (slot 15) overridden.
- `invalid_hex_override_is_ignored` — bad hex leaves palette unchanged.
- `no_overrides_leaves_palette_unchanged` — empty overrides produce equal palette.
- `parse_hex_valid` — three valid hex strings parse correctly.
- `parse_hex_invalid` — missing `#`, wrong length, bad digits all return `None`.

**Deviations:**
- Fixed a pre-existing clippy `collapsible_if` lint in `main.rs` (window title setter) while editing the file.

---

## Task 2: Wire palette to renderer

**Commit:** `db530cc` — `shipyard(phase-2): wire color palette through renderer`

**What was done:**

Created `/Users/lgbarn/Personal/myterm/arcterm-render/src/palette.rs` with `RenderPalette`:
- Same three-plus-sixteen colour structure as `ColorPalette` but lives in the render crate.
- Helper methods: `bg_wgpu()`, `bg_f32()`, `fg_glyphon()`, `cursor_f32()`, `indexed_glyphon(n)`, `indexed_rgb(n)`.
- `indexed_rgb` uses the palette's `ansi[0..16]` for the first 16 indices, then the 216-colour cube and greyscale ramp for 16–255 — replacing the old static `ANSI16` table in `text.rs`.
- `Default` returns Catppuccin Mocha values (matching the app-level default).

Updated `arcterm-render/src/lib.rs` to export `RenderPalette`.

Updated `arcterm-render/src/renderer.rs`:
- Removed hardcoded `BG_COLOR`, `BG_COLOR_F32`, `CURSOR_COLOR` constants.
- Added `palette: RenderPalette` field to `Renderer`.
- Added `Renderer::set_palette(palette)` — replaces the field and clears row hashes to force a full re-shape on the next frame.
- `render_frame` passes `&self.palette` to `build_quad_instances` and `text.prepare_grid`.
- Clear `LoadOp` now uses `self.palette.bg_wgpu()`.
- Cursor quad colour uses `palette.cursor_f32()` (falls back to fg colour only when cell has explicit fg colour).

Updated `arcterm-render/src/text.rs`:
- Removed static `ANSI16` table, `indexed_to_rgb`, `indexed_to_glyphon` helpers.
- `ansi_color_to_glyphon` now takes `palette: &RenderPalette`; `Color::Default` fg resolves via `palette.fg_glyphon()`, bg via `palette.background`.
- `prepare_grid` takes `palette: &RenderPalette` and threads it into span building and `TextArea::default_color`.

**Deviations:** None.

---

## Task 3: Config → palette → hot-reload

**Commit:** `416377b` — `shipyard(phase-2): wire color scheme config and hot-reload`

**What was done:**

Updated `arcterm-app/src/main.rs`:

- Added `use arcterm_render::{RenderPalette, Renderer};`.
- Added `palette_from_config(cfg: &ArctermConfig) -> RenderPalette` helper at the bottom of the file. It calls `colors::ColorPalette::by_name(&cfg.color_scheme)`, falls back to `ColorPalette::default()` with a warning log if the name is unknown, applies `with_overrides(&cfg.colors)`, then field-maps the result into `RenderPalette`.
- In `resumed()`: `Renderer::new()` is followed by `renderer.set_palette(palette_from_config(&cfg))`.
- In `about_to_wait()` hot-reload block: added a change-detection guard comparing all `ColorOverrides` fields and `color_scheme` between `new_cfg` and `state.config`. When any differ, `palette_from_config(&new_cfg)` is called and `state.renderer.set_palette(new_palette)` is invoked, followed by `request_redraw()`.

Fixed pre-existing `dead_code` lint on `translate_named_key` in `input.rs` by adding `#[allow(dead_code)]`.

**Deviations:**
- The hot-reload change detection performs explicit field comparisons on `ColorOverrides` rather than deriving `PartialEq` on that struct. This is because `ColorOverrides` is in `config.rs` and adding a derive would be a larger structural change. The explicit comparison achieves the same correctness guarantee.

---

## Final State

| Metric | Value |
|---|---|
| New files | `arcterm-app/src/colors.rs`, `arcterm-render/src/palette.rs` |
| Modified files | `arcterm-render/src/lib.rs`, `arcterm-render/src/renderer.rs`, `arcterm-render/src/text.rs`, `arcterm-app/src/main.rs`, `arcterm-app/src/input.rs` |
| Unit tests | 61 passing (10 new color scheme tests) |
| Clippy | `cargo clippy --package arcterm-app --package arcterm-render -- -D warnings` passes clean |
| Commits | 3 atomic commits, one per task |

**Hot-reload behaviour:** Changing `color_scheme` or any `[colors]` entry in `~/.config/arcterm/config.toml` while the terminal is running will be detected by the config watcher, cause `set_palette()` to be called, invalidate all row hashes (forcing full re-shape), and trigger a redraw — all within the next `about_to_wait` cycle.
