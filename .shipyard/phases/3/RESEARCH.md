# Research: Phase 3 — Multiplexer (Panes, Tabs, Navigation)

## Context

Arcterm is a Rust terminal emulator using wgpu (Metal/Vulkan/DX12) for GPU rendering and
glyphon for text. Phase 2 delivered a single-pane terminal with a working PTY session,
VT processor, configurable color schemes, scrollback, text selection, and clipboard. The
current architecture is:

- `AppState` owns one `Terminal` (PTY + Processor + Grid) and one `Renderer`.
- `Renderer` owns `GpuState`, `QuadRenderer`, and `TextRenderer`. `render_frame()` takes a
  single `&Grid` and submits one render pass per frame.
- Input is handled synchronously in `window_event(WindowEvent::KeyboardInput)` and written
  directly to the PTY.
- `QuadRenderer::prepare()` accepts a `&[QuadInstance]` slice (solid colored rectangles).
  `TextRenderer::prepare_grid()` accepts a single `&Grid`.

Phase 3 must transform this single-pane design into a vim-navigable multiplexer with binary
tree splits, tabs, Neovim-aware pane crossing, a leader key, and a command palette. The
research below covers every non-trivial architectural decision.

---

## 1. Pane Tree Data Structure and Layout Engine

### What the codebase currently does

`AppState` holds a single `terminal: Terminal`. Resizing calls `terminal.resize(new_grid_size)`
where `new_grid_size` is derived from the full window pixel area. There is no concept of
sub-regions.

### Design options evaluated

**Option A: Flat list with explicit geometry (Zellij approach)**

Zellij stores all panes in a `BTreeMap<PaneId, Box<dyn Pane>>` and computes geometry
separately in a `TiledPaneGrid` object. Each pane has a `PaneGeom { x, y, rows, cols }`.
Directional navigation uses geometric overlap tests: to move left, filter panes where
`is_directly_left_of(current)` and `vertically_overlaps_with(current)`, then pick the
most recently focused. Layout is recomputed from scratch when splits or closes occur.

Zellij's `PaneGeom` uses a `Dimension` type that is either `Fixed(usize)` or
`Percent(f64)` to support both absolute and proportional sizing. A `split_out()` method
divides a dimension and a `combine_*_with()` recombines. Fractional remainders from
`Percent` rounding are redistributed explicitly.

Strengths: Directional navigation is simple geometry; no tree traversal needed. Easy to
serialize. Handles non-binary splits naturally.

Weaknesses: Split invariants are not enforced by the type system — a pane can be given an
arbitrary `PaneGeom` that overlaps another. Complex logic is required to maintain tiling
consistency.

**Option B: Recursive binary tree (tmux/kitty approach)**

Each node is either a `Leaf(PaneId)` or a `Split { axis: Axis, ratio: f32, left: Box<Node>, right: Box<Node> }`. Layout is computed by a single recursive function that accepts a
pixel rect and subdivides it according to `ratio`. Directional navigation traverses the
tree upward to find the nearest ancestor that splits in the target direction, then
descends into its other child.

Strengths: Split invariants are guaranteed by construction — there are no overlapping or
missing panes. Resize is a ratio change on one node. Zoom (fullscreen toggle) is easily
implemented by temporarily bypassing the tree and rendering the focused pane at full size.

Weaknesses: Arbitrary grid layouts (e.g., a 3x3 grid) require nesting multiple splits.
Non-binary splits require a different node type. Directional navigation at deep nesting
levels requires careful upward traversal.

**Option C: Entity list with a separate layout spec (Wezterm approach)**

Panes are stored by ID, and a separate layout tree describes how they are arranged. The
layout tree can express row/column splits with multiple children. Navigation is handled
separately.

Strengths: More expressive than binary tree for complex layouts.

Weaknesses: More implementation surface for Phase 3 scope; overkill for a binary-split-only
design.

### Recommendation for Arcterm Phase 3

**Binary tree (Option B)** is the correct choice given that CONTEXT-3.md explicitly
specifies binary splits, and Phase 3 is scoped to "binary splits with configurable ratios".
It matches the design intent and keeps the layout invariants enforced by the type system.
The directional navigation algorithm is well-understood: traverse to the nearest ancestor
that splits in the requested direction, then focus the deepest leaf on the far side of that
split. The zoom feature becomes trivial (render the focused leaf at the full window rect,
bypassing tree layout).

A concrete node type for Arcterm:

```
enum PaneNode {
    Leaf { pane_id: PaneId },
    HSplit { ratio: f32, left: Box<PaneNode>, right: Box<PaneNode> },
    VSplit { ratio: f32, top: Box<PaneNode>, bottom: Box<PaneNode> },
}
```

Each `PaneId` maps to a `Terminal` in a `HashMap<PaneId, Terminal>`. Layout is computed
recursively from the root node and a `PixelRect { x, y, width, height }`. The resulting
`HashMap<PaneId, PixelRect>` is the input to both the renderer and the PTY resize logic.

Grid size for each pane: `GridSize { cols: (rect.width / cell_w).floor(), rows: (rect.height / cell_h).floor() }`.

### Integration notes

- `AppState.terminal: Terminal` becomes `AppState.panes: HashMap<PaneId, Terminal>` plus
  `AppState.layout: PaneNode` plus `AppState.focus: PaneId`.
- `Terminal::new()` signature is unchanged. One Terminal per leaf node.
- `about_to_wait` must poll `pty_rx` for every pane, not just one. The `pty_rx` channels
  are stored alongside each Terminal, e.g., in a `HashMap<PaneId, mpsc::Receiver<Vec<u8>>>`.
- On window resize, recompute all rects from the tree root and call `terminal.resize(size)`
  for each pane. Panes may get very small (min 2 cols, 1 row enforced by clamping).

---

## 2. Rendering Multiple Grids

### Current rendering pipeline

`Renderer::render_frame(&Grid, scale_factor)`:
1. Calls `build_quad_instances(grid, cell_size, scale, palette)` — produces one
   `QuadInstance` per non-default background cell and per cursor block. Coordinates start
   at pixel (0, 0).
2. Calls `TextRenderer::prepare_grid(device, queue, grid, scale, palette)` — produces one
   `TextArea` per grid row with `left=0.0, top=row_idx * cell_h`.
3. One `begin_render_pass` → quad draw → text draw → `frame.present()`.

### What needs to change

For N panes, each pane has a pixel rect `(px, py, pw, ph)`. The quad instances for pane K
must be offset by `(px, py)`. The TextAreas for pane K must have `left = px, top = py + row_idx * cell_h`.

**Glyphon TextArea supports arbitrary `left` and `top`**: The `TextArea` struct has `left: f32` and `top: f32` fields (confirmed in `arcterm-render/src/text.rs` and the glyphon source). `TextRenderer::prepare_grid` builds a `Vec<TextArea>` and passes it as a slice to `glyphon::TextRenderer::prepare()`. To render N panes, `prepare_grid` needs to accept a pane offset parameter, or the caller builds all TextAreas and passes them as one slice.

The cleanest approach: introduce `prepare_grid_at(device, queue, grid, scale, palette, offset_x_px, offset_y_px)` that accepts a pixel offset, then collect all TextAreas from all panes into a single `Vec<TextArea>` and submit them in one `prepare()` call. This keeps a single render pass per frame, which is optimal for wgpu.

**QuadRenderer already accepts arbitrary rects**: `QuadInstance { rect: [x, y, w, h], color }` is already offset-relative. To render pane K's quads, offset each quad's `x` by `pane_rect.x` and `y` by `pane_rect.y`. Collect all quads from all panes into one `Vec<QuadInstance>`, then call `quads.prepare()` once.

**Pane border quads**: A 1px horizontal border between two vertically-stacked panes is a `QuadInstance { rect: [split_x, split_y, split_w, 1.0], color: border_color }`. Same pattern for vertical borders. These are added to the master quad list before `prepare()`.

**Tab bar quads + text**: The tab bar is a horizontal strip of height `cell_h` at the top. Each tab label is a small `TextArea`. The pane tree rects are computed with a `y_offset = tab_bar_height` applied to the root rect, so panes never overlap the tab bar. This is the simplest correct approach.

**Performance**: The current `QuadRenderer` is bounded at `MAX_QUADS = 8192`. A 4-pane layout with typical content would produce far fewer quads than this. The `TextRenderer` allocates one `glyphon::Buffer` per row; for N panes total the buffer pool grows proportionally but each buffer is lightweight.

---

## 3. Tab Model

### Design

A `Tab` contains:
- `layout: PaneNode` — the binary split tree for that tab
- `panes: HashMap<PaneId, Terminal>` and `pty_channels: HashMap<PaneId, Receiver<Vec<u8>>>`
- `focus: PaneId` — the focused pane within this tab

`AppState` holds:
- `tabs: Vec<Tab>`
- `active_tab: usize`

Switching tabs: update `active_tab`. The `about_to_wait` loop only polls channels for
`tabs[active_tab]`. Background tabs accumulate data in their `mpsc::Receiver` buffers
(unbuffered by default, but the tokio mpsc channel's buffer is 64 messages per the current
`PtySession::new` code). Background PTY output accumulates until the tab is reactivated;
this is consistent with tmux's behavior.

The tab bar renders as a row of `QuadInstance` backgrounds (active tab: accent color,
inactive: dim gray) with `TextArea` labels overlaid.

---

## 4. Leader Key Implementation

### The problem

`input.rs::translate_key_event` currently converts every key event to a byte sequence and
returns immediately. There is no state. A leader key requires a two-key chord: Ctrl+a
followed by a second key, with a timeout fallback.

### tmux's approach (reference implementation)

tmux uses a table-switching model. The client maintains a `keytable` field pointing to the
currently active binding table (normally "root", switches to "prefix" when the prefix key
is pressed). On each keypress, tmux looks up the key in the current table, executes the
bound command, and resets to "root". The timeout is checked on each keypress: if the
elapsed time since entering the prefix table exceeds `prefix-timeout`, the table resets to
root and the current key is processed normally. The actual timeout source (`evtimer` or
`libevent`) fires independently and resets the table even with no subsequent keypress.

Source: `tmux/server-client.c` — `server_client_key_table_activity_diff()` and the
`if (prefix_delay > 0 && strcmp(table->name, "prefix") == 0 && ...)` check.

### Recommended approach for Arcterm

A `KeymapState` enum with two variants:

```
enum KeymapState {
    Normal,
    LeaderPending { entered_at: Instant },
}
```

Stored on `AppState`. On each `WindowEvent::KeyboardInput`:

1. Check if `KeymapState::LeaderPending` and `entered_at.elapsed() > 500ms` →
   treat as timeout: send the raw Ctrl+a byte (`0x01`) to the focused pane's PTY and
   reset to `Normal`, then re-process the current key as if in Normal state.
2. If `Normal` and the key is Ctrl+a → transition to `LeaderPending { entered_at: Instant::now() }`, consume the event (do not forward to PTY).
3. If `LeaderPending` and a second key arrives → dispatch the leader action (e.g., 'n'
   → split, 'q' → close pane), reset to `Normal`.
4. If `Normal` and the key is Ctrl+h/j/k/l → dispatch pane navigation directly (these are
   always-active, no leader required per CONTEXT-3.md).

The timeout check in step 1 runs at the *next keypress*, not via a timer. This is
acceptable and simpler — the Ctrl+a is sent retroactively if the user resumes typing. If
a timer-driven timeout is desired (so Ctrl+a is sent without a subsequent keypress), a
`tokio::time::sleep` future can be polled in `about_to_wait` using a `tokio::sync::oneshot`
channel; however, this adds complexity. For Phase 3, next-keypress timeout is sufficient.

**Ctrl+Space for command palette**: Detected as `Key::Named(NamedKey::Space)` with
`modifiers.control_key()` true. The current `translate_key_event` function would translate
this to `0x00` (Ctrl+Space = NUL). The key handler in `window_event` must intercept
Ctrl+Space *before* calling `translate_key_event`.

**Ctrl+h/j/k/l detection**: Currently `translate_key_event` returns `0x08` for Ctrl+h,
`0x0a` for Ctrl+j, `0x0b` for Ctrl+k, `0x0c` for Ctrl+l. These are the raw control codes
that would normally be forwarded to the PTY. The window_event handler must intercept these
specific control codes and trigger pane navigation instead of forwarding.

Conflict risk: Ctrl+h is also `Backspace` in many apps. The CONTEXT-3.md decision is that
Ctrl+h/j/k/l are always intercepted. This is the tmux/wezterm model. Applications that
need Ctrl+h (e.g., shells using readline) will be affected. This is a documented tradeoff.

---

## 5. Neovim Socket Communication

### Detection: is the focused pane running Neovim?

The `PtySession` struct holds a `child: Box<dyn portable_pty::Child + Send + Sync>`. The
`portable_pty` crate does not currently expose a method to get the child process PID.
However, `portable_pty::Child` on Unix systems wraps a standard process; the PID can be
retrieved by downcasting or by inspecting `/proc` (Linux) or using `sysctl` (macOS).

A practical approach on macOS/Linux:

1. At spawn time, record the child PID using a wrapper around `portable_pty` that calls
   `libc::getpid()` after fork, or use `std::process::Command` child PID if available.
2. Check environment variables of the child process. Neovim sets `$NVIM` (the socket
   address) on all child processes it spawns. A shell running *inside* Neovim will have
   `$NVIM` set. But arcterm wants to detect Neovim *as the direct child process*, not a
   shell inside Neovim.
3. Check the process name of the PTY child. On macOS: `sysctl CTL_KERN / KERN_PROC /
   KERN_PROC_PID` → `kinfo_proc` → `kp_proc.p_comm`. On Linux: read
   `/proc/<pid>/comm`. If the name is `nvim`, the pane is running Neovim.

**The `$NVIM` variable approach (for the socket address)**: When arcterm spawns a shell,
the shell runs inside arcterm. If the user then launches Neovim with `--listen <socket>`,
the socket path must be discovered by arcterm. Options:

- Parse the shell environment of the Neovim child process after it spawns. On Linux,
  `/proc/<nvim_pid>/environ` contains the environment. On macOS, `sysctl` with
  `KERN_PROCARGS2` can extract the environment from the process. The `$NVIM_LISTEN_ADDRESS`
  or `v:servername` would be set.
- Alternatively: set a known socket path as an environment variable when spawning the shell
  (e.g., `ARCTERM_PANE_ID=<id>`). Instruct the user's shell config to launch Neovim with
  `nvim --listen /tmp/arcterm-nvim-<pane_id>.sock "$@"`. This is a wrapper approach and
  requires user configuration.
- Practical simplest approach for Phase 3: Detect process name as `nvim`. Then attempt to
  read `v:servername` by connecting to the socket discovered by scanning
  `/proc/<pid>/fd` (Linux) or via `lsof -p <pid>` (macOS) for Unix socket file
  descriptors. If socket found, attempt connection.

The cleanest approach that does not require user config changes is to inspect
`/proc/<pid>/environ` on Linux and `KERN_PROCARGS2` on macOS to find the `--listen`
socket path that Neovim uses (it is stored in `v:servername` and passed as an env var to
children as `$NVIM`). Specifically: a Neovim instance that was started with `--listen
/path/to/socket` will set `NVIM=/path/to/socket` on every process it spawns as a job.
A shell running *inside* Neovim inherits `$NVIM`. Arcterm's own shell (PTY child) will
NOT have `$NVIM` unless it was launched inside Neovim. This means arcterm must inspect the
*Neovim* process's own environment, not its child shell's environment.

Revised detection sequence:
1. Get child PID of the PTY session.
2. Read process name (`/proc/<pid>/comm` or macOS sysctl). If name starts with `nvim`,
   proceed.
3. Read the Neovim process's command-line args (`/proc/<pid>/cmdline`) looking for
   `--listen <path>`. If found, use that path as the socket.
4. If `--listen` is absent: inspect whether Neovim started with no `--listen` flag and
   thus chose an automatic socket path. Neovim's automatic socket path on Unix is stored
   in `v:servername` and is not in a fixed location. In this case, scan open file
   descriptors of the nvim process for Unix socket files — on Linux:
   `ls -la /proc/<pid>/fd/` filtered to socket types; on macOS: `lsof -U -p <pid>`.
5. Attempt to connect via the discovered path.

This is moderately complex. A simpler fallback: tell users to add `alias nvim='nvim
--listen /tmp/nvim-arcterm.sock'` to their shell config. Arcterm checks for a
well-known socket pattern.

### Querying Neovim's split layout

Once connected to the socket, the sequence to determine if a split exists in direction D
from the current window:

1. `nvim_get_current_win()` → `current_win_id`
2. `nvim_win_get_position(current_win_id)` → `[row, col]` (grid cell coordinates, not pixels)
3. `nvim_list_wins()` → `[win_id_1, win_id_2, ...]`
4. For each other `win_id`:
   - `nvim_win_get_position(win_id)` → `[r, c]`
   - Compare `r` and `c` to current position to determine relative direction

Direction logic (positions are in grid rows/cols, not pixels):
- Left neighbor: `c < current_col` and `|r - current_row| < threshold`
- Right neighbor: `c > current_col` and row overlap
- Above neighbor: `r < current_row` and column overlap
- Below neighbor: `r > current_row` and column overlap

This mirrors Zellij's geometric overlap approach. A `threshold` of half the current
window's height/width serves as the overlap test.

If a neighbor exists in direction D, pass the navigation key through to Neovim (forward
the appropriate VT sequence to the PTY). Neovim's own key handling will move focus to its
neighboring split. If no neighbor, arcterm moves focus to the adjacent arcterm pane.

### Neovim RPC crate options

| Crate | Version | Stars | Last Activity | License | Async Runtime | Notes |
|-------|---------|-------|---------------|---------|---------------|-------|
| nvim-rs | ~0.6 | 266 | Early 2025 | LGPL-3 (dual MIT/Apache for new commits) | tokio or async-std (feature flag) | Unstable API, actively developed |
| neovim-lib | 0.5.1 | 192 | January 2018 | LGPL-3 | Synchronous (blocking) | Unmaintained |
| rmp-rpc | 0.3.0 | 46 | Unknown (inactive) | Unknown | tokio | Minimal adoption, appears inactive |
| rmpv (hand-rolled) | 1.3.1 | N/A | Active | MIT | None (data types only) | Use as base for custom implementation |

**Recommendation: nvim-rs** for socket communication, with fallback to a hand-rolled
msgpack-rpc client using `rmpv` and tokio's `UnixStream` if nvim-rs proves too unstable.

`nvim-rs` provides `create::tokio::new_path(socket_path, handler)` which connects to an
existing Neovim socket via `tokio::net::UnixStream`. Arcterm already depends on tokio with
`features = ["full"]`. The LGPL-3 license of nvim-rs requires dynamic linking or providing
modified source — for an open-source (MIT) project, this is manageable but note that
distributing arcterm binaries linked statically against nvim-rs would require publishing
nvim-rs source changes. Since Arcterm targets MIT or Apache 2.0 (per PROJECT.md), the
team should either: (a) use nvim-rs as a dynamically linked dependency (the LGPL allows
this), or (b) implement the RPC layer directly using `rmpv + tokio::net::UnixStream`,
which is a small (~200 lines) async msgpack-rpc client for the specific 3-4 API calls
needed. Given the narrow API surface (only `nvim_list_wins`, `nvim_win_get_position`,
`nvim_get_current_win`), a hand-rolled implementation is a viable alternative to avoid the
LGPL dependency.

**Msgpack wire format** (for hand-rolled implementation): Neovim uses standard msgpack-rpc.
A request is `[0, msgid, method, params]` and a response is `[1, msgid, error, result]`,
serialized as msgpack arrays. The `rmpv::Value` type and `rmpv::encode`/`rmpv::decode`
modules provide all necessary serialization. The connection is a plain Unix domain socket
(`tokio::net::UnixStream`).

---

## 6. Pane Border Rendering

The `QuadRenderer` pipeline already handles arbitrary colored rectangles via
`QuadInstance { rect: [x, y, w, h], color }`. A 1px border between panes is trivially
expressed as:

- Horizontal split: a 1px-tall horizontal rectangle at the split `y` position, spanning
  the full width of the parent rect.
- Vertical split: a 1px-wide vertical rectangle at the split `x` position, spanning the
  full height of the parent rect.

Focus state: when computing border quads, compare each border's adjacent panes to
`AppState.focus`. If either adjacent pane is the focused pane, use `palette.accent`; otherwise
use `palette.dim_border` (a new palette field, e.g., dark gray).

The pane rect calculation should subtract 1 pixel from the split edge to account for the
border, so the terminal content does not overdraw it.

`MAX_QUADS = 8192` is not a constraint at Phase 3 scale. A 16-pane layout produces at most
30 border quads, well within the budget.

---

## 7. Tab Bar Rendering

The tab bar is a fixed-height horizontal strip (height = 1 cell height, aligned to top of
window). It is rendered using the same quad + text pipeline:

1. Background: one `QuadInstance` covering the full tab bar area, colored with the palette
   background.
2. Per-tab quad: a `QuadInstance` for the tab label area (active: accent color, inactive:
   dim). Each tab label occupies approximately `(tab_name.len() + 2) * cell_w` pixels.
3. Per-tab label: one `TextArea` per tab, positioned within the tab bar.
4. All pane rects start at `y = tab_bar_height_px = cell_h * scale_factor` (in physical
   pixels). The pane tree layout computation passes `available_rect = PixelRect { x:0, y:
   tab_bar_height, width: window_width, height: window_height - tab_bar_height }` as the
   root rect.

Tab bar click detection: map `(cursor_x, cursor_y)` to a tab index if `cursor_y < tab_bar_height`. Switch `active_tab` on left-click.

---

## 8. Command Palette

### Approach

The command palette is an overlay rendered on top of the terminal content. It is a modal
UI state stored on `AppState` as `palette_mode: Option<PaletteState>` where `PaletteState`
contains the current input string, the list of matching commands, and the selected index.

Rendering: when `palette_mode.is_some()`:
1. A semi-transparent dimming quad behind the palette (a large `QuadInstance` with alpha
   ~0.7).
2. A bordered rectangle for the palette box.
3. A `TextArea` for the input field.
4. One `QuadInstance` + `TextArea` per visible result row.

**Alpha blending**: The existing `QuadRenderer` pipeline already uses
`wgpu::BlendState::ALPHA_BLENDING`. A dimming quad with `color = [0.0, 0.0, 0.0, 0.7]`
will correctly composite over the terminal content, since quads are drawn before text in
the current render pass.

### Input capture

When `palette_mode.is_some()`, the `window_event(KeyboardInput)` handler routes all key
events to the palette first:
- Printable characters → append to `PaletteState.query`, recompute matches.
- Backspace → remove last character.
- Arrow up/down → change `selected_index`.
- Enter → execute selected command, close palette.
- Escape → close palette, restore normal mode.
- No events reach `translate_key_event` while the palette is open.

This is a pure state machine addition to the existing `window_event` match arm, before the
current keyboard handling logic.

### Fuzzy matching

For Phase 3 (pane/tab commands only, ~20 items), simple substring matching is sufficient.
No external crate is needed. `command_label.to_lowercase().contains(&query.to_lowercase())`
is adequate. For future phases where the palette grows to hundreds of commands, consider
**nucleo** (1.3k stars, used in helix-editor, 0.4.0 as of Feb 2024, Smith-Waterman
algorithm, pure Rust, actively maintained). Avoid **fuzzy-matcher** (archived January 2026,
no longer maintained).

---

## 9. Comparison Matrix: Neovim RPC Options

| Criteria | nvim-rs | neovim-lib | Hand-rolled (rmpv) |
|----------|---------|------------|--------------------|
| Version | ~0.6 (crates.io) | 0.5.1 | rmpv 1.3.1 |
| Stars | 266 | 192 | N/A |
| Last active | Early 2025 | January 2018 | rmpv: active |
| License | LGPL-3 + dual MIT/Apache for new code | LGPL-3 | MIT (rmpv) |
| Async runtime | tokio or async-std (feature flag) | Synchronous blocking | tokio (manual) |
| API stability | Unstable (documented) | Frozen (abandoned) | Total control |
| Connect to existing socket | Yes (`create::tokio::new_path`) | Yes | Yes (UnixStream) |
| Implementation effort | Low | Low | Medium (~200 lines) |
| Risk | API may break between releases | Does not compile on 2024 edition tools likely | Must implement RPC framing manually |
| Stack compatibility | Tokio already in Cargo.toml | Conflicts: blocking I/O in async app | Perfect fit |

## Comparison Matrix: Fuzzy Matching Options

| Criteria | nucleo | fuzzy-matcher | substring (built-in) |
|----------|--------|---------------|----------------------|
| Stars | 1,300 | 295 (archived) | N/A |
| Version | 0.4.0 | 0.4.1 (frozen) | N/A |
| Maintenance | Active (helix-editor) | Archived Jan 2026 | N/A |
| Algorithm | Smith-Waterman + affine gaps | Skim v2, clangd | str::contains |
| Match quality | Excellent | Good | Adequate for <50 items |
| Dependency weight | Moderate | Light | Zero |
| Phase 3 need | Overkill | Archived | Sufficient |

---

## Recommendation Summary

| Decision | Recommendation | Rationale |
|----------|----------------|-----------|
| Pane tree structure | Binary tree (`PaneNode` enum) | Matches CONTEXT-3.md design, type-safe invariants, trivial zoom |
| Multi-grid rendering | Single render pass with per-pane offset on quads and TextAreas | No render pass overhead; glyphon TextArea supports arbitrary left/top |
| Tab model | `Vec<Tab>` on AppState, only active tab polled | Simple; background tabs buffer in mpsc channels |
| Leader key | `KeymapState` enum, next-keypress timeout | Matches tmux model; no async timer complexity |
| Ctrl+h/j/k/l | Intercept in `window_event` before `translate_key_event` | Per CONTEXT-3.md; accepted tradeoff with shell Ctrl+h |
| Neovim detection | Process name via `/proc/<pid>/comm` (Linux) or sysctl (macOS) | No user config required for detection |
| Neovim socket | `--listen` arg scan or fd scan; fall back gracefully | Avoids requiring user alias/wrapper |
| Neovim RPC | nvim-rs for Phase 3; migrate to hand-rolled if LGPL is a concern | Lowest implementation risk; tokio-compatible |
| Pane borders | `QuadInstance` 1px lines via existing QuadRenderer | Zero new GPU infrastructure needed |
| Tab bar | Quad + TextArea, pane rects offset by tab bar height | Consistent with existing pipeline |
| Command palette | `Option<PaletteState>` on AppState, input intercepted in window_event | No new framework; renders via existing quad + text pipeline |
| Fuzzy matching | `str::contains` for Phase 3 | Sufficient for ~20 commands; add nucleo in Phase 5+ |

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| nvim-rs API breaks between versions | Medium | Medium | Pin to a specific git commit or version. The narrow API surface (3-4 calls) means changes are easy to adapt. |
| nvim-rs LGPL license conflicts with MIT binary distribution | Medium | Medium | Implement the ~200-line hand-rolled msgpack-rpc client using rmpv + tokio::UnixStream before release. |
| Neovim socket discovery fails on macOS (sysctl complexity) | Medium | Low | Fall back gracefully: if socket not found, treat as no-Neovim and do standard arcterm pane crossing. Log at debug level. |
| Glyphon TextArea coordinate offset accumulates floating-point error | Low | Low | Compute `left` and `top` from integer pixel rects; cast to f32 at submit time. Rounding error at cell boundaries is < 1 pixel. |
| `MAX_QUADS = 8192` exceeded if palette renders many results | Low | Medium | Increase `MAX_QUADS` to 16384 before Phase 3. Cost is one larger buffer allocation at startup. |
| Binary tree directional navigation is confusing at depth | Medium | Low | Implement and document the upward-traversal algorithm clearly. Add unit tests covering all split orientations. |
| Ctrl+h interception breaks readline Backspace in some shells | High | Low | Document the behavior. Provide a config option to disable Ctrl+h interception. Note: Ctrl+Backspace is a separate key event and is not intercepted. |
| Multi-pane `about_to_wait` poll loop performance | Low | Medium | All pane channels are polled with `try_recv()` (non-blocking). For N panes, this is N non-blocking channel polls per event loop iteration — negligible. |
| Tab switching with large scrollback buffers pauses the UI | Low | Medium | Background tab grids are not rendered; only PTY bytes accumulate in channels. No performance issue at tab switch time. |

---

## Implementation Considerations

### Integration points with existing code

- `AppState` struct in `main.rs`: add `tabs: Vec<Tab>`, `active_tab: usize`, `keymap_state: KeymapState`, `palette_mode: Option<PaletteState>`. Remove `terminal: Terminal` and `pty_rx`.
- `Renderer::render_frame` signature: change from `(&Grid, scale_factor)` to accept a slice of `(PixelRect, &Grid)` pairs plus overlay data (borders, tab bar, palette). Alternatively, keep `render_frame` generic and move the loop into `AppState` — the latter avoids touching the renderer's public API.
- `TextRenderer::prepare_grid`: add `offset_x: f32, offset_y: f32` parameters, or refactor to take `offset` in the `TextArea` construction loop.
- `QuadRenderer::prepare`: already accepts `&[QuadInstance]` — no signature change needed. The call site builds the full quad list including borders, tab bar, and palette.
- `input.rs::translate_key_event`: unchanged. The new leader/palette interception happens in `window_event` *before* calling this function.

### New modules to add

- `arcterm-app/src/layout.rs`: `PaneNode`, `PaneId`, layout computation (`compute_rects`), directional navigation (`focus_in_direction`), split/close operations.
- `arcterm-app/src/tab.rs`: `Tab` struct, tab switching, tab add/close.
- `arcterm-app/src/keymap.rs`: `KeymapState` enum, `handle_key_event` function.
- `arcterm-app/src/palette.rs`: `PaletteState`, command list, input handling, rendering.
- `arcterm-app/src/neovim.rs`: process detection, socket discovery, RPC calls (uses nvim-rs or hand-rolled).

### Migration path

Phase 3 does not need to maintain backward compatibility with Phase 2's single-pane
`AppState`. The migration is a restructuring of `AppState` in `main.rs`. All other crates
(`arcterm-core`, `arcterm-vt`, `arcterm-pty`, `arcterm-render`) are unchanged except for
the minor `prepare_grid` signature addition in `arcterm-render`.

### Testing strategy

- Unit tests for `layout.rs`: compute_rects for various tree shapes, directional navigation
  in all four directions, zoom toggle, resize ratio clamping.
- Unit tests for `keymap.rs`: leader key state transitions, timeout behavior, Ctrl+h
  interception.
- Unit tests for `neovim.rs`: mock Unix socket server that speaks msgpack-rpc, verify
  direction inference logic from mock window positions.
- Manual tests: open 4 panes, verify Ctrl+h/j/k/l navigation, verify Neovim integration,
  verify tab switching, verify command palette opens and executes.

### Performance implications

- N panes means N PTY reader threads (unchanged from current 1 thread). Each is a blocked
  OS thread in `pty-reader` thread pool — acceptable for typical 4-8 pane use.
- Rendering N grids: glyphon's dirty-row optimization (row hashes) applies per-pane. With
  the existing `row_hashes: Vec<u64>` in `TextRenderer`, panes with static content incur
  near-zero re-shaping cost per frame.
- The tab-switch cost is O(1) — only the active tab's panes are polled and rendered.

---

## Sources

1. Codebase: `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs` (AppState, App, window_event handler)
2. Codebase: `/Users/lgbarn/Personal/myterm/arcterm-app/src/terminal.rs` (Terminal struct)
3. Codebase: `/Users/lgbarn/Personal/myterm/arcterm-render/src/renderer.rs` (render_frame pipeline)
4. Codebase: `/Users/lgbarn/Personal/myterm/arcterm-render/src/quad.rs` (QuadRenderer, MAX_QUADS, QuadInstance)
5. Codebase: `/Users/lgbarn/Personal/myterm/arcterm-render/src/text.rs` (TextRenderer, TextArea, prepare_grid)
6. Codebase: `/Users/lgbarn/Personal/myterm/arcterm-app/src/input.rs` (translate_key_event, Ctrl+h = 0x08)
7. Codebase: `/Users/lgbarn/Personal/myterm/arcterm-pty/src/session.rs` (PtySession, mpsc channel buffer=64)
8. Codebase: `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs` (Grid, GridSize)
9. Codebase: `/Users/lgbarn/Personal/myterm/.shipyard/phases/3/CONTEXT-3.md` (design decisions)
10. Codebase: `/Users/lgbarn/Personal/myterm/.shipyard/ROADMAP.md` (Phase 3 scope and success criteria)
11. `https://github.com/KillTheMule/nvim-rs` — nvim-rs: 266 stars, LGPL-3, tokio feature, `create::tokio::new_path` for socket connection, API unstable
12. `https://raw.githubusercontent.com/KillTheMule/nvim-rs/master/src/create/tokio.rs` — `new_path()` uses `tokio::net::UnixStream::connect(path)`
13. `https://github.com/daa84/neovim-lib` — neovim-lib: 192 stars, v0.5.1, last release January 2018, abandoned
14. `https://docs.rs/rmpv/latest/rmpv/` — rmpv 1.3.1: msgpack Value types, encode/decode modules, MIT, actively maintained
15. `https://raw.githubusercontent.com/neovim/neovim/master/runtime/doc/api.txt` — nvim_list_wins returns window ID array; nvim_win_get_position returns [row, col] in grid cells; external connection via msgpack-rpc; $NVIM env var used by child processes
16. `https://raw.githubusercontent.com/neovim/neovim/master/runtime/doc/starting.txt` — `--listen <addr>` sets the primary listen address / v:servername
17. `https://raw.githubusercontent.com/tmux/tmux/master/server-client.c` — prefix key table switching, `server_client_key_table_activity_diff()` for timeout, `prefix-timeout` option
18. `https://raw.githubusercontent.com/zellij-org/zellij/main/zellij-utils/src/pane_size.rs` — PaneGeom, Dimension (Fixed/Percent), split_out(), combine_*_with()
19. `https://raw.githubusercontent.com/zellij-org/zellij/main/zellij-server/src/panes/tiled_panes/mod.rs` — BTreeMap storage, active_panes, directional navigation via TiledPaneGrid
20. `https://raw.githubusercontent.com/zellij-org/zellij/main/zellij-server/src/panes/tiled_panes/tiled_pane_grid.rs` — directional navigation using is_directly_*_of() + horizontally/vertically_overlaps_with()
21. `https://github.com/helix-editor/nucleo` — nucleo 0.4.0, 1.3k stars, Smith-Waterman fuzzy matching, used in helix-editor, production-ready
22. `https://github.com/lotabout/fuzzy-matcher` — fuzzy-matcher 295 stars, archived January 22, 2026, no longer maintained
23. `https://github.com/little-dude/rmp-rpc` — rmp-rpc 0.3.0, 46 stars, inactive, not recommended

---

## Uncertainty Flags

1. **Neovim socket discovery on macOS without `--listen`**: The approach of scanning
   `/proc/<pid>/fd` is Linux-specific. The macOS equivalent (parsing `lsof` output or
   using the `proc_info` syscall with `PROC_PID_FD`) is complex and poorly documented.
   Further investigation of `libproc` on macOS is needed before implementing socket
   auto-discovery. The simplest fallback is requiring the user to launch Neovim with
   `--listen`.

2. **nvim-rs LGPL compliance**: The exact implications of LGPL-3 for a statically linked
   Rust binary (which is the default for Rust) are uncertain and legally contested. Legal
   review or switching to the hand-rolled approach is recommended before the project's
   first public release.

3. **glyphon TextArea bounds clipping**: The `TextBounds` field on `TextArea` clips
   rendering to a rectangle. Whether this correctly clips pane text to the pane's pixel
   rect (preventing overflow into adjacent panes) needs to be verified empirically. Setting
   `bounds = TextBounds { left: rect.x, top: rect.y, right: rect.x + rect.w, bottom: rect.y + rect.h }` should work based on the field semantics, but this has not been
   tested in the current codebase.

4. **`portable_pty::Child` PID access**: The `portable_pty` crate's `Child` trait does not
   expose a `pid()` method in the public API as of v0.9. Whether the concrete type behind
   the `Box<dyn Child>` can be downcast to access the PID needs verification against the
   `portable_pty` v0.9 source. An alternative is to use OS-level process enumeration (find
   all child processes of the arcterm process and match by creation time).

5. **Neovim RPC response ordering**: The Neovim API documentation states "Responses must be
   given in reverse order of requests (like unwinding a stack)." For the simple sequential
   query pattern used in Arcterm (send one request, wait for response, send next), this
   constraint has no practical impact. However, if queries are pipelined for performance,
   this ordering constraint must be respected by the RPC client implementation.

6. **Tab bar text rendering position**: Glyphon's `TextArea.top` is a pixel offset from
   the top of the viewport (not from the tab bar baseline). Whether glyphon's text
   renderer clips correctly within the tab bar region (preventing tab labels from bleeding
   into pane content below) should be verified using `TextBounds`. The `Viewport` struct
   controls the visible area for all text — it may be necessary to issue a separate
   `prepare`/`render` call for the tab bar if a shared Viewport clips incorrectly.
