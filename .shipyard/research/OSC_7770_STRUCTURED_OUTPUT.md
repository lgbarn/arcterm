# Research: OSC 7770 Structured Output

## Context

ArcTerm is a Rust workspace fork of WezTerm (~60 crates). The planned OSC 7770 feature renders rich
content (syntax-highlighted code blocks, interactive JSON trees, side-by-side diffs, and inline
images) directly in the terminal via a custom escape sequence. The feature specification lives at
`specs/003-structured-output-osc7770/spec.md`.

This research covers four prerequisite topics: (1) how WezTerm's existing OSC parsing infrastructure
works and where OSC 7770 hooks in, (2) how the GPU renderer handles non-text content today, (3) which
Rust crate to use for syntax highlighting, and (4) what existing rich terminal protocols do and what
WezTerm already implements.

---

## Topic 1: WezTerm's OSC Parsing Infrastructure

### Data Flow: raw bytes to terminal model

The full pipeline, as confirmed by codebase inspection:

```
PTY master fd
  │
  └─ mux/src/lib.rs: read_from_pane_pty()  [blocking reader thread per pane]
       │ raw bytes → socketpair
       └─ parse_buffered_data() [parser thread per pane, line 140]
            │ termwiz::escape::parser::Parser::new() — wraps VTParser (vtparse crate)
            │ parser.parse(&buf, |action| { ... action.append_to(&mut actions) })
            └─ send_actions_to_mux()
                 └─ pane.perform_actions(actions)
                      └─ term/src/terminalstate/performer.rs: Performer::perform(action)
                           └─ match action {
                               Action::OperatingSystemCommand(osc) => self.osc_dispatch(*osc)
                             }
```

Source: `mux/src/lib.rs` lines 140–235 (observed); `term/src/terminalstate/performer.rs`
lines 252–288 (observed).

### The OSC parser: `wezterm-escape-parser`

The escape sequence parser lives in the `wezterm-escape-parser` crate. The key types are:

- **`wezterm_escape_parser::parser::Parser`** wraps `vtparse::VTParser`. When the VT state machine
  identifies an OSC sequence, it hands the semicolon-delimited byte slices to
  `OperatingSystemCommand::parse(&[&[u8]])`.

- **`OperatingSystemCommand::parse()`** (`wezterm-escape-parser/src/osc.rs` line 155): calls
  `internal_parse()`, which looks up the numeric prefix in a static `OscMap` (a `HashMap<&str,
  OperatingSystemCommandCode>`). If the numeric code is not in the map, `internal_parse()` returns
  `Err`, and `parse()` falls back to:
  ```rust
  OperatingSystemCommand::Unspecified(Vec<Vec<u8>>)
  ```
  This is the critical path for OSC 7770: because `"7770"` is not in the `osc_entries!` macro table
  (observed at `osc.rs` line 457–505), an OSC 7770 sequence today produces
  `OperatingSystemCommand::Unspecified(vec![b"7770".to_vec(), <payload bytes>])`.

- **`Performer::osc_dispatch()`** (`performer.rs` line 737): the `Unspecified` arm at line 772 logs
  a warning only if `config.log_unknown_escape_sequences()` is true, then **silently drops** the
  sequence. This is the correct graceful-degradation behavior in non-ArcTerm terminals.

Source: `wezterm-escape-parser/src/osc.rs` lines 33–55, 154–167, 256–278, 457–505 (observed);
`term/src/terminalstate/performer.rs` lines 772–782 (observed).

### Registered OSC sequence codes (current exhaustive list)

| Code | Meaning |
|------|---------|
| 0 | SetIconNameAndWindowTitle |
| 1 | SetIconName |
| 2 | SetWindowTitle |
| 4 | ChangeColorNumber |
| 7 | SetCurrentWorkingDirectory (CWD) |
| 8 | SetHyperlink |
| 9 | SystemNotification (iTerm2) |
| 10–19 | Dynamic color set/query |
| 46 | SetLogFileName |
| 50 | SetFont |
| 51 | EmacsShell |
| 52 | ManipulateSelectionData (clipboard) |
| 104, 105, 110–119 | ResetColors, ResetSpecialColor, etc. |
| 133 | FinalTermSemanticPrompt (shell integration) |
| 777 | RxvtProprietary |
| 1337 | ITermProprietary (images, user vars, unicode version) |

`7770` is not registered. Source: `wezterm-escape-parser/src/osc.rs` lines 457–505 (observed).

### How OSC sequences are stored in scrollback

OSC sequences with side effects (images, hyperlinks, colors) are **not stored as OSC tokens** in the
scrollback buffer. Their effects are stored as cell-level attributes:

- **Hyperlinks**: stored as `Arc<Hyperlink>` on `CellAttributes` (via `set_hyperlink()`).
- **Images** (iTerm2/Kitty/Sixel): stored as `ImageCell` attached to `CellAttributes` via
  `cell.attrs_mut().set_image(img)` or `attach_image(img)`. Each cell covering an image stores a
  texture coordinate slice into the image data.
- **Semantic zones**: stored as `SemanticType` on the current `pen` attributes, which are then baked
  into each printed cell.

For OSC 7770's rich content blocks, this means a new storage mechanism is required. The existing
model of "every unit of content fits in a grid cell" does not accommodate an arbitrarily-tall
structured block that spans many rows. The most practical approach (confirmed by the image protocol
precedent) is to: (a) reserve a run of blank rows in the scrollback at parse time, (b) attach a
marker on the first row (analogous to `ImageCell`) that identifies the block, and (c) in the
renderer, detect that marker and call a custom rendering path.

Source: `term/src/terminalstate/image.rs` lines 192–216 (observed); `term/src/terminalstate/iterm.rs`
lines 132–151 (observed).

### Where to add OSC 7770 support

There are two required touch points and one optional one:

1. **`wezterm-escape-parser/src/osc.rs`** — Add `"7770"` to the `osc_entries!` macro and a new
   `OperatingSystemCommand::ArcTermStructuredOutput(StructuredBlock)` variant. This is the only
   change required to the parser crate.

2. **`term/src/terminalstate/performer.rs`** — Add a match arm for
   `OperatingSystemCommand::ArcTermStructuredOutput(block)` inside `osc_dispatch()`. The handler
   calls a new `self.set_structured_block(block)` method (analogous to `self.set_image()`).

3. **`arcterm-structured-output` crate** (new, optional but recommended) — Contains the JSON
   payload parser, block type definitions, and the per-block-type rendering logic. Keeps all new
   code isolated from the WezTerm core to minimize merge conflicts with upstream.

---

## Topic 2: WezTerm's GPU Renderer

### How the renderer iterates terminal lines

`paint_pane()` in `wezterm-gui/src/termwindow/render/pane.rs` iterates visible rows and calls
`render_screen_line()` in `screen_line.rs` for each. The screen line renderer maps each cell to
a GPU quad via `TripleLayerQuadAllocator`. The quad system has three layers: background fill, text
glyphs, and overlay (used for images that draw over text).

Source: `wezterm-gui/src/termwindow/render/pane.rs` lines 32–39 (observed); `screen_line.rs`
lines 422–710 (observed).

### How images are rendered today

Images are stored as `ImageCell` values attached to `CellAttributes`. Inside `render_screen_line()`:

1. For each `CellCluster`, the renderer calls `cluster.attrs.images()` (line 436).
2. For each `ImageCell`, it calls `self.populate_image_quad(...)` (line 474).
3. `populate_image_quad()` (`render/mod.rs` line 441): retrieves a cached `Sprite` from `GlyphCache`
   via `cached_image()`, then sets texture coordinates and position on a GPU quad.

The texture atlas is managed by `GlyphCache` (`glyphcache.rs`), which uses `guillotiere` for
rectangle packing. Images are decoded by the `image` crate (v0.25) and uploaded to GPU texture
memory on first use.

Source: `wezterm-gui/src/termwindow/render/mod.rs` lines 440–519 (observed);
`wezterm-gui/src/termwindow/render/screen_line.rs` lines 422–710 (observed).

### What image protocols WezTerm already implements

All three major terminal image protocols are implemented:

| Protocol | Escape type | Implementation file | Status |
|----------|-------------|--------------------|----|
| iTerm2 (OSC 1337) | OSC | `term/src/terminalstate/iterm.rs` | Full (production) |
| Sixel (DCS) | Device Control | `term/src/terminalstate/sixel.rs` | Experimental |
| Kitty (APC) | APC | `term/src/terminalstate/kitty.rs` | Supported (config flag required) |

WezTerm's own documentation (`wezterm.org/imgcat.html`) confirms iTerm2 protocol support with the
`wezterm imgcat` subcommand. Kitty graphics can be enabled with `enable_kitty_graphics = true` in
config.

Source: `term/src/terminalstate/` directory listing (observed); `wezterm.org/imgcat.html` (fetched);
HN comment confirming Sixel/Kitty/iTerm2 all supported [6].

### What the "allow_images" guard does

`populate_image_quad()` checks `self.allow_images == AllowImage::No` at line 452 and returns early
if images are disabled. This is a config-level switch. OSC 7770 should introduce its own
`enable_structured_output` config flag following the same pattern.

### Implications for OSC 7770 rendering

The existing image rendering mechanism (cells carrying `ImageCell` texture coordinates) is the right
model for OSC 7770's `image` type. For the other content types (code, JSON, diff), the approach
must be different because the content is character-based (colored text runs), not a pixel texture.

The most compatible approach for code/JSON/diff rendering: at parse time, apply syntax highlighting
to produce `Action::Print` sequences with appropriate `SGR` color attributes. This reuses the
entire existing text rendering pipeline without requiring any new GPU rendering code. The structured
block's plain-text representation occupies terminal rows exactly like normal output. This is how
every other terminal-based syntax highlighter (bat, delta) works in practice.

For the image type, reuse `assign_image_to_cells()` with `ImageAttachStyle::Iterm` — the
infrastructure is already present.

---

## Comparison Matrix: Syntax Highlighting Libraries

| Criteria | syntect | tree-sitter-highlight | bat (uses syntect) |
|----------|---------|----------------------|--------------------|
| Version | 5.3.0 | 0.26.7 | N/A (not a library) |
| Maturity | ~9 years (2016) | ~7 years (2019) | N/A |
| Total crates.io downloads | 12,506,860 | 3,040,642 | N/A |
| Recent downloads | 3,119,976 | 451,733 | N/A |
| GitHub stars | 2.3k | ~16k (tree-sitter repo) | ~50k (bat) |
| Last release | Sep 27, 2025 | Mar 14, 2026 | N/A |
| Grammars included by default | ~40 (Sublime Text defaults) | None (load per-language) | ~200 (via syntect-assets) |
| Language grammar format | Sublime Text `.sublime-syntax` | Tree-sitter queries (`.scm`) | Sublime Text |
| License | MIT | MIT | N/A |
| Syntax-only mode (no regex) | No (uses regex engine) | Yes (incremental parse) | N/A |
| Binary size contribution | Moderate (~500 KB+ for grammar bundle) | Large (per-language `.so` or WASM) | N/A |
| Incremental parsing | No | Yes | No |
| Accuracy | High (battle-tested, bat/Zola/xi-editor) | Very high (AST-based) | High |
| Learning curve | Low | Medium-High | N/A |
| Stack compatibility | Pure Rust, no unsafe required | Unsafe C bindings via bindgen | N/A |
| WezTerm already uses? | No | No | No |

---

## Topic 3: Syntax Highlighting in Rust — Detailed Analysis

### Option A: syntect

**Strengths:**
- Used in production by `bat` (50k GitHub stars), `Zola`, `xi-editor`, `delta`, and many others.
  This is the most battle-tested Rust highlighting library.
- Sublime Text syntax definitions are a known quantity. They support all 14+ languages specified
  in the OSC 7770 spec (`specs/003-structured-output-osc7770/spec.md` line 131) and hundreds more.
- The API for ArcTerm's use case is straightforward: `SyntaxSet::load_defaults()` + `LassoTheme`
  + `ClassHighlighter` produces a stream of `(style, text)` tuples that map directly to SGR escape
  sequences. No async, no thread concerns.
- The `syntect-assets` crate (a companion, 500+ syntaxes from bat's collection) can be optionally
  linked for broader language coverage.
- Pure Rust with no C FFI required. Fits naturally into the existing build system.
- MIT licensed. The existing `deny.toml` is already accepting MIT dependencies.

**Weaknesses:**
- The project description on GitHub explicitly says it is "mostly complete" and "not under heavy
  development." The maintainer (trishume) merges PRs but is not adding major features. Last release
  was September 2025 — this is consistent with a mature, stable library, not an abandoned one.
- Grammars are loaded from a binary dump of Sublime Text's defaults. The default set has
  approximately 40 languages; covering 14 specific languages requires using `syntect-assets` (bat's
  larger bundle) or shipping custom `.sublime-syntax` files.
- No incremental parsing: for very long files (500+ lines per the spec's SC-004 requirement), the
  full highlight pass runs on parse. For a terminal, this is one-shot output; incremental parsing
  is not relevant because terminal output is not re-edited in place.

**Integration notes:**
- No conflicts with existing ArcTerm dependencies. syntect uses `regex` and `onig` (optional) for
  its regex engine; WezTerm already uses `fancy-regex`. These do not conflict.
- To achieve the spec's SC-001 (14+ languages), add `syntect-assets` as a dependency of the new
  `arcterm-structured-output` crate. This ships bat's grammar bundle, which includes Python,
  JavaScript, TypeScript, Rust, Go, Java, C/C++, Ruby, Bash, YAML, TOML, JSON, Markdown, HTML/CSS,
  SQL, and 200+ more.
- Output from syntect is `(Style, &str)` tuples. Mapping to SGR: for each tuple emit
  `\x1b[38;2;R;G;Bm` (truecolor foreground) + text + `\x1b[0m`. These are fed as
  `Action::Print` sequences through the existing `Performer::flush_print()` path.

Source: crates.io API (fetched); GitHub trishume/syntect (fetched); bat README.

### Option B: tree-sitter-highlight

**Strengths:**
- AST-based parsing produces more accurate highlighting than regex-based approaches (no
  mis-highlighting of multi-line strings or nested constructs).
- Actively developed: version 0.26.7 was released March 14, 2026.
- The `tree-sitter-highlight` crate produces a `Highlight` iterator yielding `ScopeStart`,
  `ScopeEnd`, and `Source` events — structurally similar to syntect's output.

**Weaknesses:**
- **No bundled grammars.** Each language requires a separate `tree-sitter-<lang>` crate (e.g.,
  `tree-sitter-python`, `tree-sitter-rust`). Supporting 14 languages means adding 14 grammar
  crates. Each contains C code compiled via `cc` build scripts. This substantially increases build
  time and binary size.
- The C FFI via `bindgen` adds complexity to cross-compilation and to `cargo-deny` license audits.
  WezTerm already vendors several C libraries (HarfBuzz, FreeType, Cairo); adding 14 more C grammar
  libraries increases maintenance burden.
- The highlight configuration (`.scm` query files) must be loaded per-language from the filesystem
  or embedded at compile time; there is no ready-made bundled distribution equivalent to
  `syntect-assets`.
- Learning curve is higher: requires understanding of tree queries, scope mappings, and the
  highlight theme format. syntect's simpler API is a better match for ArcTerm's use case (one-shot
  terminal output highlighting, not editor integration).

**Integration notes:**
- Would require 14+ new C-code build dependencies, one per language. This conflicts with the
  CLAUDE.md convention of keeping ArcTerm-specific code in `arcterm-*` crates with minimal upstream
  merge conflicts — adding many C grammar build scripts touches the root `Cargo.toml` and the
  workspace's compile time significantly.

### Option C: bat (terminal output tool, not a library)

Bat (`github.com/sharkdp/bat`) itself is a binary, not a library. It uses syntect internally. It
is included here only as an ecosystem reference: bat's grammar bundle (`syntect-assets`) is the
practical way to get 200+ syntect grammars without maintaining them.

---

## Topic 4: Existing Rich Terminal Protocols

### iTerm2 Inline Image Protocol (OSC 1337)

**Format:**
```
ESC ] 1337 ; File = [key=value;...] : <base64 encoded file data> BEL
```
(BEL = `\x07`; ST = `ESC \` is also accepted as terminator)

**Key arguments:** `name` (base64 filename), `size` (bytes), `width` (cells/pixels/% or `auto`),
`height` (cells/pixels/% or `auto`), `preserveAspectRatio` (0 or 1), `inline` (0 or 1).

**Payload:** The file content (PNG, JPEG, GIF, WebP, PDF, etc.) is base64-encoded and placed after
the `:` separator. The entire payload must fit in a single escape sequence. A multipart variant
(iTerm2 3.5+) splits large files across multiple sequences.

**Standard behavior in unsupporting terminals:** Because this is an OSC sequence, compliant
terminals that do not recognize code `1337` produce `OperatingSystemCommand::Unspecified(...)` and
silently drop it. This is the same path OSC 7770 follows today in ArcTerm.

Source: `https://iterm2.com/documentation-images.html` (fetched); `term/src/terminalstate/iterm.rs`
(observed in ArcTerm).

### Kitty Graphics Protocol (APC)

**Format:**
```
ESC _ G <control data> ; <base64 payload> ESC \
```
Uses **APC** (Application Programming Command, byte `0x9F`), not OSC. The `G` immediately follows
`ESC _`. This is specifically chosen because "most terminal emulators ignore APC codes."

**Key fields:** `a` (action: T=transmit, p=place, d=delete), `f` (format: 24=RGB, 32=RGBA,
100=PNG), `s`/`v` (width/height in pixels), `t` (transmission mode: d=direct, f=file, s=shared
memory), `m` (more chunks: 0=last, 1=more).

**Chunking:** Payload split into chunks ≤4096 bytes. Only the first chunk carries full metadata;
subsequent chunks need only `m` key.

**Key differences from iTerm2:** APC vs OSC container; pixel-level placement (not character-cell);
z-index/alpha compositing; animation frames; shared memory transfer; explicit delete operations.

**Standard behavior in unsupporting terminals:** APC sequences are discarded by all standard
terminal parsers that do not explicitly handle them. vtparse (used by WezTerm/ArcTerm) handles
unknown APC sequences via the same "ignore" path.

Source: `https://sw.kovidgoyal.net/kitty/graphics-protocol/` (fetched).

### Standard for handling unknown OSC sequences

ECMA-48 specifies that terminals MUST silently discard OSC sequences with unrecognized command
codes. The xterm documentation (`invisible-island.net/xterm/ctlseqs/ctlseqs.html`) and iTerm2
documentation both confirm this behavior. The OSC 8 hyperlink specification (the primary recent
addition to the OSC ecosystem) explicitly relies on this guarantee: "terminals that correctly
implement OSC parsing according to ECMA-48 are guaranteed not to suffer from compatibility issues."

**Verification against ArcTerm's implementation:** Confirmed in codebase. `OperatingSystemCommand::
parse()` maps unrecognized numeric codes to `OperatingSystemCommand::Unspecified(...)`. The
`Performer::osc_dispatch()` `Unspecified` arm logs at `warn` level (only if
`log_unknown_escape_sequences()` is true) and returns without action. No bytes are printed to the
screen. OSC 7770 sequences sent to a non-ArcTerm WezTerm instance or to any standard xterm-family
terminal will be silently consumed. Scenario 3 of the spec's User Story 1 is satisfied by the
existing behavior.

Source: `term/src/terminalstate/performer.rs` lines 772–782 (observed); ECMA-48 §8.3.89
(reference); OSC 8 hyperlink specification (reference).

### WezTerm's current OSC 1337 (iTerm2) image support

WezTerm fully implements the iTerm2 inline image protocol. The implementation is production-quality:

- `ITermProprietary::File(image)` case in `osc_dispatch()` calls `self.set_image(image)` which is
  defined in `term/src/terminalstate/iterm.rs`.
- `set_image()` decodes the image with the `image` crate (v0.25), computes pixel-to-cell
  dimensions, then calls `assign_image_to_cells()` — the shared image-to-cell attachment function.
- The image texture is cached in `GlyphCache` and uploaded to the GPU texture atlas on first render.
- WezTerm adds a proprietary extension: `doNotMoveCursor=1` prevents cursor advancement after image
  display.

For OSC 7770's `type: "image"` content, the **simplest implementation path** is to decode the
payload image data and call `assign_image_to_cells()` directly — reusing 100% of the existing
image infrastructure. No new GPU code is needed for image support.

Source: `term/src/terminalstate/iterm.rs` (observed); `term/src/terminalstate/image.rs` (observed);
`wezterm-gui/src/termwindow/render/mod.rs` lines 440–519 (observed).

---

## Recommendation: Syntax Highlighting

**Selected: syntect v5.3.0 + syntect-assets**

syntect is the only viable choice for ArcTerm's use case. The decision criteria:

1. **No C FFI.** ArcTerm already maintains a large C FFI surface (HarfBuzz, FreeType, Cairo,
   OpenSSL). Adding 14 tree-sitter grammar crates would significantly increase build complexity,
   cross-compilation burden, and binary size. syntect is pure Rust.

2. **Language coverage without per-language crates.** The spec mandates 14+ languages (SC-001).
   syntect-assets provides 200+ language definitions in a single compiled-in bundle. tree-sitter
   requires one crate per language.

3. **Output format maps directly to existing rendering.** syntect produces `(Style, &str)` tuples.
   Converting these to SGR sequences and feeding them as `Action::Print` into the existing
   `Performer` requires zero changes to the GPU renderer or the cell model.

4. **Production-proven.** bat, Zola, and delta are production tools used by millions. syntect is
   not experimental.

5. **Maintenance posture is appropriate.** syntect being "mostly complete" is a feature for
   ArcTerm, not a liability. ArcTerm does not need syntect to add new features; it needs it to
   reliably highlight code. A stable, well-tested library is preferred over an actively-evolving one
   in this role.

tree-sitter-highlight was not chosen because its grammar distribution model (one C crate per
language) conflicts with ArcTerm's goal of keeping ArcTerm-specific code isolated and minimizing
build complexity. The accuracy benefit of AST-based parsing is irrelevant for terminal output
highlighting, where the content is read-only and one-shot.

---

## Recommendation: OSC 7770 Integration Architecture

**Finding: OSC 7770 should render as ANSI text, not as custom GPU elements, for code/JSON/diff.**

Based on analysis of the existing rendering pipeline:

- The GPU renderer has no facility for "custom widget at row N." All content is cells.
- The image protocol's `assign_image_to_cells()` works by attaching texture data to cells — but
  code, JSON, and diff content is text, not pixels.
- The most compatible path: the OSC 7770 handler converts structured content to a sequence of ANSI
  SGR-colored `Action::Print` calls. This requires no changes to the renderer, respects scrollback,
  works with existing copy-to-clipboard (copies visible text), and handles terminal resize via the
  existing line-reflow mechanism.
- For the `image` type, `assign_image_to_cells()` from `term/src/terminalstate/image.rs` is
  directly reusable.

**The JSON tree interactive feature (FR-004, collapsible nodes) requires special handling.** No
existing mechanism supports interactive in-line terminal widgets with state. This either requires
a separate overlay pane (analogous to the existing `QuickSelect`, `CopyMode`, `DebugOverlay`
overlays in `wezterm-gui/src/overlay/`) or a simplified non-interactive rendering for v1 (just
pretty-printed, color-coded JSON). The spec lists JSON tree as P2; the overlay approach should be
designed after the P1 code-block feature is working.

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| OSC 7770 payload size causes memory pressure (spec allows 10MB) | Medium | Medium | Enforce configurable `max_osc_payload_size` in `parse_buffered_data()` before the payload reaches `OperatingSystemCommand::parse()`. Reject and log oversized payloads early. |
| syntect grammar bundle increases binary size unacceptably | Medium | Low | Make `syntect-assets` optional behind a Cargo feature flag. Default to syntect's built-in ~40-language bundle; opt-in to full 200+ language bundle. |
| JSON interactive tree (FR-004) is architecturally incompatible with the cell model | High | Medium | For v1, render JSON as static pretty-printed colored text (FR-007 fallback applies). Design the interactive tree as a separate overlay pane in a follow-on feature. |
| Upstream WezTerm adds OSC 7770 handling that conflicts with ArcTerm's variant | Low | Medium | The number 7770 is chosen to be well clear of all existing OSC codes. Upstream WezTerm has no plans for this code. Monitor upstream OSC additions with each periodic merge. |
| SGR output from syntect conflicts with the terminal's current color scheme (FR-015) | Medium | Medium | Map syntect theme colors relative to the terminal's background (dark vs. light). Use a syntect `Theme` that respects the configured color scheme, or provide a config option for the highlighting theme name. |
| Scrollback re-render of large structured blocks causes jank | Medium | Medium | Cache the pre-rendered ANSI byte sequence for each structured block keyed by (block-id, terminal-width). Invalidate cache only on terminal resize. |
| truncated OSC 7770 sequences (connection drop mid-sequence) | Low | Low | vtparse/VTParser already handles truncated sequences correctly: if ST never arrives, the accumulator is discarded when the next non-OSC byte arrives. ArcTerm inherits this behavior. |
| syntect panics on malformed input | Low | High | Wrap all syntect calls in `std::panic::catch_unwind()`. Fall back to plain text if highlighting fails. This is a documented risk with the `onig` backend; use the default pure-Rust `fancy-regex` backend in syntect (via `default-features = false, features = ["default-syntaxes"]`). |

---

## Implementation Considerations

### Integration points in existing code

1. **`wezterm-escape-parser/src/osc.rs`**: Add `ArcTermStructuredOutput = "7770"` to the
   `osc_entries!` macro. Add `OperatingSystemCommand::ArcTermStructuredOutput(StructuredBlock)` to
   the `OperatingSystemCommand` enum. Add a parse arm in `internal_parse()` that deserializes the
   semicolon-separated payload as JSON.

2. **`term/src/terminalstate/performer.rs`**: Add a match arm in `osc_dispatch()` for
   `ArcTermStructuredOutput`. Delegate to a new
   `term/src/terminalstate/arcterm_structured.rs` (following the pattern of `iterm.rs`, `kitty.rs`).

3. **`arcterm-structured-output` crate** (new): Contains the `StructuredBlock` type, the JSON
   schema, the per-type rendering logic (syntect for code, serde_json for JSON, a diff parser for
   diffs, `assign_image_to_cells()` call for images). Depends on `term`, `syntect`, `serde_json`.

4. **`env-bootstrap/src/lib.rs`**: No change needed unless Lua-level config for structured output
   is desired. A `structured_output_syntax_theme: String` config field on `Config` is recommended
   but not required for P1.

5. **`wezterm-gui/src/termwindow/render/`**: No changes required for P1 (code blocks rendered as
   ANSI text). Changes required for JSON interactive tree (P2 follow-on).

### Migration path

No existing feature is replaced. The structured output system is entirely additive. The only
observable change to existing behavior is that OSC 7770 sequences no longer emit a `log::warn` in
non-ArcTerm debug builds.

### Testing strategy

- **Unit tests in `arcterm-structured-output`**: Parse a JSON payload for each content type; verify
  the produced `Action` list contains correct SGR sequences.
- **Integration test**: Feed a raw OSC 7770 byte sequence through `termwiz::escape::parser::Parser`
  and verify it produces `Action::OperatingSystemCommand(ArcTermStructuredOutput(...))`.
- **Round-trip test**: Feed the resulting `Action` list through `TerminalState::perform_actions()`
  and verify the scrollback contains the expected colored text cells.
- **Graceful degradation test**: Feed an OSC 7770 sequence to a `vtparse` VTParser running the
  unmodified upstream osc_entries table; verify `Unspecified(...)` is produced and no bytes appear
  on screen.
- **Malformed input test**: Feed truncated, oversized, and invalid-JSON payloads; verify no panic
  and log-only behavior.

### Performance implications

- syntect's one-shot highlight for a 500-line code block takes <1ms on modern hardware (bat
  processes thousands of lines per second in practice). This is well within the SC-003 requirement
  of sub-100ms block rendering.
- The `parse_buffered_data()` thread already coalesces actions before dispatching. OSC 7770
  handling happens in the same thread as all other OSC parsing; a 10MB payload cap prevents
  unbounded allocation.
- The ANSI-text rendering approach means structured blocks use the same `GlyphCache` / shaping
  pipeline as normal text. No additional GPU resources are consumed per block.

---

## Sources

1. `term/src/terminalstate/performer.rs` — `osc_dispatch()`, `Performer::perform()` (observed)
2. `wezterm-escape-parser/src/osc.rs` — `OperatingSystemCommand::parse()`, `osc_entries!` macro,
   `internal_parse()` (observed)
3. `mux/src/lib.rs` lines 140–235 — `parse_buffered_data()`, `send_actions_to_mux()` (observed)
4. `term/src/terminalstate/iterm.rs` — iTerm2 image protocol implementation (observed)
5. `term/src/terminalstate/image.rs` — `assign_image_to_cells()`, `ImageAttachStyle` (observed)
6. `wezterm-gui/src/termwindow/render/mod.rs` lines 440–519 — `populate_image_quad()` (observed)
7. `wezterm-gui/src/termwindow/render/screen_line.rs` lines 422–710 — image rendering path (observed)
8. `specs/003-structured-output-osc7770/spec.md` — Feature specification (observed)
9. `.shipyard/codebase/ARCHITECTURE.md` — ArcTerm architecture, layer descriptions (observed)
10. `.shipyard/codebase/STACK.md` — Dependency inventory, GPU backends (observed)
11. [https://crates.io/api/v1/crates/syntect](https://crates.io/api/v1/crates/syntect) — syntect v5.3.0, 12.5M downloads, last updated Sep 27 2025
12. [https://github.com/trishume/syntect](https://github.com/trishume/syntect) — 2.3k stars, 61 open issues, "mostly complete" status
13. [https://crates.io/api/v1/crates/tree-sitter-highlight](https://crates.io/api/v1/crates/tree-sitter-highlight) — v0.26.7, 3.04M downloads, last updated Mar 14 2026
14. [https://iterm2.com/documentation-images.html](https://iterm2.com/documentation-images.html) — iTerm2 OSC 1337 protocol specification (fetched)
15. [https://sw.kovidgoyal.net/kitty/graphics-protocol/](https://sw.kovidgoyal.net/kitty/graphics-protocol/) — Kitty APC graphics protocol specification (fetched)
16. [https://news.ycombinator.com/item?id=32159968](https://news.ycombinator.com/item?id=32159968) — WezTerm supports kitty/sixel/iterm2 (source confirmation)
17. [https://wezterm.org/imgcat.html](https://wezterm.org/imgcat.html) — WezTerm iTerm2 protocol documentation (fetched)
18. [https://invisible-island.net/xterm/ctlseqs/ctlseqs.html](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html) — xterm control sequence reference, OSC silent-ignore behavior
19. [https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda) — OSC 8 hyperlink spec, ECMA-48 silent-ignore guarantee

---

## Uncertainty Flags

- **Number of syntect built-in grammars**: The exact count in syntect's compressed default bundle
  is not clearly documented. The `syntect-assets` companion crate lists 500+ grammars; the default
  bundle covers fewer. Whether Python, JavaScript, TypeScript, Rust, Go, Java, C/C++, Ruby, Bash,
  YAML, TOML, JSON, Markdown, HTML/CSS, and SQL are all in the default bundle (without
  `syntect-assets`) was not verified. This should be confirmed before deciding whether
  `syntect-assets` is required for the 14-language SC-001 requirement.

- **JSON interactive tree architecture**: The spec (FR-004) requires collapsible nodes with keyboard
  navigation. The cell model cannot represent interactive state. The overlay-pane approach (analogous
  to `CopyMode`) is the only identified path, but it requires significant GUI work beyond OSC parsing.
  Whether FR-004 is feasible in the initial feature branch without an overlay pane was not fully
  analyzed. It is likely a P2 follow-on that should be scoped separately.

- **OSC payload size limit in vtparse/VTParser**: The `vtparse` crate's internal accumulator buffer
  has a size limit. It is unknown whether a 10MB OSC payload (the spec's default limit in FR-009)
  would cause vtparse to truncate or error before the payload reaches ArcTerm's OSC handler. This
  should be tested before committing to the 10MB default limit.

- **syntect thread safety**: syntect's `SyntaxSet` and `ThemeSet` are `Send + Sync` when compiled
  with the `default-features` (which uses `fancy-regex`). However, syntect with the `onig` feature
  (which links libonig C library) is not `Send`. ArcTerm must not enable the `onig` feature.
  This should be confirmed when adding syntect to the `arcterm-structured-output` crate's Cargo.toml.

- **Diff rendering approach**: The spec requires side-by-side layout (FR-005). Terminal output is
  inherently line-oriented. Side-by-side layout requires interleaving left and right columns within
  the terminal width. Whether this can be achieved purely via ANSI text (using character-level
  alignment) or requires a dedicated rendering mode was not fully researched. The `delta` tool
  achieves side-by-side diffs in terminals as pure text output — it should be referenced as the
  implementation model.
