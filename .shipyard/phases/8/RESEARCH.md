# Research: Phase 8 — Config Overlays, Polish, and Release

## Context

Arcterm is a GPU-rendered AI terminal emulator written in Rust. After seven phases it has:
a working wgpu renderer, a multiplexer with vim-style navigation, a TOML config system
(`arcterm-app/src/config.rs`) with hot-reload via `notify`, a command-palette overlay pattern
(`palette.rs` + `renderer.rs OverlayQuad`), structured output (OSC 7770), workspaces, WASM
plugins, and the Phase 7 AI integration layer.

Phase 8 must close out five distinct problem domains before a public release:

1. **Config overlay system** — pending/accepted TOML layers, diff view, accept/reject/edit flow
2. **`arcterm config flatten`** — a new CLI subcommand that collapses the overlay stack into one TOML
3. **Cross-pane search** (`Leader+/`) — regex search across all pane scrollback with match highlighting
4. **Performance optimization** — hit the <5ms latency, <100ms cold start, <50MB memory targets
5. **Release packaging** — cross-compiled binaries for macOS (aarch64 + x86_64), Linux (x86_64), Windows (x86_64)
6. **Documentation** — man page, example configs, plugin guide

Each domain has at least one library choice that needs a concrete recommendation. Research below covers each.

---

## Research Area 1: Config Overlay — TOML Merge and Diff

### Problem

The overlay system must:
- Load a base `config.toml` and N ordered overlay files (all typed as `ArctermConfig`)
- Merge them field-by-field (last overlay wins per key)
- Produce a human-readable diff between the base and a pending overlay to display in the UI
- Serialize the final resolved config back to valid TOML for `config flatten`

### Candidates

| Criteria | `toml` v1 (already used) | `toml_edit` v0.22 | `serde-toml-merge` |
|---|---|---|---|
| Maturity | ~10 years, v1.0 | ~6 years, v0.22 | ~2 years, v0.3 |
| License | MIT / Apache 2.0 | MIT / Apache 2.0 | MIT |
| Format-preserving | No | Yes | No |
| Serde integration | Full | Full | Full |
| Field-level merge | Via serde struct | Via `Document` API | Direct value merge |
| Diff support | None | None built-in | None |
| Already in deps | Yes (`arcterm-app/Cargo.toml`) | No | No |
| TOML serialize back | Yes | Yes (preserves comments) | Yes |

**Diff library candidates:**

| Criteria | `similar` v2 | `diffy` v0.3 | `dissimilar` v1 |
|---|---|---|---|
| License | Apache 2.0 | MIT | MIT / Apache 2.0 |
| Algorithm | Myers + Patience + LCS | Myers + 3-way | Google diff-match-patch |
| API | `TextDiff::from_lines()` | `create_patch()` | `diff()` on char level |
| Unified diff output | Yes (built-in) | Yes | No (semantic chunks only) |
| Maintenance | Active, 1.2k stars, v2.x | Moderate | Active (dtolnay) |
| Already in deps | No | No | No |

### Analysis

The `toml` crate (v1) is already present in `arcterm-app/Cargo.toml`. `ArctermConfig` is fully
`Deserialize`-derived with `#[serde(default)]` on every field. Field-level merging can be
implemented without adding any new crate: deserialize each overlay into `ArctermConfig`, then
apply a hand-written merge function that checks `Option<T>` fields against `None` (treat absent
= "don't override") and overwrites present fields. The struct is shallow (no deeply nested maps),
so this is straightforward.

`toml_edit` would be valuable if format preservation (comments, key ordering) mattered. It does
not matter here: overlay files are AI-generated and the resolved output of `config flatten` is a
clean machine-written file. Adding `toml_edit` would be net overhead.

`serde-toml-merge` merges arbitrary `toml::Value` trees but does not know about the
`ArctermConfig` schema. Using it would require round-tripping through `toml::Value` and then
re-deserializing, adding complexity without benefit over typed struct merging.

For the **diff view** in the `Leader+o` overlay, the diff must be between two TOML strings
(serialize pending overlay to TOML string, serialize base to TOML string, diff line-by-line). The
`similar` crate's `TextDiff::from_lines()` API is the clear fit: it returns a unified diff
directly, has an active maintenance history (1.2k stars, v2.x, Apache 2.0), and the output can
be rendered using the existing `OverlayQuad` + overlay-text pattern already established in
`renderer.rs`. `diffy` is also capable but `similar` has cleaner ergonomics for this use case.

**Recommendation for config overlay:** Use `toml` (already present) for parsing and merging.
Add `similar` for line diff generation. No additional crates required for serialization back to
TOML.

---

## Research Area 2: `arcterm config flatten` CLI Subcommand

### Problem

`arcterm config flatten` must be a new variant in the existing `CliCommand` enum in `main.rs`.
The existing CLI uses `clap = { version = "4", features = ["derive"] }`. The subcommand:
- Reads base config from `~/.config/arcterm/config.toml`
- Reads all accepted overlays from `~/.config/arcterm/overlays/accepted/` in sorted order
- Reads the workspace overlay if one is active (optional flag or auto-detect)
- Writes the fully resolved `ArctermConfig` as TOML to stdout

### Analysis

No new crates are needed. The existing `toml` crate serializes via `toml::to_string_pretty()`.
`clap` 4 derive already handles subcommand dispatch. `dirs` (already present) resolves the
config directory. The existing `ArctermConfig::load()` function in `config.rs` needs a parallel
`load_and_apply_overlays()` function — straightforward because every field has a `Default` and
the struct is `Deserialize`.

The only design question is whether `Serialize` must be derived on `ArctermConfig`. Currently
`config.rs` only derives `Deserialize`. Adding `#[derive(Serialize)]` (serde feature already
enabled) is the only code change required to enable `toml::to_string_pretty()` output.

**Conclusion:** No new crate needed. This is an extension of existing code.

---

## Research Area 3: Cross-Pane Search

### Problem

`Leader+/` must open a search overlay that accepts a regex pattern, searches through all pane
scrollback buffers, and highlights matches. Navigation with `n`/`N` moves between results.
Match highlight quads must be drawn on the correct pane at the correct cell coordinates.

### Candidate: `regex` crate (already in workspace dependencies)

`regex` v1 is already declared at the workspace level in `Cargo.toml` and imported in
`arcterm-app/Cargo.toml`. It is also imported directly in `arcterm-app/src/main.rs`.

**Key properties of `regex` v1.12.3:**
- License: MIT / Apache 2.0
- Guarantee: `O(m * n)` worst-case time complexity (no catastrophic backtracking)
- Key API: `Regex::find_iter(&str)` returns an iterator of `Match` with `.start()` / `.end()`
  byte offsets
- Feature flags in use by default: `perf-literal` (aho-corasick), `perf-dfa` (lazy DFA)
- Does not support lookahead/lookbehind — `fancy-regex` does, at the cost of the no-backtracking guarantee

### Candidate: `fancy-regex`

`fancy-regex` wraps the `regex` crate for simple patterns and falls back to backtracking for
fancy features. Adds ~50 kB to the binary and removes the linear-time safety guarantee. The
primary use case here (user-typed search patterns in a terminal) is simple: literal strings,
anchors, character classes. There is no identified user need for lookahead.

### Comparison Matrix

| Criteria | `regex` v1.12 (already present) | `fancy-regex` v0.14 |
|---|---|---|
| License | MIT / Apache 2.0 | MIT |
| Time complexity | O(m * n) guaranteed | O(m * n) for simple; backtracking for fancy |
| Lookahead / lookbehind | No | Yes |
| Backreferences | No | Yes |
| Already in deps | Yes | No |
| Binary size impact | Zero (already linked) | ~50 kB additional |
| API for byte-offset matches | `find_iter()` → `Match` | Same (wraps regex internally) |
| Catastrophic backtracking risk | None | Present for fancy patterns |

### Integration Notes

The `PaletteState` / `WorkspaceSwitcherState` pattern in `palette.rs` is the direct template
for a `SearchOverlayState`. The state machine would hold:
- `query: String` (regex input)
- `compiled: Option<regex::Regex>` (recompiled on query change)
- `matches: Vec<(PaneId, usize, usize)>` (pane, byte-start, byte-end per match)
- `selected: usize` (current match index)

Match highlighting maps byte offsets back to `(row, col)` in the grid and renders tinted
`OverlayQuad` instances per matched cell range, using the existing `render_multipane` overlay
system. The `OverlayQuad` type (`renderer.rs:47`) already takes `rect: [f32; 4]` and
`color: [f32; 4]` — no renderer changes required.

The scrollback buffer is accessible via `Terminal.grid()`, which holds a `Grid` with a scrollback
region. All pane grids are available in the `AppState.panes` `HashMap`.

**Recommendation:** Use `regex` (already in deps). No new crate needed. `fancy-regex` is not
justified — the added binary size and backtracking risk outweigh the lookahead feature that no
terminal search use case requires.

---

## Research Area 4: Performance Optimization

### Problem

The Phase 8 targets are: key-to-screen latency < 5ms, cold start < 100ms, memory < 50MB.
The question is: how do you measure these programmatically rather than by eye?

### Existing Instrumentation

Arcterm already has a `latency-trace` cargo feature flag (`arcterm-app/Cargo.toml`, line 13)
with meaningful coverage:

- **Cold start**: `TraceInstant::now()` is captured at the top of `main()` (line 484). The first
  frame's `AboutToWait` handler logs `[latency] cold start → first frame: {:?}` via
  `self.cold_start.elapsed()` (line 2224–2227).
- **Frame submission**: `t0 = TraceInstant::now()` before each `render_multipane` call; duration
  logged as `[latency] frame submitted in {:?}` (line 2218–2220).
- **Key → PTY write**: `t0 = TraceInstant::now()` on `KeyboardInput` (line 2238); duration logged
  as `[latency] key → PTY write` (line 2380).
- **PTY output → processed**: `t0 = TraceInstant::now()` on PTY bytes received; duration logged
  (line 1561).
- **FPS**: counted per `AboutToWait` cycle and logged at intervals via `fps_last_log` / `fps_frame_count`.

### What is Missing

The `latency-trace` instrumentation currently measures key→PTY write and PTY→processed, but
does not measure the full round-trip (key down to screen pixels updated). To fully validate the
<5ms target, a "key down → frame presented" timestamp pair is needed: the `KeyboardInput` event
timestamp as `t0` and the `present_frame()` wgpu submission as `t1`.

### Approach: Measurement Protocol

The most reliable programmatic approach for a wgpu application is:

1. Enable `latency-trace` feature: `cargo run --package arcterm-app --features latency-trace`
2. Set `RUST_LOG=debug` to capture all `[latency]` log lines
3. Parse the log output to compute p50/p99 of frame times and key round-trip

For cold start, the existing instrumentation is already correct — the log line `[latency] cold
start → first frame:` appears on the first `AboutToWait` after window creation.

For **memory profiling**, the standard approach on macOS is:
- `heaptrack` or `cargo-heaptrack` for heap profiling
- `/usr/bin/time -l` on macOS to capture peak RSS
- For a more detailed audit: `RUST_LOG=info cargo run` and observe memory via `Activity Monitor`
  or `ps -o rss`

No new runtime library is needed for latency measurement — the existing `std::time::Instant`
infrastructure is sufficient. The optimization pass itself will rely on:
- Lazy initialization of `syntect`'s `SyntaxSet` (load on first structured block, not at startup)
- Deferred plugin loading (load plugins after first frame)
- Switching `wgpu` present mode to `PresentMode::Immediate` during latency measurement (as noted
  in `CONTEXT-8.md`) to remove vsync as a confounder
- Memory: audit for per-pane allocations (scrollback Vec sizing, glyph atlas pre-allocation)

---

## Research Area 5: Release Packaging

### Problem

Need cross-compiled binaries for:
- macOS aarch64 (Apple Silicon native)
- macOS x86_64 (Intel)
- Linux x86_64
- Windows x86_64

Current `.cargo/config.toml` configures `x86_64-apple-darwin` (lld linker) and
`aarch64-apple-darwin` (default linker) for native macOS builds. No cross-compilation is
configured. CI is described in the roadmap but no release workflow exists yet.

### Candidates

| Criteria | `cargo-dist` v0.31 | `cross` (cross-rs) | Manual GitHub Actions matrix |
|---|---|---|---|
| Latest version | 0.31.0 (Feb 23, 2026) | Latest from git | N/A |
| License | MIT | MIT | N/A |
| macOS native build | Yes (macos-15-intel + macos-latest runners) | No — Docker/Linux host only | Yes |
| Linux x86_64 | Yes — native runner | Yes — Docker | Yes — ubuntu runner |
| Windows x86_64 | Yes — native runner | Docker (Windows containers) | Yes — windows runner |
| macOS aarch64 | Yes — native runner (free on GHA) | No | Yes — macos-latest runner |
| macOS → Linux cross | Via cargo-zigbuild (integrated) | Yes (Docker) | Manual with zigbuild/cross |
| macOS code signing | Documented workflow (needs Apple certs) | Not applicable | Manual step |
| Windows installer | Shell + MSI (via WiX) | No | No |
| GitHub CI generation | `cargo dist init` generates full workflow YAML | No | Manual |
| Docker dependency | No | Required (Docker 20.10+) | No |
| Audit trail / checksums | Yes (SHA256 + GitHub Releases) | No | Manual |
| Learning curve | Low (config-driven) | Medium (Docker setup) | Medium (YAML authoring) |

**`cargo-dist` v0.31.0** (released Feb 23, 2026) is the highest-level tool. Key facts:

- Native runners: cargo-dist uses macOS native runners for macOS targets (aarch64 free on GitHub
  Actions), eliminating Docker entirely. For Linux ARM64, GitHub's free native arm64 runners are
  used by default.
- Cross-compilation: cargo-dist integrates cargo-zigbuild for Linux cross-targets and cargo-xwin
  for Windows cross-targets when building from a Linux host.
- Generates complete GitHub Actions YAML via `cargo dist init --ci github`.
- Produces: `.tar.gz` (macOS/Linux), `.zip` (Windows), optionally shell installers and MSI.
- License: MIT.
- `cargo-zigbuild` **only supports Linux and macOS targets**. Windows (`x86_64-pc-windows-msvc`)
  requires cargo-xwin or a native Windows runner — cargo-dist handles this automatically by
  using a native Windows runner.

**`cross` (cross-rs)** is Docker-based and does not run on macOS as the cross-compilation host.
Since the project's primary development platform is macOS (`aarch64-apple-darwin` is in
`.cargo/config.toml` and the context notes "macOS primary polish target"), cross-rs is
unsuitable for local development use and requires Docker in CI. It adds CI complexity with no
advantage over native runners.

**Manual GitHub Actions matrix** is feasible and offers maximum control. The cost is maintaining
the workflow YAML manually. For a project with two macOS targets, one Linux target, and one
Windows target, the manual matrix is three to four jobs — manageable but redundant when
cargo-dist generates equivalent YAML automatically.

**`cargo-zigbuild` standalone** (v0.22.1, Feb 18, 2026) is useful if the project wants to
cross-compile from macOS to Linux on a developer machine. It does **not** support Windows targets.
It is best used as an integration within cargo-dist rather than as the primary packaging tool.

### macOS Code Signing and Notarization

macOS Gatekeeper requires code signing for distribution outside the App Store. The notarization
step (uploading to Apple for malware scanning) adds 30–120 seconds per build. cargo-dist
documents a codesigning workflow using `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
`APPLE_TEAM_ID`, and `APPLE_API_KEY` secrets. The `CONTEXT-8.md` explicitly defers signing for
the initial release. This is a sound decision: unsigned binaries can be distributed via GitHub
Releases with a note for users to bypass Gatekeeper (`xattr -dr com.apple.quarantine`).

**Recommendation:** Use `cargo-dist`. It generates the full GitHub Actions release workflow,
handles native runners for all four targets, integrates cargo-zigbuild and cargo-xwin for any
cross-compilation scenarios, and produces checksums automatically. Code signing can be added
later without changing the architecture.

---

## Research Area 6: Man Page Generation

### Problem

A man page for `arcterm` must be generated and bundled with the release. The CLI is defined with
`clap = { version = "4", features = ["derive"] }` and already has subcommands
(`Open`, `Save`, `List`, `Plugin`) defined in `main.rs`.

### Candidate: `clap_mangen` v0.2.33

`clap_mangen` is the official clap-ecosystem crate for man page generation. It generates ROFF
format from a `clap::Command` definition.

| Property | Value |
|---|---|
| Latest version | 0.2.33 |
| License | MIT / Apache 2.0 |
| clap requirement | `^4.0.0` (matches project's clap 4) |
| Build dependency | Yes — added as `[build-dependencies]` only |
| Output format | ROFF (standard man page format) |
| API surface | `Man::new(cmd).render(&mut writer)` |
| Subcommand support | `generate_to()` generates pages for all subcommands |
| Maintenance | Official clap-rs ecosystem, same maintainers as clap |

**Alternatives:**

- **Hand-written ROFF**: No maintenance advantage; requires ROFF knowledge; falls out of sync
  with `--help` text.
- **mdbook + pandoc**: Suitable for rich documentation sites, not for man pages shipped with
  binaries.
- **`man` crate**: Unmaintained as of 2024; requires manual ROFF construction.

**Integration pattern:** Add `clap_mangen` to `[build-dependencies]` in `arcterm-app/Cargo.toml`.
Write a `build.rs` that instantiates the same `Cli` struct (via `clap::CommandFactory` from the
`derive` feature — already enabled) and calls `Man::new(cmd).render()` to write
`target/man/arcterm.1`. The CI release workflow copies this file into the release archive.

No changes to `src/main.rs` are required. The clap derive `CommandFactory` trait is already
available because `features = ["derive"]` is set.

**Recommendation:** Use `clap_mangen`. It is the canonical tool, requires zero ROFF knowledge,
stays in sync with `--help` automatically, and is a build-only dependency (zero runtime overhead).

---

## Comparison Matrix Summary

| Domain | Recommendation | Alternative Considered | Reason Alternative Rejected |
|---|---|---|---|
| Config merge | `toml` (existing) + typed struct merge | `toml_edit`, `serde-toml-merge` | No format-preservation needed; existing crate sufficient |
| Config diff display | `similar` (new dep) | `diffy`, `dissimilar` | `similar` has unified diff output API + active maintenance |
| Cross-pane search | `regex` (existing workspace dep) | `fancy-regex` | No lookahead needed; existing dep; linear-time guarantee |
| Latency measurement | `std::time::Instant` + existing `latency-trace` feature | `latency_trace` crate (external) | Instrumentation already in place; no new dep needed |
| Release packaging | `cargo-dist` v0.31 | `cross`, manual matrix | Native runners, no Docker, generates CI YAML automatically |
| Man page | `clap_mangen` v0.2.33 | Hand-written ROFF | Stays in sync with clap derive; zero ROFF knowledge needed |

---

## Detailed Analysis

### Config Overlay System (codebase integration)

`ArctermConfig` in `config.rs` is a flat struct with `#[serde(default)]` on the top-level and
each nested config struct (`ColorOverrides`, `MultiplexerConfig`, `KeybindingConfig`). Every
field has either a primitive type or `Option<T>`.

The merge semantics required: "overlay field wins over base field if it is non-default." The
challenge is that TOML's absence and the Rust default are indistinguishable after deserialization
— a user who explicitly sets `font_size = 14.0` in an overlay cannot be distinguished from an
overlay that simply omits `font_size`.

Two approaches:

**A. Semantic merge via Option-wrapping:** Change the overlay struct to use `Option<T>` for every
field, so absence is `None`. This is a new type (e.g., `ArctermConfigOverlay`) that is
`Deserialize` but not necessarily `Serialize`. Apply overlay: for each field, if overlay field is
`Some(v)`, set base field to `v`.

**B. Value-level merge via `toml::Value`:** Deserialize both base and overlay to `toml::Value`,
merge recursively (overlay values win), then deserialize the merged `Value` to `ArctermConfig`.
This handles absence naturally because TOML keys absent from the overlay file are simply absent
from the `Value` map.

Approach B is simpler to implement and does not require a new struct type. The existing `toml`
crate's `Value` type supports recursive merge via `HashMap` operations. This is the recommended
implementation path.

The overlay diff for the UI (`Leader+o`):
1. Serialize pending overlay `toml::Value` to a TOML string via `toml::to_string_pretty()`
2. Serialize the current resolved config to TOML string
3. Pass both to `similar::TextDiff::from_lines()` to generate a unified diff
4. Render the diff lines as `PaletteText` with color-coded `OverlayQuad` backgrounds
   (green for additions, red for deletions) using `render_multipane`'s `overlay_quads` and
   `overlay_text` parameters

The accept/reject/edit key actions (`a`, `x`, `e`) follow the same `PaletteEvent` dispatch
pattern as `PaletteState::handle_key()`.

### `arcterm config flatten` (codebase integration)

A new `CliCommand::Config` variant with subcommand `Flatten` must be added to the `CliCommand`
enum in `main.rs`. This is handled before the GUI starts (the pattern for `List`, `Open`, `Save`
is already established). The flatten logic:

1. Call `ArctermConfig::config_path()` to find base
2. List `~/.config/arcterm/overlays/accepted/` with `std::fs::read_dir`, sort by filename
3. Apply each overlay via `toml::Value` merge
4. Deserialize merged `Value` to `ArctermConfig`
5. `ArctermConfig` must derive `Serialize` (add `#[derive(Serialize)]` to the struct)
6. `toml::to_string_pretty(&resolved_cfg)` to stdout

No wgpu or winit initialization is touched.

### Cross-Pane Search (codebase integration)

The `regex::Regex` type is already imported in `main.rs` (confirmed: `regex.workspace = true` in
`arcterm-app/Cargo.toml`). The existing `PaletteState` in `palette.rs` is the exact template.

A `SearchOverlayState` struct in `palette.rs` would follow the same `query: String` / filtered
results / `handle_key()` / `render_quads()` / `render_text_content()` pattern. Additional state:
- `compiled_regex: Option<regex::Regex>` (recompiled on each `query` change)
- `matches: Vec<SearchMatch>` where `SearchMatch = { pane_id: PaneId, row: usize, col_start: usize, col_end: usize }`
- `current_match: usize`

Search execution iterates `AppState.panes`, for each pane iterates `terminal.grid().rows()` (both
visible and scrollback), converts each row's cells to a `String`, calls `compiled_regex.find_iter(&row_str)`,
and maps byte offsets back to column indices.

Match highlighting renders one `OverlayQuad` per match cell range at the correct physical pixel
position within each pane's `rect`. The `render_multipane` call already accepts `overlay_quads:
&[OverlayQuad]` — this is the insertion point.

The input overlay (regex query field) follows the same `PaletteQuad` + `PaletteText` rendering
pattern, positioned at the bottom of the screen (distinct from the top-positioned command palette).

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Config overlay merge produces surprising results when overlay omits a field that happens to equal the default | Medium | Low | Use `toml::Value` merge (key-absent = "don't override"); document in user-facing config guide |
| `similar` diff of two TOML strings produces cosmetically poor output (key ordering differences) | Low | Low | Serialize both base and overlay with `toml::to_string_pretty()` which sorts keys consistently |
| `cargo-dist init` generates a workflow that fails for `x86_64-apple-darwin` cross-compile from Linux | Medium | Medium | Use native macOS runners for both macOS targets; cargo-dist defaults to this as of v0.26+ |
| macOS unsigned binaries trigger Gatekeeper warnings | High (expected) | Medium | Document `xattr -dr com.apple.quarantine` in README; defer signing to post-launch |
| Windows binary missing MSVC runtime DLLs | Medium | High | Compile with `x86_64-pc-windows-msvc` on native Windows runner; cargo-dist's Windows runner ships correct runtime; test on clean VM |
| Cross-pane search across large scrollback buffers (10,000 lines × N panes) takes >16ms | Medium | Medium | Run search on a background tokio task; send results via channel back to event loop; never block the render thread |
| `latency-trace` feature reports key→PTY write but misses PTY read→frame presented gap | High | Medium | Add a second `TraceInstant` at the start of the PTY `got_data` branch and a third at `gpu.present_frame()` to close the measurement gap |
| Cold start >100ms due to synchronous `SyntaxSet::load_defaults()` in syntect | High | High | Move syntect init to a `tokio::spawn_blocking` task; serve structured blocks from a lazy `OnceLock<HighlightEngine>` |
| `clap_mangen` generates a man page that omits subcommand descriptions | Low | Low | Ensure every `CliCommand` variant has `#[command(about = "...")]` annotations before calling `generate_to()` |
| `regex::Regex::new()` called on every character typed in search overlay (expensive) | Medium | Medium | Compile only on `Enter` or after a 200ms debounce; show a compile-error message inline if the regex is invalid |

---

## Implementation Considerations

### Integration Points with Existing Code

- `config.rs`: Needs `#[derive(Serialize)]` on `ArctermConfig`, `ColorOverrides`,
  `MultiplexerConfig`, `KeybindingConfig`. Currently only `Deserialize` is derived.
- `main.rs` `CliCommand` enum: Add `Config { subcommand: ConfigSubcommand }` variant before
  the GUI starts.
- `main.rs` `AppState`: Add `search_overlay: Option<SearchOverlayState>` to hold search mode.
- `palette.rs`: Add `SearchOverlayState` struct following the `PaletteState` template.
- `renderer.rs`: No changes — `render_multipane` already accepts `overlay_quads` and
  `overlay_text`. Match highlight quads slot directly into `overlay_quads`.
- `arcterm-app/Cargo.toml`: Add `similar` as a dependency; add `clap_mangen` as a build-dependency.
- New file: `build.rs` in `arcterm-app/` for man page generation.
- New file: `.github/workflows/release.yml` generated by `cargo dist init --ci github`.

### Migration Path (no existing solution to replace)

No existing overlay system, search, or release pipeline exists. All additions are net-new code.
The diff view (`Leader+o`) reuses the `PaletteState` rendering pattern directly — no refactor
required.

### Testing Strategy

- Config merge: Unit tests in `config.rs` for each overlay scenario (field present in overlay,
  field absent, conflicting values). Follow existing test conventions in the same file.
- `config flatten`: Integration test that writes a base + two overlay files to a temp directory,
  runs the flatten command, and asserts the stdout is valid TOML with correct resolved values.
- Cross-pane search: Unit tests in `palette.rs` (or a new `search.rs`) for `SearchOverlayState`
  — query compilation, match collection from a mock grid, `n`/`N` navigation, Escape close.
- Latency: Run `cargo run --features latency-trace` with `RUST_LOG=debug`, parse `[latency]`
  lines, assert p99 key→frame < 5ms on developer hardware.
- Release: `cargo dist build --artifacts=local` on macOS to verify binary output before pushing.

### Performance Implications

- **Config flatten** is a startup-time code path that exits immediately; performance is not
  a concern.
- **Cross-pane search** must not block the event loop. See risk mitigation above regarding
  background task for large scrollback.
- **Config overlay loading** on startup adds at most a directory scan and N TOML parses; with
  typical N < 10 and small overlay files, this will be sub-millisecond. No concern.
- **`similar` diff** on two TOML strings of < 200 lines takes < 100µs. No concern.

---

## Sources

1. [clap_mangen docs.rs — v0.2.33](https://docs.rs/clap_mangen/latest/clap_mangen/)
2. [clap_mangen — crates.io](https://crates.io/crates/clap_mangen)
3. [cargo-dist releases — GitHub](https://github.com/axodotdev/cargo-dist/releases)
4. [cargo-dist CHANGELOG — GitHub](https://github.com/axodotdev/cargo-dist/blob/main/CHANGELOG.md)
5. [cargo-zigbuild — GitHub](https://github.com/rust-cross/cargo-zigbuild)
6. [cross (cross-rs) — GitHub](https://github.com/cross-rs/cross)
7. [regex crate — docs.rs](https://docs.rs/regex/latest/regex/)
8. [regex crate — crates.io](https://crates.io/crates/regex)
9. [similar — GitHub](https://github.com/mitsuhiko/similar)
10. [similar — docs.rs](https://docs.rs/similar)
11. [diffy — docs.rs](https://docs.rs/diffy/0.2.1/diffy/)
12. [toml_edit — crates.io](https://crates.io/crates/toml_edit)
13. [serde-toml-merge — crates.io](https://crates.io/crates/serde-toml-merge)
14. [Terminal latency — Dan Luu](https://danluu.com/term-latency/)
15. [cross-compilation support issue #74 — cargo-dist](https://github.com/axodotdev/cargo-dist/issues/74)
16. [Rust Cross-Compilation With GitHub Actions — reemus.dev](https://reemus.dev/tldr/rust-cross-compilation-github-actions)
17. [Zig Makes Rust Cross-Compilation Just Work — actually.fyi](https://actually.fyi/posts/zig-makes-rust-cross-compilation-just-work/)
18. [fancy-regex — GitHub](https://github.com/fancy-regex/fancy-regex)
19. [GitHub Actions arm64 runners GA announcement — GitHub Changelog](https://github.blog/changelog/2024-09-03-github-actions-arm64-linux-and-windows-runners-are-now-generally-available/)

---

## Uncertainty Flags

- **cargo-dist macOS code signing workflow**: Research confirms signing is supported via Apple
  credentials stored as GitHub Secrets, but the exact `dist.toml` configuration keys and
  required entitlements for a terminal emulator (PTY access, accessibility) are not confirmed.
  Further investigation needed when signing is added post-launch.

- **wgpu `PresentMode::Immediate` on macOS**: Setting immediate present mode for latency
  measurement may not be honored on all macOS/Metal versions. If the driver ignores
  `Immediate`, latency traces will include vsync intervals (8.3ms at 120Hz) and
  measurements will be misleading. This needs to be tested empirically on the development
  machine.

- **`toml::to_string_pretty()` key ordering stability**: The `toml` v1 crate's serializer
  output key ordering is not formally guaranteed to be stable across versions. If key ordering
  changes between runs, the `similar` diff will show spurious changes. This needs to be
  verified empirically, and if unstable, a sort-keys pass should be applied before diffing.

- **scrollback buffer API**: The research assumes `terminal.grid().rows()` provides access to
  both scrollback and visible rows as a contiguous iterator. The exact API of the `Grid` type
  in `arcterm-core` was not read in full. This must be confirmed before implementing the
  cross-pane search scan loop.

- **`cargo-dist` Windows MSVC binary portability**: cargo-dist's Windows runner produces
  `x86_64-pc-windows-msvc` binaries. These may require the Visual C++ Redistributable on end-user
  machines. The alternative (`x86_64-pc-windows-gnu`) produces fully static binaries but
  requires the MinGW toolchain. The correct default for a terminal emulator targeting Windows
  users has not been determined.
