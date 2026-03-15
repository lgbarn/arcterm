---
phase: terminal-fidelity
plan: "3.1"
wave: 3
dependencies: ["2.1", "2.2"]
must_haves:
  - 8 named built-in color schemes (Catppuccin Mocha, Dracula, Solarized Dark/Light, Nord, Tokyo Night, Gruvbox Dark, One Dark)
  - Color scheme selected via config.toml color_scheme field
  - Custom RGB overrides in [colors] section override scheme slots
  - Color palette flows from config through renderer to quad and text pipelines
  - Scheme changes apply on hot-reload without restart
files_touched:
  - arcterm-app/src/colors.rs (new file)
  - arcterm-app/src/config.rs
  - arcterm-app/src/main.rs
  - arcterm-render/src/text.rs
  - arcterm-render/src/renderer.rs
tdd: true
---

# PLAN-3.1 -- Color Schemes (Built-in and Custom)

## Goal

Implement 8 named built-in color schemes plus custom RGB override support. The
active color palette is determined by config.toml and flows through to the
renderer for both text foreground colors and background quad colors.

## Why Wave 3

Depends on PLAN-2.1 for the quad pipeline (backgrounds use palette colors) and
PLAN-2.2 for the config system (color_scheme field + hot-reload channel). This
plan wires palette colors into both the text and quad rendering paths.

## Tasks

<task id="1" files="arcterm-app/src/colors.rs" tdd="true">
  <action>
  Create the color schemes module with all 8 built-in palettes.

  1. Define `ColorPalette` struct:
     - `foreground: (u8, u8, u8)` -- default text fg
     - `background: (u8, u8, u8)` -- default window bg
     - `cursor: (u8, u8, u8)` -- cursor color
     - `ansi: [(u8, u8, u8); 16]` -- the 16 ANSI colors (0-15)
     Each field is an RGB triple.

  2. Implement `ColorPalette::by_name(name: &str) -> Option<ColorPalette>` that
     returns the named scheme. Supported names:
     - "catppuccin-mocha" (default)
     - "dracula"
     - "solarized-dark"
     - "solarized-light"
     - "nord"
     - "tokyo-night"
     - "gruvbox-dark"
     - "one-dark"

  3. Define each palette with accurate RGB values from the official scheme
     specifications. The 16 ANSI colors map to the scheme's terminal color
     definitions.

  4. Implement `ColorPalette::with_overrides(mut self, overrides: &ColorOverrides) -> Self`
     that applies user's [colors] overrides. Each override is an optional hex
     string ("#rrggbb"). Parse hex strings and replace the corresponding palette
     slot.

  5. Implement `ColorPalette::default() -> Self` returning catppuccin-mocha.

  6. Write tests:
     - by_name("catppuccin-mocha") returns Some with correct fg/bg
     - by_name("nonexistent") returns None
     - with_overrides replaces foreground when override is set
     - with_overrides leaves slots untouched when override is None
     - All 8 schemes have distinct foreground colors (no copy-paste errors)
     - Hex parsing handles uppercase and lowercase
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-app -- colors</verify>
  <done>All color scheme tests pass. All 8 built-in schemes return valid palettes. Overrides correctly replace individual palette slots. Hex parsing works for both "#FF0000" and "#ff0000".</done>
</task>

<task id="2" files="arcterm-render/src/text.rs, arcterm-render/src/renderer.rs" tdd="false">
  <action>
  Modify the renderer to accept and use a ColorPalette instead of hardcoded colors.

  1. Add a `palette` parameter to `Renderer::new()` (or a separate
     `set_palette(&mut self, palette)` method). Store palette in the Renderer struct.

  2. Replace the hardcoded `BG_COLOR` constant with `palette.background` converted
     to wgpu::Color.

  3. In `text.rs`, modify `ansi_color_to_glyphon()` to accept a `&ColorPalette`
     parameter (or make it a method on ColorPalette). When resolving
     `Color::Default`, use palette.foreground/palette.background instead of the
     hardcoded 0xd0d0d0 / 0x1e1e2e.

  4. When resolving `Color::Indexed(n)` for n < 16, look up `palette.ansi[n]`
     instead of the hardcoded ANSI16 table. For n >= 16, continue using the
     existing 256-color cube/greyscale math (those are standard and not
     scheme-dependent).

  5. In the quad pipeline integration (from PLAN-2.1), use palette colors for:
     - Cursor quad color: `palette.cursor`
     - Default background quads: `palette.background`

  6. Since text.rs defines `indexed_to_rgb()` with its own ANSI16 table, either:
     - Remove the ANSI16 table from text.rs and pass the palette through, OR
     - Keep indexed_to_rgb for the 256-color cube/greyscale but override indices
       0-15 with palette values.
     The cleaner approach is to pass the palette and use it for 0-15.

  7. Re-export or move ColorPalette so arcterm-render can use it. Options:
     - Move ColorPalette to arcterm-core (it's a shared data type)
     - Or have arcterm-app pass resolved RGB tuples to the renderer
     The simplest approach: define a `RenderPalette` in arcterm-render with the
     same structure, and have arcterm-app convert ArctermConfig's palette to it.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app</verify>
  <done>arcterm-app builds. The renderer uses palette colors for default fg/bg, ANSI 0-15, cursor color, and window clear color. No hardcoded color values remain in the rendering path for configurable colors.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs, arcterm-app/src/config.rs" tdd="false">
  <action>
  Wire color scheme selection through config and hot-reload.

  1. In `resumed()`, after loading config:
     - Resolve the palette: `ColorPalette::by_name(&config.color_scheme)`
       with fallback to default if name is invalid (log a warning).
     - Apply overrides: `palette.with_overrides(&config.colors)`.
     - Pass the resolved palette to `Renderer::new()`.

  2. In the config hot-reload handler (about_to_wait, from PLAN-2.2):
     - When a new config arrives, re-resolve the palette.
     - Call `renderer.set_palette(new_palette)` (or equivalent).
     - Request redraw.
     - Color scheme changes take effect immediately without restart.

  3. Add `mod colors;` to main.rs.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app</verify>
  <done>arcterm-app builds. Color scheme from config.toml is applied at startup. Changing color_scheme in config.toml triggers hot-reload and the new palette takes effect on the next frame without restarting.</done>
</task>
