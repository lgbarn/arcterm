---
phase: terminal-fidelity
plan: "2.2"
wave: 2
dependencies: ["1.1"]
must_haves:
  - TOML config file at ~/.config/arcterm/config.toml
  - Config controls font_family, font_size, shell, scrollback_lines, color_scheme
  - Hot-reload via notify file watcher
  - Default config generated on first run
  - Config values override hardcoded defaults in renderer and app
files_touched:
  - arcterm-app/Cargo.toml
  - arcterm-app/src/config.rs (new file)
  - arcterm-app/src/main.rs
  - arcterm-render/src/renderer.rs
tdd: true
---

# PLAN-2.2 -- TOML Configuration System with Hot-Reload

## Goal

Implement a TOML-based configuration system loaded from `~/.config/arcterm/config.toml`
that controls font, colors, shell, scrollback capacity, and keybindings. The config
hot-reloads when the file changes on disk.

## Why Wave 2

Depends on PLAN-1.1 for the `max_scrollback` field in Grid (config sets the
scrollback capacity). Does not touch any arcterm-core or arcterm-vt files.
Parallel with PLAN-2.1 (renderer) -- this plan touches `renderer.rs` only to
make FONT_SIZE and BG_COLOR configurable (passed as parameters), while PLAN-2.1
touches `renderer.rs` for the quad pipeline and render_frame changes. The
specific sections modified do not overlap.

**File overlap note with PLAN-2.1:** Both plans touch `arcterm-render/src/renderer.rs`.
PLAN-2.1 modifies `render_frame()` to add quad rendering. PLAN-2.2 modifies
`Renderer::new()` to accept font_size as a parameter and adds a `reconfigure()`
method. These are distinct code regions. If built sequentially within Wave 2,
PLAN-2.1 should be built first, then PLAN-2.2 adapts to whatever `render_frame`
looks like.

## Tasks

<task id="1" files="arcterm-app/Cargo.toml, arcterm-app/src/config.rs" tdd="true">
  <action>
  Create the config module with TOML parsing, defaults, and validation.

  1. Add dependencies to arcterm-app/Cargo.toml:
     - `toml = "1"`
     - `serde = { version = "1", features = ["derive"] }`
     - `dirs = "6"`
     - `notify = "8"`

  2. Create `arcterm-app/src/config.rs` with:

  3. Define `ArctermConfig` struct with serde Deserialize + Default:
     ```
     font_family: String          // default "monospace"
     font_size: f32               // default 14.0
     line_height_ratio: f32       // default 1.4
     shell: Option<String>        // default None (use $SHELL)
     scrollback_lines: usize      // default 10_000
     color_scheme: String         // default "catppuccin-mocha"
     cursor_style: String         // default "block" (block/underline/bar)
     cursor_blink: bool           // default false
     window_opacity: f32          // default 1.0
     padding: u32                 // default 4
     ```

  4. Define `[colors]` sub-struct `ColorOverrides` with Optional fields for all 16
     ANSI slots plus foreground, background, cursor color. Each is
     `Option<String>` (hex like "#ff0000").

  5. Define `[keybindings]` sub-struct with `copy: String` (default "super+c"),
     `paste: String` (default "super+v").

  6. Implement `ArctermConfig::load() -> ArctermConfig`:
     - Use `dirs::config_dir()` to find `~/.config/arcterm/config.toml`
     - If file exists, read and parse with `toml::from_str()`. On parse error,
       log a warning and return defaults.
     - If file does not exist, return defaults (do NOT create the file
       automatically -- let the user opt in).

  7. Implement `ArctermConfig::config_path() -> PathBuf` for the watcher.

  8. Write tests:
     - Default values are sensible (font_size=14.0, scrollback=10000)
     - Parsing a minimal TOML string overrides specific fields
     - Parsing an empty string returns defaults
     - Invalid TOML returns defaults (not panic)
     - Color hex parsing validates format
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-app -- config</verify>
  <done>All config tests pass. ArctermConfig::load() returns defaults when no file exists. Parsing valid TOML overrides fields correctly. Invalid TOML logs warning and returns defaults.</done>
</task>

<task id="2" files="arcterm-app/src/main.rs, arcterm-app/src/config.rs" tdd="false">
  <action>
  Integrate config loading into the application startup and wire values to subsystems.

  1. In `main()` or at the top of `resumed()`, call `ArctermConfig::load()` and store
     the result in `AppState` as a new `config: ArctermConfig` field.

  2. Pass `config.font_size` to `Renderer::new()` instead of the hardcoded FONT_SIZE.
     Modify `Renderer::new()` signature to accept `font_size: f32` as a parameter
     (remove the `FONT_SIZE` constant or keep it as a fallback).

  3. Pass `config.shell` to `Terminal::new()` -- this requires modifying `Terminal::new`
     and `PtySession::new` to accept an optional shell path override. If
     `config.shell` is Some, use it; otherwise fall back to $SHELL env var.

  4. Pass `config.scrollback_lines` to Grid when constructing via Terminal (add a
     `max_scrollback` parameter or set it after construction).

  5. Set window title to "Arcterm" (already done, but ensure config could override).

  6. Add `mod config;` to main.rs.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app</verify>
  <done>arcterm-app builds cleanly. Config values flow from TOML to Renderer (font_size), Terminal/Grid (scrollback_lines), and PtySession (shell path). Default behavior is unchanged when no config file exists.</done>
</task>

<task id="3" files="arcterm-app/src/config.rs, arcterm-app/src/main.rs" tdd="false">
  <action>
  Implement config hot-reload using notify file watcher.

  1. In `config.rs`, add a function `watch_config(tx: mpsc::Sender<ArctermConfig>)` that:
     - Creates a `notify::recommended_watcher` watching the config file path.
     - On file modify events, re-reads and parses the config file.
     - Sends the new ArctermConfig over the mpsc channel.
     - Runs on a background thread (spawned via std::thread).

  2. In `AppState`, add `config_rx: mpsc::Receiver<ArctermConfig>`.

  3. In `about_to_wait()`, check `config_rx.try_recv()`. If a new config arrives:
     - Log the reload.
     - Update `state.config` with the new values.
     - If font_size changed, the renderer needs to be recreated (or add a
       `reconfigure_font()` method -- for Phase 2, log that font changes require
       restart).
     - If scrollback_lines changed, update Grid's max_scrollback.
     - If color_scheme changed, update the color palette (implemented in PLAN-3.1).
     - Request a redraw.

  4. Start the watcher in `resumed()` after config is loaded.

  5. The watcher must handle the config file not existing (watch the directory instead
     and react when the file is created).
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app</verify>
  <done>arcterm-app builds. The config watcher compiles and starts in resumed(). Config changes trigger re-read and update of runtime-modifiable settings. Font size change logs a restart-required message. No panics when config file does not exist.</done>
</task>
