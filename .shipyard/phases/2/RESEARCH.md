# Research: Phase 2 — Terminal Fidelity and Configuration

## Context

Phase 1 produced a working GPU-rendered terminal using: `vte 0.15`, `wgpu 28`, `winit 0.30`,
`glyphon 0.10`, `portable-pty 0.9`, and `tokio 1`. The codebase is structured as a
five-crate workspace: `arcterm-core` (grid + cell types), `arcterm-vt` (VT parser + handler
trait), `arcterm-pty` (PTY session), `arcterm-render` (wgpu + glyphon renderer), and
`arcterm-app` (winit application, input handling, terminal wiring).

Phase 2 must advance the terminal from "runs a shell" to "daily-driver quality." The six
distinct research questions answered here are:

1. **Current codebase state** — what exists and what gaps must be filled
2. **DEC private modes** — which modes are needed and how vte dispatches them
3. **TOML config crate** — parsing and hot-reload approach
4. **Clipboard integration** — crate selection
5. **Mouse events in winit 0.30** — exact API for drag selection
6. **Ring buffer for scrollback** — standard library vs dedicated crates
7. **Color scheme format** — how to embed named schemes in Rust

---

## Part 1: Existing Codebase Analysis

### arcterm-core/src/grid.rs — What Needs to Change for Scrollback

**Current state:** `Grid` holds a flat `Vec<Vec<Cell>>` (`cells` field, indexed
`[row][col]`). Scroll-up logic calls `cells.drain(0..n)` (O(n) shift on the whole Vec) and
appends blank rows at the bottom. There is no scrollback storage at all — rows that scroll
off the top are discarded forever. The `Grid` also has no concept of:
- An alternate screen buffer (needed for vim/tmux/htop)
- A viewport offset (for user scrollback review)
- A cursor saved/restored state (needed for DECSC/DECRC and mode 1049)

**Required changes:**

| Component | Change needed |
|-----------|---------------|
| Primary screen storage | Replace `Vec<Vec<Cell>>` with a ring buffer (scrollback) + viewport |
| Alternate screen buffer | Add a second `Grid` (or identical storage) for the alt screen |
| Viewport offset | `scroll_offset: usize` — 0 = current content, N = N lines above |
| Cursor save/restore | `saved_cursor: CursorPos` plus saved attributes |
| Scrollback capacity | `max_scrollback: usize` (from config, default 10,000) |
| Mode flags | `alt_screen_active: bool`, `cursor_visible: bool`, `auto_wrap: bool`, `app_cursor_keys: bool`, `bracketed_paste: bool` |

The alternate screen should be a complete second `Grid` with its own `cells`, `cursor`, and
`current_attrs`. Switching between screens is a swap of active context, not a copy.

**Key constraint:** The renderer currently accesses `grid.rows()` and `grid.cursor` directly
from `arcterm-render/src/text.rs`. Any scrollback-aware rendering path must pass a view slice
(rows visible in the current viewport) to the renderer rather than the full raw storage.

### arcterm-vt/src/processor.rs — VT Parser Gaps

**Current state:** The `Performer` impl in `processor.rs` handles a minimal CSI set:
`A/B/C/D` (cursor movement), `H/f` (cursor position), `J` (erase in display), `K`
(erase in line), `m` (SGR), `S/T` (scroll up/down). The `esc_dispatch` callback is a
no-op (`fn esc_dispatch(...) {}`). The `csi_dispatch` handler ignores the `intermediates`
parameter entirely, which means it cannot distinguish `ESC [ h` (standard set mode) from
`ESC [ ? h` (DEC private set mode).

**The `?` private mode mechanism:** Research confirms that `vte` passes the `?` byte
(0x3F) in the `intermediates` slice during DEC private mode sequences. When the terminal
receives `ESC [ ? 1 h` (DECCKM), `csi_dispatch` is called with:
- `intermediates = &[0x3F]` (the `?` character)
- `action = 'h'`
- `params` containing `[1]`

The current code matches only on `action` and ignores `intermediates`, so all DEC private
mode sequences (`?h` / `?l`) fall through to the `_ => {}` catch-all.

**Missing CSI sequences for vim/tmux/htop compatibility:**

| Sequence | Description | action | intermediates | params |
|----------|-------------|--------|---------------|--------|
| `CSI ? 1 h/l` | DECCKM: application/normal cursor keys | `h`/`l` | `[0x3F]` | `[1]` |
| `CSI ? 7 h/l` | DECAWM: auto-wrap mode | `h`/`l` | `[0x3F]` | `[7]` |
| `CSI ? 25 h/l` | DECTCEM: cursor visibility | `h`/`l` | `[0x3F]` | `[25]` |
| `CSI ? 47 h/l` | Alt screen (legacy) | `h`/`l` | `[0x3F]` | `[47]` |
| `CSI ? 1047 h/l` | Alt screen (use/restore) | `h`/`l` | `[0x3F]` | `[1047]` |
| `CSI ? 1049 h/l` | Alt screen + cursor save | `h`/`l` | `[0x3F]` | `[1049]` |
| `CSI ? 2004 h/l` | Bracketed paste mode | `h`/`l` | `[0x3F]` | `[2004]` |
| `CSI ? 1000 h/l` | Mouse button reporting | `h`/`l` | `[0x3F]` | `[1000]` |
| `CSI ? 1002 h/l` | Mouse button+drag reporting | `h`/`l` | `[0x3F]` | `[1002]` |
| `CSI ? 1003 h/l` | Mouse all-motion reporting | `h`/`l` | `[0x3F]` | `[1003]` |
| `CSI ? 1006 h/l` | SGR mouse encoding | `h`/`l` | `[0x3F]` | `[1006]` |

**Missing ESC sequences** (currently no-op in `esc_dispatch`):

| Sequence | Description | byte |
|----------|-------------|------|
| `ESC =` | DECKPAM: application keypad mode | `=` (0x3D) |
| `ESC >` | DECKPNM: normal keypad mode | `>` (0x3E) |
| `ESC 7` | DECSC: save cursor | `7` (0x37) |
| `ESC 8` | DECRC: restore cursor | `8` (0x38) |

**Missing CSI sequences** (non-private, action letter dispatch):

| Sequence | Description | action |
|----------|-------------|--------|
| `CSI P` | DCH: delete character(s) | `P` |
| `CSI @` | ICH: insert character(s) | `@` |
| `CSI L` | IL: insert line(s) | `L` |
| `CSI M` | DL: delete line(s) | `M` |
| `CSI X` | ECH: erase character(s) | `X` |
| `CSI G` | CHA: cursor horizontal absolute | `G` |
| `CSI d` | VPA: vertical position absolute | `d` |
| `CSI r` | DECSTBM: set scroll region (top/bottom margins) | `r` |
| `CSI n` | DSR: device status report (cursor position query) | `n` |
| `CSI c` | DA: device attributes (primary) | `c` |

`DECSTBM` (set scroll region) is critical: vim and htop use scrolling regions to isolate
status bars. Without it, `scroll_up` affects the entire screen rather than a bounded region.

**Handler trait additions needed:**

The `Handler` trait in `arcterm-vt/src/handler.rs` currently has no methods for mode
setting, alternate screen, scrolling region, or cursor save/restore. All of these need new
default-no-op methods plus corresponding `Grid` implementations.

### arcterm-render/src/ — Selection Highlighting and Dirty-Rect

**Current renderer flow** (`renderer.rs` → `text.rs`): Every frame rebuilds all row
`Buffer` objects from scratch, re-shapes all glyphs, and re-uploads all vertex data via
`glyphon::TextRenderer::prepare()`. This is O(rows × cols) per frame regardless of what
changed.

**Background colors are not rendered.** The current code calls `clear(BG_COLOR)` (a flat
dark background) and then renders text glyphs with per-cell foreground colors only. Cell
background colors from SGR (e.g., `ESC[41m` for red background) are stored in
`CellAttrs.bg` but are never used in `text.rs`. The `ansi_color_to_glyphon` function takes
an `is_fg: bool` flag but uses the bg color only to determine the default; it never fills
cell background rectangles.

**Gotcha: glyphon has no per-cell background support.** The `TextArea` struct has a
`default_color` field (text foreground fallback) but no background color field. There is no
API to fill a colored rectangle behind a glyph region. This is a fundamental architectural
gap for Phase 2.

**Required approach for background colors and selection highlighting:**

Add a separate wgpu render pass (or a sub-pass before the glyphon pass) that draws colored
quads for:
1. Cell background colors (where `cell.attrs.bg != Color::Default`)
2. Selection highlighting (the selected cell range)
3. Cursor block (currently faked via inverse-video in the text pass, but a solid quad would
   be more correct and easier to style)

This requires adding a simple wgpu pipeline with a vertex buffer of colored rectangles
(position + color per quad). The pipeline runs before glyphon's text pass in the same
`CommandEncoder`. This is the standard approach used by Alacritty and WezTerm.

**Dirty-rect rendering opportunity:** The `Cell.dirty` flag and `Grid.dirty` flag already
exist. The renderer can check these to skip re-shaping rows where no cell changed. Glyphon
does not support partial buffer invalidation, so the optimization is at the "skip
`buf.set_rich_text()` / `buf.shape_until_scroll()` for clean rows" level. This is a
meaningful optimization for the 120 FPS target.

### arcterm-app/src/main.rs — Mouse Events and Config Loading

**Current state:** The `window_event` handler matches only: `CloseRequested`,
`ModifiersChanged`, `Resized`, `RedrawRequested`, `KeyboardInput`. There is no match arm
for any mouse event variant. The `App` struct has no mouse state (last position, button
state, selection anchor).

**Mouse state needed in `App` or `AppState`:**

```
last_cursor_pos: PhysicalPosition<f64>
mouse_button_down: bool
selection_anchor: Option<CellPos>  // where drag started
selection_active: Option<(CellPos, CellPos)>  // start..end
```

**Config loading:** There is no config loading code anywhere in the codebase. The renderer
has a hardcoded `FONT_SIZE: f32 = 14.0` and `BG_COLOR` constant. All Phase 2 config values
are currently compile-time constants.

---

## Part 2: Crate Comparisons

### 2A. TOML Config Crate

**Decision context:** The PROJECT.md spec says `toml crate` already. This research
validates that choice and answers the hot-reload approach.

#### Comparison Matrix

| Criteria | `toml` v1.x | `toml_edit` v0.22 | `config` v0.14 |
|----------|-------------|-------------------|----------------|
| Purpose | Serde deserialization + serialization | Format-preserving editing | Multi-source config (env, file, defaults) |
| Version | 1.0.6 (March 6, 2026) | 0.22.x | 0.14.x |
| Downloads/month | 36.8M | 68.2M (used by cargo) | ~1.2M |
| License | MIT / Apache-2.0 | MIT / Apache-2.0 | MIT |
| Serde integration | Native (it is serde) | Separate via `toml_edit::de` | Wraps multiple parsers |
| Schema validation | Via serde derive | Via serde derive | Via serde derive |
| Hot-reload | None built-in | None built-in | None built-in |
| Dependency weight | Minimal | Moderate | Heavy (multi-backend) |
| Stack compatibility | Exactly what PROJECT.md specifies | Overkill for read-only config | Too heavy, adds env-var concerns |

**Recommendation: `toml` v1.x.** Arcterm needs read-only deserialization of a config
struct from a TOML file. The `toml_edit` crate's format-preserving editing is designed for
tools that write back to TOML (like cargo). The `config` crate is designed for applications
that aggregate from multiple sources (env vars, CLI args, files) which adds complexity and
dependencies arcterm does not need. The `toml` crate with `serde::Deserialize` on a config
struct is the right level of complexity.

**Hot-reload approach:** The `notify` crate (version 9.0.0-rc.2 as of Feb 2026; v8.2.0
was the last stable) watches the config file path for `ModifyKind::Data` events. When a
change event fires, the app re-reads and deserializes the config file. The
`notify-debouncer-mini` companion crate (v0.7.0) consolidates rapid sequential events from
editors that write files via temp-file rename (common in vim/neovim) into a single
notification. Debounce interval of 300–500ms is appropriate for config reload (fast enough
to feel instant, long enough to avoid partial-write reads).

**notify integration pattern:**

The `notify` watcher runs in a background thread. Its callback sends a message on a channel
that the winit event loop polls in `about_to_wait`. This is consistent with the existing
PTY reader pattern in the codebase.

**Note on notify version:** v9.0.0 is release-candidate as of research date. Ship with
v8.2.0 (last stable). v9 introduces a revised event model; the rc API may change before
final release.

### 2B. Clipboard Crate

#### Comparison Matrix

| Criteria | `arboard` v3.6 | `copypasta` v0.10 | `clipboard` v0.5 |
|----------|----------------|-------------------|------------------|
| Version | 3.6.1 (Aug 23, 2025) | 0.10.2 (Apr 25, 2025) | 0.5.0 (unmaintained) |
| Downloads/month | 2.0M | 157K | ~20K (declining) |
| Crates depending | 1,545 | ~150 | ~60 |
| License | Apache-2.0 / MIT | Apache-2.0 / MIT | MIT |
| Maintainer | 1Password (corporate) | Community | Effectively abandoned |
| Platforms | Linux (X11 + Wayland opt-in), macOS, Windows | Linux (X11 + Wayland), macOS, Windows | Linux (X11 only), macOS, Windows |
| Wayland support | Yes (feature flag `wayland-data-control`) | Yes | No |
| Image clipboard | Yes | No | No |
| API surface | `get_text()`, `set_text()`, `get_image()`, `set_image()` | `get_contents()`, `set_contents()` | `get_contents()`, `set_contents()` |
| Key limitation | Linux: clipboard data vanishes if `Clipboard` is dropped before paste | Limited documentation | X11 only, unmaintained |

**Recommendation: `arboard` v3.6.** The 1Password corporate maintenance backing,
dominant download share (13x copypasta), and Wayland support make it the clear choice.
The Linux data-lifetime limitation is a real gotcha: the `Clipboard` object must remain
alive long enough for a paste client to complete the X11 selection protocol handshake.
For arcterm's use case (terminal clipboard), the correct pattern is to keep a single
`Clipboard` instance for the application lifetime, stored in `AppState`.

`copypasta` was not chosen because it has a fraction of the ecosystem adoption and offers
no advantages over arboard for arcterm's use case. `clipboard` was not chosen because it
is unmaintained and lacks Wayland support.

**Cargo feature to enable on Linux for Wayland:**
```toml
arboard = { version = "3", features = ["wayland-data-control"] }
```

### 2C. Ring Buffer for Scrollback

#### Comparison Matrix

| Criteria | `std::collections::VecDeque` | `circular-buffer` crate | `ringbuffer` crate |
|----------|------------------------------|------------------------|--------------------|
| Source | Rust standard library | Third-party | Third-party |
| Allocation | Heap, growable | Heap, fixed capacity | Heap, fixed capacity |
| Overflow behavior | Manual: check len == cap, pop_front | Automatic: overwrites oldest | Automatic: overwrites oldest |
| Random access | O(1) via index | O(1) | O(1) |
| Iterator | `iter()`, `range()` | `iter()` | `iter()` |
| Additional dependency | None | Yes | Yes |
| Version / activity | Rust stdlib (always current) | ~0.4.x | ~0.3.x |
| Downloads/month | N/A (stdlib) | Low (~50K) | Low (~80K) |
| Slice access | `make_contiguous()` for &[T] | Yes | Yes |

**Recommendation: `VecDeque` from the standard library.** The scrollback buffer needs:
- Push new rows at the back (`push_back`)
- Drop old rows when at capacity (`pop_front`)
- Index access for rendering a viewport window
- No additional dependency

`VecDeque::with_capacity(max_scrollback)` preallocates the ring storage. Overflow is one
`if self.scrollback.len() == self.max_scrollback { self.scrollback.pop_front(); }` check.
This is two lines of code and zero additional dependencies.

The `circular-buffer` and `ringbuffer` crates offer automatic overflow, but this is a
three-line convenience that does not justify adding a dependency. Alacritty uses a custom
`Storage` type backed by a `Vec` with manual ring semantics; WezTerm uses `VecDeque`.
Both validate the "standard library is sufficient" conclusion.

**Memory sizing:** Each `Vec<Cell>` row is `cols × size_of::<Cell>()`. `Cell` is currently
`{char (4 bytes), CellAttrs (6 bytes), bool (1 byte)} = 11 bytes`, but with alignment
padding likely 12 bytes. For 10,000 rows × 220 cols (wide terminal): 10,000 × 220 × 12
bytes = ~26 MB. Within the "< 10MB per pane" target this is tight at max width; consider
reducing the default max or compressing the `Cell` struct (e.g., pack attrs into a u32).

---

## Part 3: Mouse Events in winit 0.30

The winit 0.30.9 `WindowEvent` enum includes these mouse-related variants (all fields
confirmed from `docs.rs/winit/0.30.9/winit/event/enum.WindowEvent.html`):

```
WindowEvent::CursorMoved {
    device_id: DeviceId,
    position: PhysicalPosition<f64>,  // pixels from top-left, f64
}

WindowEvent::MouseInput {
    device_id: DeviceId,
    state: ElementState,      // Pressed | Released
    button: MouseButton,      // Left | Right | Middle | Back | Forward | Other(u16)
}

WindowEvent::MouseWheel {
    device_id: DeviceId,
    delta: MouseScrollDelta,  // LineDelta(f32, f32) | PixelDelta(PhysicalPosition<f64>)
    phase: TouchPhase,
}

WindowEvent::CursorEntered { device_id: DeviceId }
WindowEvent::CursorLeft    { device_id: DeviceId }
```

**Key design constraints for selection:**

1. **Mouse position is not included in `MouseInput`.** The position must be tracked
   separately via `CursorMoved` and stored in `AppState`. When `MouseInput { state:
   Pressed, button: Left }` fires, the stored `last_cursor_pos` is the click position.
   This is the standard winit pattern — confirmed by the winit GitHub issue #883.

2. **Convert `PhysicalPosition<f64>` to cell coordinates** requires dividing by
   `(cell_width × scale_factor, cell_height × scale_factor)`. The `cell_size` field
   already exists on `TextRenderer` in `arcterm-render/src/text.rs`.

3. **Mouse reporting to the PTY** (for terminal apps that request mouse events via modes
   1000/1002/1003 + 1006 SGR encoding): When mouse reporting is active, `CursorMoved`
   and `MouseInput` events must be formatted as escape sequences and written to the PTY
   rather than (or in addition to) updating the selection state. The SGR mouse encoding
   (mode 1006) format is: `ESC [ < Cb ; Cx ; Cy M/m` where `Cb` = button code, `Cx`/`Cy`
   = 1-based cell column/row, `M` = press, `m` = release.

4. **Double-click and triple-click** require click timing. `winit` does not provide
   click-count events. The app must track the timestamp of the last click and compare it
   to the current click time. `std::time::Instant` is sufficient; a 300ms threshold for
   double-click is standard.

5. **Modifier keys for paste**: `Ctrl+Shift+C` / `Cmd+C` for copy and `Ctrl+Shift+V` /
   `Cmd+V` for paste. The `modifiers: ModifiersState` already tracked in `App` provides
   `modifiers.super_key()` for Cmd on macOS and `modifiers.shift_key()` for Shift.
   The existing `input.rs` only checks `modifiers.control_key()`.

**Mouse scroll for scrollback viewport:** `MouseWheel` with `LineDelta` delivers (x, y)
line counts. A `LineDelta(0.0, -3.0)` means scroll down 3 lines (wheel toward user).
The convention: negative y = scroll down (toward newer content), positive y = scroll up
(toward older content). Update `grid.scroll_offset` accordingly.

---

## Part 4: DEC Private Mode Dispatch — Implementation Pattern

The `csi_dispatch` call in `processor.rs` needs to branch on whether `intermediates`
contains `0x3F` (`?`). The cleanest pattern:

```rust
fn csi_dispatch(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
    if ignore { return; }
    let raw: Vec<&[u16]> = params.iter().collect();
    let is_private = intermediates.first() == Some(&b'?');

    if is_private {
        // DEC private modes: CSI ? <param> h/l
        let mode = raw.first().and_then(|p| p.first()).copied().unwrap_or(0);
        match (action, mode) {
            ('h', 1)    => self.handler.set_dec_mode(DecMode::Decckm, true),
            ('l', 1)    => self.handler.set_dec_mode(DecMode::Decckm, false),
            ('h', 7)    => self.handler.set_dec_mode(DecMode::Decawm, true),
            ('l', 7)    => self.handler.set_dec_mode(DecMode::Decawm, false),
            ('h', 25)   => self.handler.set_dec_mode(DecMode::Dectcem, true),
            ('l', 25)   => self.handler.set_dec_mode(DecMode::Dectcem, false),
            ('h', 47)   => self.handler.set_alt_screen(true),
            ('l', 47)   => self.handler.set_alt_screen(false),
            ('h', 1047) => self.handler.set_alt_screen(true),
            ('l', 1047) => self.handler.set_alt_screen(false),
            ('h', 1049) => self.handler.set_alt_screen_save_cursor(true),
            ('l', 1049) => self.handler.set_alt_screen_save_cursor(false),
            ('h', 2004) => self.handler.set_dec_mode(DecMode::BracketedPaste, true),
            ('l', 2004) => self.handler.set_dec_mode(DecMode::BracketedPaste, false),
            ('h', 1000) => self.handler.set_mouse_mode(MouseMode::Button),
            ('l', 1000) => self.handler.set_mouse_mode(MouseMode::None),
            ('h', 1002) => self.handler.set_mouse_mode(MouseMode::ButtonDrag),
            ('l', 1002) => self.handler.set_mouse_mode(MouseMode::None),
            ('h', 1003) => self.handler.set_mouse_mode(MouseMode::AnyMotion),
            ('l', 1003) => self.handler.set_mouse_mode(MouseMode::None),
            ('h', 1006) => self.handler.set_mouse_encoding(MouseEncoding::Sgr),
            ('l', 1006) => self.handler.set_mouse_encoding(MouseEncoding::X10),
            _ => {}  // unhandled private mode
        }
    } else {
        // existing standard CSI dispatch + new additions
        match action {
            // ... existing A/B/C/D/H/f/J/K/m/S/T ...
            'G' => { /* CHA */ }
            'd' => { /* VPA */ }
            'r' => { /* DECSTBM: set scroll region */ }
            'P' => { /* DCH: delete character */ }
            '@' => { /* ICH: insert character */ }
            'L' => { /* IL: insert line */ }
            'M' => { /* DL: delete line */ }
            'X' => { /* ECH: erase character */ }
            'n' => { /* DSR: device status report */ }
            'c' => { /* DA: device attributes */ }
            _ => {}
        }
    }
}
```

**Bracketed paste wire-up:** When `BracketedPaste` mode is active and the user pastes, the
input layer must wrap the pasted bytes with `ESC [ 200 ~` ... `ESC [ 201 ~` before writing
to the PTY. This is a responsibility of `arcterm-app/src/input.rs`, not the VT parser.

---

## Part 5: Color Scheme Format

### Approach Comparison

| Approach | Pros | Cons |
|----------|------|------|
| `const` arrays of `[(u8,u8,u8); 16]` in Rust | Zero runtime cost, compiler-verified, no parse step | Verbose to write, no IDE color preview |
| Embedded TOML (`include_str!` + parse at startup) | Readable, easy to add schemes, no recompile to add | Slight startup parse cost, string embedding |
| External files bundled with binary | Easiest to add new schemes, hotloadable | Complicates distribution (file not in binary) |

**Recommendation: `const` arrays in Rust source.** The eight built-in schemes have 16
ANSI colors each plus a background and foreground default. Defining them as `const` arrays
in a `schemes.rs` module makes them zero-cost, discoverable by `cargo clippy`, and ensures
the binary is self-contained (required for a single-binary distribution model). The
verbosity is acceptable: 8 schemes × 18 colors = 144 color values — approximately 200
lines of Rust.

**Color scheme data structure:**

```rust
pub struct ColorScheme {
    pub name: &'static str,
    pub background: (u8, u8, u8),
    pub foreground: (u8, u8, u8),
    pub ansi: [(u8, u8, u8); 16],  // indices 0-15 of the xterm-256 palette
}
```

The `ansi` array replaces entries 0–15 of the `indexed_to_rgb` table in
`arcterm-render/src/text.rs`. The remaining 240 entries (xterm 6×6×6 color cube +
grayscale ramp) are standard and scheme-independent.

**Config override:** The config struct allows per-slot RGB override in a `[colors]`
TOML table. These merge on top of the named scheme's `ansi` array at config-load time.

**Named scheme palette sources for the eight built-in schemes:**

Authoritative RGB values exist in the official repos for each scheme. The 16-color ANSI
palette for each can be sourced from:

- **Catppuccin Mocha:** https://github.com/catppuccin/catppuccin (spec repo)
- **Dracula:** https://draculatheme.com/spec — black=#282A36, red=#FF5555, green=#50FA7B, yellow=#F1FA8C, blue=#BD93F9, magenta=#FF79C6, cyan=#8BE9FD, white=#F8F8F2
- **Solarized Dark / Light:** https://ethanschoonover.com/solarized/
- **Nord:** https://www.nordtheme.com/docs/colors-and-palettes
- **Tokyo Night:** https://github.com/folke/tokyonight.nvim
- **Gruvbox Dark:** https://github.com/morhetz/gruvbox-contrib
- **One Dark:** https://github.com/atom/one-dark-syntax

These are all MIT/public domain palettes and may be embedded directly in source.

**Gotcha:** The xterm-256 "base 16" entries (indices 0–15) are not standardized — every
terminal emulator chooses different RGB values for them. The `indexed_to_rgb` function in
`arcterm-render/src/text.rs` currently hardcodes the classic GNOME Terminal palette.
Named color schemes replace exactly these 16 entries. The 6×6×6 cube (indices 16–231) and
grayscale ramp (232–255) are mathematically defined and not scheme-dependent.

---

## Part 6: Config System Architecture

### Config struct design (recommended)

```toml
# ~/.config/arcterm/config.toml

[general]
shell = "/bin/zsh"          # default: $SHELL
scrollback_lines = 10000

[font]
family = "monospace"        # or specific: "JetBrains Mono"
size = 14.0

[colors]
scheme = "catppuccin-mocha" # named built-in

# Optional per-slot overrides (applied on top of named scheme)
# [colors.overrides]
# black = "#1e1e2e"

[keybindings]
# Phase 2: only copy/paste modifiers
copy  = "Ctrl+Shift+C"
paste = "Ctrl+Shift+V"
```

### XDG config path

The `dirs` crate (v6.0.0, 12.9M downloads/month) provides `dirs::config_dir()` which
returns `~/.config` on Linux (respecting `$XDG_CONFIG_HOME`) and the platform equivalent
on macOS (`~/Library/Application Support`) and Windows (`AppData\Roaming`). The config
file path is: `dirs::config_dir() / "arcterm" / "config.toml"`.

This matches the CONTEXT-2.md decision of `~/.config/arcterm/config.toml` on Linux/macOS.

**Cargo dependency additions needed for Phase 2:**

```toml
# workspace Cargo.toml
toml      = "1"
serde     = { version = "1", features = ["derive"] }
notify    = "8"                 # file watcher (stable; not 9.0.0-rc)
arboard   = { version = "3", features = ["wayland-data-control"] }
dirs      = "6"
```

These go in `[workspace.dependencies]`. Individual crates (`arcterm-app`, `arcterm-core`)
opt in via `dep.workspace = true`.

---

## Files That Need Modification

| File | Change | Priority |
|------|--------|----------|
| `arcterm-core/src/grid.rs` | Add scrollback `VecDeque`, alt screen, viewport offset, cursor save, mode flags | Critical |
| `arcterm-core/src/cell.rs` | Evaluate `Cell` size; consider packing attrs to reduce memory | Medium |
| `arcterm-core/src/lib.rs` | Export new types (`ScrollbackBuffer`, `TerminalMode`, `MouseMode`) | Critical |
| `arcterm-vt/src/handler.rs` | Add Handler methods: `set_dec_mode`, `set_alt_screen`, `set_alt_screen_save_cursor`, `set_mouse_mode`, `set_mouse_encoding`, `set_scroll_region`, `save_cursor`, `restore_cursor`, `delete_char`, `insert_char`, `insert_line`, `delete_line`, `erase_chars`, `cursor_absolute_col`, `cursor_absolute_row`, `device_status_report`, `device_attributes` | Critical |
| `arcterm-vt/src/processor.rs` | Add `is_private` branch in `csi_dispatch`; implement `esc_dispatch` for `=`, `>`, `7`, `8` | Critical |
| `arcterm-render/src/text.rs` | Refactor `prepare_grid` to accept a viewport slice; add background quad rendering | Critical |
| `arcterm-render/src/renderer.rs` | Add background quad pipeline (colored rectangles before glyphon pass); expose selection API | High |
| `arcterm-render/src/lib.rs` | Export new render types | Medium |
| `arcterm-app/src/main.rs` | Add `CursorMoved`, `MouseInput`, `MouseWheel` match arms; add config loading and hot-reload watcher; add `Clipboard` to `AppState` | Critical |
| `arcterm-app/src/input.rs` | Add DECCKM-aware arrow key encoding; add bracketed paste wrapping; add Cmd+C/Cmd+V handling; add mouse reporting escape formatting | High |
| `arcterm-app/src/terminal.rs` | Expose scroll offset, selection state, mouse mode | High |
| New: `arcterm-app/src/config.rs` | Config struct with `serde::Deserialize`; load and hot-reload logic | Critical |
| New: `arcterm-core/src/schemes.rs` | `const ColorScheme` definitions for 8 built-in schemes | High |
| `Cargo.toml` | Add `toml`, `serde`, `notify`, `arboard`, `dirs` to `[workspace.dependencies]` | Critical |
| `arcterm-app/Cargo.toml` | Opt in to new workspace dependencies | Critical |
| `arcterm-render/Cargo.toml` | No new external deps needed for background quad pass | — |

---

## Comparison Matrix — New Crates for Phase 2

| Criteria | `toml` v1.0.6 | `notify` v8.2.0 | `arboard` v3.6.1 | `VecDeque` (stdlib) | `dirs` v6.0.0 |
|----------|---------------|-----------------|------------------|---------------------|---------------|
| Purpose | Config deserialization | File watching | Clipboard | Scrollback buffer | XDG paths |
| Maturity | Very high (30K crate dependents) | High (9.0 RC in progress) | High (1.5K crate dependents) | Rust std (stable) | High (12.9M DL/mo) |
| License | MIT / Apache-2.0 | CC0 / MIT / Apache-2.0 | Apache-2.0 / MIT | Rust lang (MIT) | MIT |
| Maintenance | Active (March 2026 release) | Active (v9 RC Feb 2026) | Active (1Password) | stdlib | Active (Jan 2025) |
| Stack compatibility | Aligns with PROJECT.md spec | Adds background thread | Cross-platform on all targets | No deps | Cross-platform |
| Key risk | None | v9 RC may change API | Linux clipboard lifetime | Memory at max width | None |

---

## Recommendation

**Use all five additions: `toml` + `notify` + `arboard` + `VecDeque` + `dirs`.** None of
these choices are controversial — they are the clear leaders in their respective niches.
The only non-trivial decision is the ring buffer, where stdlib `VecDeque` wins over
specialized crates purely because zero additional dependencies are needed for a feature
that requires three lines of code to implement.

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| glyphon background limitation requires separate render pipeline | High (certain — confirmed) | High | Plan and implement background quad pipeline in Phase 2 plan from the start; do not treat it as an afterthought |
| `Cell` memory usage exceeds 10MB/pane target at max width + scrollback | Medium | Medium | Measure first: profile `size_of::<Cell>()` and peak VecDeque size before optimizing; pack `CellAttrs` into a u64 if needed |
| Linux arboard clipboard data vanishes when `Clipboard` is dropped | High (certain on X11) | Low | Store `Clipboard` in `AppState` with app lifetime; do not create/drop per-operation |
| DECSTBM scroll region — incorrect interaction with scrollback | High | High | Scroll region operations must NOT push rows into the scrollback buffer; only full-screen scrolls push to scrollback |
| notify v9 RC API changes before Phase 2 ships | Medium | Low | Pin to `notify = "8"` (stable); upgrade to v9 in Phase 3+ when it goes stable |
| Mouse reporting encoding (modes 1000/1006) breaks apps expecting extended mouse | Medium | High | Implement SGR encoding (mode 1006) alongside basic mode 1000; most modern apps prefer SGR |
| DEC private mode state not persisted across alt screen switch | Medium | High | Alt screen switch must save/restore the full mode state (DECCKM, DECAWM, etc.) along with cursor and grid content |
| DECSTBM missing causes vim/htop to render incorrectly | High (current state) | High | Implement DECSTBM as first priority within VT work — it affects every full-screen app |
| Hot-reload race condition (file partially written during read) | Low | Low | Debounce by 300ms via `notify-debouncer-mini`; on parse error, keep the previous config |

---

## Implementation Considerations

### Integration Points

**Scrollback and rendering:** The renderer's `prepare_grid` in `text.rs` currently takes
`&Grid` and calls `grid.rows()`. After the scrollback refactor, it needs a viewport slice:
the rows currently visible on screen (accounting for `scroll_offset`). This is a
`&[Vec<Cell>]` slice of length `grid.size.rows`. The `Grid` should provide a method like
`visible_rows() -> &[Vec<Cell>]` that applies the current `scroll_offset`.

**Alt screen and renderer:** When the alt screen is active, the renderer reads cells from
the alt screen grid. `Terminal::grid()` must return a reference to whichever screen is
currently active. The renderer does not need to know which screen is active.

**Mouse reporting and input:** `arcterm-app/src/input.rs` currently has no awareness of
terminal mode state. It needs access to the current `mouse_mode` and `app_cursor_keys`
flags to decide whether arrow keys should output `ESC [ A` (normal) or `ESC O A`
(application mode). This coupling between input and terminal state is the trickiest
architectural change.

**Config hot-reload scope:** In Phase 2, hot-reload applies to: font size, color scheme,
scrollback limit. It does NOT apply to: shell path (session already running). Changing
font size requires re-measuring the cell size and resizing the PTY/grid.

### Migration Path

No user-visible config file exists yet. Phase 2 introduces the config file for the first
time. Reasonable defaults must work without any config file present
(missing file → use all defaults, no error).

### Testing Strategy

- VT parsing: table-driven unit tests in `arcterm-vt` — feed raw bytes, assert handler
  calls. Test each DEC private mode `?h`/`?l` pair, alternate screen switch sequences,
  DECSTBM, and bracketed paste.
- Scrollback: unit tests in `arcterm-core` — push N rows, verify ring behavior at
  capacity, verify viewport offset indexing.
- Config: unit test deserialization from TOML strings; test missing-file graceful default.
- Clipboard: integration-test only on local (not CI headless) — clipboard requires a
  display server. Gate clipboard tests with `#[cfg(target_has_feature = "display")]` or
  simply document as manual test.
- Mouse events: manual test checklist (cell conversion, drag selection, scroll).

### Performance Implications

- **Background quad pipeline:** Adding a separate geometry pass before glyphon increases
  GPU work per frame. For a 80×24 grid with all cells having non-default backgrounds,
  that is 1,920 quads. At 120 FPS this is trivial. Use instanced rendering (one draw
  call with 1,920 instances) rather than 1,920 individual draw calls.
- **Dirty-rect optimization:** Skipping `set_rich_text()` + `shape_until_scroll()` for
  clean rows is the highest-value optimization. In a typical shell session, fewer than
  5% of rows change per frame. This can yield a 10–20x reduction in CPU work per frame.
- **VecDeque.pop_front at capacity:** `pop_front` on `VecDeque` is O(1) amortized. At
  10,000 lines, this is not measurable. No concern.
- **Config hot-reload:** TOML deserialization of a small config file takes < 1ms. No
  performance concern even if triggered frequently.

---

## Sources

1. https://docs.rs/vte/0.15.0/vte/trait.Perform.html — Perform trait signatures, esc_dispatch confirmation
2. https://docs.rs/vte/latest/src/vte/lib.rs.html — vte source: `?` byte collected into intermediates via action_collect in CsiParam state
3. https://www.xfree86.org/current/ctlseqs.html — authoritative xterm control sequence reference; all DEC private mode numbers and ESC sequences confirmed here
4. https://lib.rs/crates/arboard — arboard v3.6.1, Aug 2025, 2.0M DL/mo, 1,545 dependents
5. https://lib.rs/crates/notify — notify v9.0.0-rc.2 (Feb 2026); v8.2.0 last stable, 6.5M DL/mo
6. https://lib.rs/crates/notify-debouncer-mini — v0.7.0 (Aug 2025), 782K DL/mo
7. https://lib.rs/crates/toml — toml v1.0.6 (Mar 2026), 36.8M DL/mo, 30,656 dependents
8. https://lib.rs/crates/dirs — dirs v6.0.0 (Jan 2025), 12.9M DL/mo, config_dir() XDG support
9. https://lib.rs/crates/copypasta — copypasta v0.10.2 (Apr 2025), 157K DL/mo
10. https://docs.rs/winit/0.30.9/winit/event/enum.WindowEvent.html — CursorMoved, MouseInput, MouseWheel field types
11. https://docs.rs/winit/0.30.9/winit/event/enum.MouseButton.html — MouseButton variants: Left, Right, Middle, Back, Forward, Other(u16)
12. https://github.com/rust-windowing/winit/issues/883 — confirms mouse position not included in MouseInput; must track via CursorMoved
13. https://doc.rust-lang.org/stable/std/collections/struct.VecDeque.html — VecDeque API reference
14. https://deepwiki.com/grovesNL/glyphon — confirms TextArea has no background color field; per-cell backgrounds require a separate geometry pass
15. https://github.com/grovesNL/glyphon — glyphon source and issue tracker
16. https://draculatheme.com/spec — Dracula color scheme authoritative RGB values
17. https://github.com/alacritty/alacritty/issues/2171 — confirms vte intermediates handling for private CSI sequences
18. https://wezterm.org/scrollback.html — WezTerm scrollback architecture reference (VecDeque approach)
19. https://epage.github.io/blog/2025/07/toml-09/ — toml v0.9 architecture changes (toml vs toml_edit separation)

---

## Uncertainty Flags

1. **vte intermediates for `?` — confirm with a real byte trace.** The source code analysis
   strongly implies that `?` (0x3F) is collected into `intermediates` during `CsiParam`
   state, but the research did not run a live test. Before shipping Phase 2 VT code, add a
   debug log in `csi_dispatch` that prints `intermediates` when handling `h`/`l` actions,
   and verify with `printf '\033[?25l'` (hide cursor) in the running terminal.

2. **notify v8 vs v9 API delta.** Research identified v9 is RC. The v8 stable API should
   be used, but the exact `notify::RecommendedWatcher` and event callback signature for v8
   vs v9 differ. Confirm the v8 API from `docs.rs/notify/8.2.0` before writing the
   hot-reload watcher, as v8 is not the version on `docs.rs/notify` (which serves latest).

3. **glyphon 0.10 vs 0.9 API differences.** The workspace currently pins `glyphon = "0.10"`,
   but the Phase 1 RESEARCH.md documented glyphon 0.9. The Phase 1 lesson notes in
   CONTEXT-2.md confirm "glyphon 0.10 (not 0.9) for wgpu 28 compatibility." The background
   quad pipeline will be implemented separately from glyphon, so this is not blocking, but
   any new glyphon API usage should be validated against v0.10 docs specifically.

4. **Cell memory layout.** The current `Cell` struct is `{char, CellAttrs, bool}`. `char`
   is 4 bytes, `CellAttrs` has `fg: Color` (5 bytes as enum), `bg: Color` (5 bytes),
   `bold/italic/underline/reverse: bool` (4 bytes), plus alignment padding. The actual
   `size_of::<Cell>()` should be measured at runtime before committing to a scrollback
   implementation, as it directly determines memory usage. If it exceeds 20 bytes, packing
   into a more compact representation should be addressed in Phase 2 rather than deferred.

5. **DECSTBM scroll region interaction with scrollback.** The semantics of what rows should
   be pushed to the scrollback buffer when a scroll region is active is subtle. The correct
   behavior: lines scrolled off the top of the scroll region do NOT enter the scrollback
   buffer if the scroll region does not cover the full screen. Only full-screen scroll-ups
   (no active scroll region, or scroll region = full screen) push rows to scrollback. This
   is the behavior of xterm and Alacritty but it was not verified against a primary spec
   document during this research pass.
