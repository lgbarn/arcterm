# Research: Phase 5 — Workspaces and Session Persistence

## Context

Arcterm is a GPU-rendered terminal emulator (wgpu + winit + tokio) that reached a full
multiplexer in Phase 3 and a structured-output rendering layer in Phase 4.  Phase 5 adds
project-aware workspaces: a TOML schema for describing layouts, session auto-save/restore,
a fuzzy workspace switcher bound to `Leader+w`, and the `arcterm open/save/list` CLI
subcommands.

### Current stack relevant to Phase 5

| Layer | Existing crate | Notes |
|---|---|---|
| TOML config (read-only) | `toml = "1"` + `serde = { features = ["derive"] }` | Already in `arcterm-app/Cargo.toml`; used by `config.rs` |
| Config path | `dirs = "6"` | `dirs::config_dir()` → `~/.config/arcterm/` |
| CLI entry | `fn main()` in `main.rs` — raw `EventLoop::new()` with zero arg-parsing | No clap, no `std::env::args` inspection today |
| Fuzzy UI pattern | `PaletteState` in `palette.rs` | Query string + filtered-index list + arrow/enter/escape key handling; rendered as wgpu quads + text |
| Pane tree | `PaneNode` (enum: `Leaf`, `HSplit`, `VSplit`) in `layout.rs` | Stores `PaneId(u64)` + `f32` ratio; no `Serialize` today |
| Tab model | `TabManager` / `Tab` in `tab.rs` | Owns a `PaneNode`, focus `PaneId`, `zoomed: Option<PaneId>` |
| Terminal spawn | `Terminal::new(size, shell: Option<String>)` in `terminal.rs` | Takes optional shell override; CWD not explicitly threaded in |
| PTY | `arcterm-pty` crate (`PtySession::new`) | Spawns shell process via `portable-pty` |

The `config.rs` pattern (derive `Deserialize` on a struct, call `toml::from_str`) is the
established convention for all TOML I/O in this codebase.

---

## Comparison Matrix

### Topic 1: CLI subcommand parsing

| Criteria | clap (derive) | clap (builder) | pico-args | std::env::args |
|---|---|---|---|---|
| Version (Mar 2026) | 4.5.60 | 4.5.60 | 0.5.0 (Jun 2022) | std — n/a |
| Monthly downloads | ~42M | same crate | ~3M | n/a |
| GitHub stars | 16.3k | 16.3k | ~700 | n/a |
| License | MIT/Apache-2.0 | MIT/Apache-2.0 | MIT | n/a |
| Help generation | Automatic | Automatic | None | None |
| Subcommand ergonomics | Enum derive — single source of truth | Builder API — verbose | Manual match on strings | Manual match on strings |
| Compile-time cost | ~11s added (syn "full" parse) | Lower than derive | Minimal | Zero |
| Binary size | Medium (+~100–200 KB stripped) | Same | Minimal | Zero |
| Already in Cargo.toml | No | No | No | Yes (std) |
| Stack fit | High — matches serde/derive style already used | Medium | Low | Low |

### Topic 2: TOML serialization for workspace/session files

| Criteria | toml crate (already present) | toml_edit | serde_json |
|---|---|---|---|
| Version (Mar 2026) | 1.0.6 | 0.25.4 | 1.x |
| Monthly downloads | ~37M | ~35M | ~60M |
| License | MIT/Apache-2.0 | MIT/Apache-2.0 | MIT/Apache-2.0 |
| Serialize support | Yes (`toml::to_string`) | Yes (via `serde` feature) | Yes |
| Deserialize support | Yes | Yes | Yes |
| Format preservation | No (re-serializes clean) | Yes — preserves comments/order | N/A |
| Human-editable output | Yes — clean TOML | Yes — preserves user edits | No — JSON is less readable |
| Already in Cargo.toml | Yes | No | Yes (workspace dep) |
| Stack fit | Exact match — same as config.rs pattern | Overkill for new files | Wrong format for workspace |

### Topic 3: Fuzzy matching algorithm for workspace switcher

| Criteria | nucleo | fuzzy-matcher | Hand-rolled substring |
|---|---|---|---|
| Version (Mar 2026) | 0.5.0 (Feb 2026) | 0.3.7 (Oct 2020) | n/a |
| Monthly downloads | ~42k | ~1.7M | n/a |
| GitHub stars | 1.3k | ~400 | n/a |
| License | MPL-2.0 | MIT | n/a |
| Algorithm | Smith-Waterman + Unicode presegmentation | Smith-Waterman (skim) or clangd | `str::contains` |
| fzf score parity | Yes — intentional compatibility | Partial | No |
| Unicode correctness | Yes — multi-codepoint graphemes | ASCII-only bonus system | Basic |
| Active maintenance | Yes — helix-editor actively uses it | Stalled since 2020 | n/a |
| Async/parallel scoring | Yes — designed for TUI background updates | No | n/a |
| New dependency required | Yes | Yes | No |
| Stack fit | Medium — MPL-2.0 requires source disclosure for modifications; adds dependency | Low — stale | High — zero deps, but lower quality |

---

## Detailed Analysis

### Topic 1: CLI Subcommand Parsing

#### Option A: clap with derive macro

**Strengths:** The derive API (`#[derive(Parser)]` on a struct, `#[derive(Subcommand)]` on an enum)
is the idiomatic Rust approach and already mirrors how `serde` derive is used throughout
the codebase.  Help generation is free.  Shell completions are available via `clap_complete`
if needed in Phase 8.  At 16.3k GitHub stars and ~42M monthly downloads (lib.rs, Mar 2026),
community support is unambiguous.  Latest release (v4.5.60, Feb 19, 2026) shows active
maintenance.  Used by ~473k projects.

**Weaknesses:** The `clap_derive` proc-macro forces `syn` into "full" parse mode, adding
roughly 1 second to syn's own compile time and approximately 11 additional seconds to a
clean build overall.  Binary size increases by ~100–200 KB stripped.  For a binary that is
already pulling in `wgpu`, `winit`, `swash`, and `tokio`, these additions are not materially
significant.

**Integration notes:** `main()` currently goes directly to `EventLoop::new()` with zero arg
inspection.  The integration point is straightforward: parse args at the very top of `main()`
using `clap::Parser::parse()`, which runs before the tokio runtime and winit loop are
created.  For non-GUI subcommands (`list`, `save` when invoked outside a running session),
the function returns after printing results, never touching winit.  For `open <workspace>`,
the parsed workspace path is passed into the `App` struct before `event_loop.run_app()`.
The pattern is: parse → branch → either GUI path or pure-CLI path.

#### Option B: clap builder API

**Strengths:** Avoids the syn/proc-macro compile cost by not using derive.  Produces the
same runtime behavior and help output.

**Weaknesses:** Verbose — each argument is manually constructed with `Arg::new().long().short()`.
Inconsistent with the codebase's heavy use of `#[derive(Deserialize)]` and `#[derive(Debug, Clone)]`.
The compile-time saving is a few seconds on cold builds; for an interactive terminal project
already spending significant compile time on wgpu, this is not a meaningful win.

#### Option C: pico-args

**Strengths:** Tiny compile footprint, MIT license.

**Weaknesses:** Last release was June 2022 (stalled).  No help generation.  Documented ordering
bugs with multi-arg parsing.  Three subcommands (`open`, `save`, `list`) plus future Phase 8
subcommands (`config flatten`, `plugin install`) make manual parsing increasingly fragile.
Not recommended.

#### Option D: std::env::args

**Strengths:** Zero dependencies.

**Weaknesses:** No help, no shell completion foundation, no subcommand routing.  Any mistake
in manual arg parsing silently continues.  Phase 8 adds more subcommands (`plugin install`,
`config flatten`) — manual parsing compounds maintenance debt.  Rejected.

---

### Topic 2: TOML Serialization for Workspace and Session Files

#### Option A: toml crate (already present in Cargo.toml)

**Strengths:** Already declared as `toml = "1"` in `arcterm-app/Cargo.toml`.  The existing
`config.rs` establishes the exact pattern to reuse: derive `Deserialize` (and now also
`Serialize`) on structs, call `toml::from_str` / `toml::to_string`.  The workspace file
format in `CONTEXT-5.md` is already expressed as TOML, which directly maps to this
approach.  Version 1.0.6 released March 6, 2026 — actively maintained by the toml-rs org
(Eric Huss, Ed Page).  ~37M monthly downloads.

**Weaknesses:** `toml::to_string` does not preserve user-added comments or key ordering
when round-tripping.  For session auto-save files this is not a concern.  For workspace files
that users manually edit, preserving their comments would require `toml_edit`.  Given the
Phase 5 scope — write-once-then-restore for auto-save, and write-new for `arcterm save` —
comment preservation is not required.

**Integration notes:** `PaneNode` is currently `#[derive(Clone, Debug)]`.  To serialize it,
add `Serialize, Deserialize` to that derive line.  `PaneId(u64)` is a newtype around `u64`
— also straightforward to derive.  `Axis` and `Tab` are enums/structs — same.  The key
design decision is whether to serialize the live `PaneId` values (which are runtime-assigned
via `AtomicU64`) or to emit a stable tree structure that reassigns fresh IDs on restore.
The latter is safer: serialize the tree shape and per-pane metadata, not the numeric IDs.

#### Option B: toml_edit

**Strengths:** Preserves comments and key ordering when round-tripping user-edited files.

**Weaknesses:** Different API — does not use `serde` derive as directly; requires a
document-oriented edit model.  Adds a new dependency.  Overkill for Phase 5: auto-save
files are machine-written and the `arcterm save` command writes fresh files from scratch.
If Phase 8 adds in-place config editing, `toml_edit` becomes relevant then.  Deferred.

#### Option C: serde_json

**Strengths:** Already present as a workspace dep (`serde_json = "1"`).

**Weaknesses:** JSON is not human-readable for nested terminal layouts; it is not
git-committable in any practical sense.  The Phase 5 requirement explicitly states
"human-readable, git-committable."  Rejected on format grounds.

---

### Topic 3: Fuzzy Matching for Workspace Switcher

#### Option A: nucleo

**Strengths:** Backed by the Helix editor team; actively used in production.  Correct
Unicode handling (multi-codepoint graphemes).  fzf-compatible scoring.  Designed specifically
for TUI applications.  v0.5.0 released Feb 24, 2026 — actively maintained.

**Weaknesses:** MPL-2.0 license requires that modifications to nucleo itself be open-sourced
(copyleft on the library, not on the linking application under MPL-2.0 Section 3.3).  For
an MIT-licensed application like arcterm that does not modify nucleo's source, this is
acceptable — but worth flagging.  Only ~42k monthly downloads, meaning a smaller community
than clap/serde.  Adds a new dependency.

**Integration notes:** The existing `PaletteState` in `palette.rs` already implements the
query-string + filtered-index list + arrow-key navigation pattern.  A `WorkspaceSwitcherState`
can be structured identically, with the scoring step delegated to `nucleo::Matcher` instead
of `str::contains`.  The rendered overlay reuses the same `PaletteQuad`/`PaletteText`
infrastructure.

#### Option B: fuzzy-matcher

**Strengths:** MIT license.  ~1.7M monthly downloads, more widely adopted today.

**Weaknesses:** Last release October 2020 — over five years stale.  ASCII-only bonus system.
No active maintenance.  Rejected on staleness grounds.

#### Option C: Hand-rolled `str::contains` (substring match)

**Strengths:** Zero new dependency.  Matches the current `PaletteState.update_filter()`
pattern exactly (it uses `str::contains` today).

**Weaknesses:** Not fuzzy — requires exact substring.  Poor user experience when workspace
names share prefixes (e.g., `my-project-api` vs `my-project-web`).  Acceptable for the
command palette (10 fixed commands), but inadequate for an unbounded workspace list.

**However**, this is worth reconsidering: the workspace switcher is expected to list files
from `~/.config/arcterm/workspaces/`.  For a typical user with 5–20 workspaces, the quality
difference between `str::contains` and Smith-Waterman scoring is marginal.  The real
question is whether the dependency cost of nucleo is justified.  See Recommendation below.

---

### Topic 4: Per-Pane CWD Capture for Serialization

Getting the current working directory of a child PTY process (not the arcterm process itself)
requires platform-specific work:

- **Linux**: Read the `/proc/<pid>/cwd` symlink — reliable and zero-dependency.
- **macOS**: Use `proc_pidinfo` via the `libproc` crate or a direct `libc` syscall to
  `proc_pidinfo(pid, PROC_PIDVNODEPATHINFO, ...)`.  The `libproc` crate supports both macOS
  and Linux.
- **Windows**: `NtQueryInformationProcess` or `GetModuleFileNameExW` — significantly more
  complex.

The `arcterm-pty` crate already uses `portable-pty` which wraps `libc`.  The PID of the
child process is accessible from `PtySession`.  Phase 5 targets macOS primarily (based on
project history), so using `libc` directly via `proc_pidinfo` on macOS and `/proc/pid/cwd`
on Linux is the lowest-friction approach without adding a new crate.

---

### Topic 5: Session Auto-Save File Location and Format

The `CONTEXT-5.md` decision already specifies:
- Workspaces: `~/.config/arcterm/workspaces/<name>.toml`
- Auto-save: implied same directory with a reserved name (e.g., `_last_session.toml`)

The `dirs::config_dir()` function (already used in `config.rs`) gives
`~/.config/arcterm/` on macOS/Linux via XDG.  The workspace directory is
`dirs::config_dir().join("arcterm").join("workspaces")`.

Zellij's approach (KDL format, 1-second serialization cadence, cache directory, layout
resurrection) is instructive: they found that serializing every second during normal use
causes measurable I/O.  A better strategy for arcterm is to serialize only on exit (via a
Drop handler or a `WindowEvent::CloseRequested` hook in `window_event`) rather than on a
timer.  This matches the Phase 5 scope as stated in `CONTEXT-5.md`.

---

## Recommendation

### CLI Subcommand Parsing: clap with derive macro

**Selected: `clap = { version = "4", features = ["derive"] }`**

Justification: The derive pattern is consistent with the codebase's use of
`#[derive(Deserialize)]` throughout.  Auto-generated help is table-stakes for a terminal
emulator shipping a public CLI.  The compile-time cost is real but not significant relative
to the existing dependency graph (wgpu, winit, tokio).  pico-args and raw `std::env::args`
were rejected because Phase 5 introduces three subcommands with arguments and Phase 8 adds
more — manual parsing compounds into a maintenance liability.  The builder API was rejected
because it conflicts with the derive-first style of the codebase without meaningful benefit.

### TOML serialization: existing `toml` crate (no new dependency)

**Selected: existing `toml = "1"` + extend PaneNode/Tab/TabManager with `#[derive(Serialize, Deserialize)]`**

Justification: `toml` is already in `Cargo.toml`.  `config.rs` established the `toml::from_str`
/ `toml::to_string` pattern.  Workspace files are written fresh by `arcterm save` and by
the auto-save path — format preservation is not needed.  `toml_edit` and `serde_json` were
rejected: the former adds complexity for a problem that does not yet exist; the latter
produces machine-readable output that violates the human-editability requirement.

### Fuzzy matching: hand-rolled `str::contains` for Phase 5, nucleo deferred to Phase 6/7

**Selected for Phase 5: extend the existing `PaletteState` pattern with `str::contains`
(same algorithm it already uses)**

Justification: The workspace switcher lists `~/.config/arcterm/workspaces/*.toml` files.  A
typical user has 5–30 workspaces.  At that scale, substring filtering is adequate and the
user experience difference vs. Smith-Waterman is negligible.  Adding nucleo introduces a
new MPL-2.0 dependency and ~42k-download-per-month crate for marginal UX gain.  If the
workspace list grows significantly (Phase 7 considers project-indexed workspaces), nucleo
can be integrated then.  fuzzy-matcher was rejected outright due to five years of stalled
maintenance.

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `PaneNode` serialization schema changes break saved sessions | Med | Med | Version the workspace TOML with a `schema_version = 1` top-level field; on mismatch, log a warning and fall back to single-pane default |
| CWD capture via proc/pidinfo fails on some macOS versions | Low | Low | Wrap in `Option<PathBuf>`; if CWD cannot be read, restore pane with workspace `directory` as fallback |
| Session file written to disk mid-crash leaves partial/invalid TOML | Low | Med | Write to a `.tmp` file, then `std::fs::rename` atomically; rename is atomic on POSIX |
| clap compile time slows CI on cold builds | Med | Low | clap is a build-time one-time cost; CI caches `target/`; the derive feature is the only additional cost over the builder API |
| Workspace restore spawns panes before window dimensions are known | Med | Med | Defer pane spawning to `ApplicationHandler::resumed` where `window.inner_size()` is valid, same as the current first-pane initialization path |
| `toml::to_string` serialization of `PaneNode` enum produces valid but unexpected TOML shape | Low | Low | Write a round-trip unit test (`serialize → deserialize → assert_eq`) before shipping; add it to the test suite alongside the existing `layout.rs` tests |
| `WorkspaceSwitcherState` diverges from `PaletteState` in behavior | Low | Low | Extract a shared `OverlayState<Item>` generic or simply duplicate the struct and keep both co-located in `palette.rs` with a comment |

---

## Implementation Considerations

### Integration points with existing code

1. **`main()` in `main.rs`**: The only change at the entry point is calling
   `clap::Parser::parse()` before `EventLoop::new()`.  Non-GUI subcommands (`list`) return
   immediately; `open <workspace>` passes a parsed path into the `App` struct.

2. **`PaneNode`, `Tab`, `TabManager` in `layout.rs` and `tab.rs`**: Add
   `#[derive(Serialize, Deserialize)]` to these types.  `PaneId(u64)` needs a stable
   serialized form — the recommendation is to serialize the tree shape independently of
   live IDs and to reconstruct fresh `PaneId::next()` values on restore.  This requires
   a separate `WorkspacePaneNode` DTT (data-transfer type) or a custom `Serialize` impl
   that omits the ID.

3. **`AppState` in `main.rs`**: Needs a `serialize_session() -> WorkspaceFile` method and a
   corresponding `restore_from(WorkspaceFile)` path.  The restore path mirrors the existing
   `resumed()` handler in `App` which already creates the first terminal via
   `Terminal::new()`.

4. **`palette.rs`**: `WorkspaceSwitcherState` mirrors `PaletteState` with `Vec<WorkspaceEntry>`
   items (name + path) instead of `Vec<PaletteCommand>`.  The rendering quads and text
   helpers are unchanged.  Add a `WorkspaceSwitcherState` struct alongside `PaletteState`;
   do not generalize the existing `PaletteState` to avoid risk to Phase 3/4 behavior.

5. **`config.rs`**: The `ArctermConfig::config_path()` pattern (using `dirs::config_dir()`)
   should be extracted into a shared `arcterm_config_dir() -> PathBuf` helper so both
   config loading and workspace file discovery use the same path logic.

### Migration path

There is no existing session format to migrate from.  Phase 5 writes the first sessions.
Schema version 1 is the baseline.

### Testing strategy

- Unit tests for TOML round-trip: create a `WorkspaceFile` in memory, serialize to string
  with `toml::to_string`, deserialize back, assert equality.  Place alongside `config.rs`
  tests.
- Unit tests for `WorkspaceSwitcherState`: same structure as the existing `PaletteState`
  tests in `palette.rs` — type characters, assert filtered list, press Enter/Escape.
- Integration test for `arcterm list`: invoke the binary with `--list` / `list` subcommand
  in a temp directory, verify output format.  (No GUI required.)
- Manual test: `arcterm save my-project` → verify file at
  `~/.config/arcterm/workspaces/my-project.toml` with expected TOML shape; reopen arcterm,
  verify layout restored.

### Performance implications

- TOML serialization of a 4-pane layout: negligible (microseconds).
- Workspace file I/O on exit: single synchronous write, acceptable.
- Workspace file discovery (`read_dir` on `~/.config/arcterm/workspaces/`): synchronous,
  acceptable for `Leader+w` activation (not on the hot render path).
- `str::contains` over 30 workspace names per keystroke: zero measurable cost.

---

## Sources

1. lib.rs crate page for clap: https://lib.rs/crates/clap
2. GitHub repository for clap: https://github.com/clap-rs/clap
3. clap compile time issue thread: https://github.com/clap-rs/clap/issues/2037
4. clap binary size issue thread: https://github.com/clap-rs/clap/issues/1365
5. clap docs.rs overview: https://docs.rs/clap/latest/clap/
6. lib.rs crate page for toml: https://lib.rs/crates/toml
7. lib.rs crate page for toml_edit: https://lib.rs/crates/toml_edit
8. GitHub repository for toml-rs: https://github.com/toml-rs/toml
9. lib.rs crate page for nucleo: https://lib.rs/crates/nucleo
10. GitHub repository for nucleo: https://github.com/helix-editor/nucleo
11. lib.rs crate page for fuzzy-matcher: https://lib.rs/crates/fuzzy-matcher
12. Zellij session resurrection documentation: https://zellij.dev/documentation/session-resurrection.html
13. pico-args lib.rs: https://lib.rs/crates/pico-args
14. proc_pidinfo crate (macOS CWD): https://crates.io/crates/proc_pidinfo
15. libproc-rs cross-platform: https://github.com/andrewdavidmackenzie/libproc-rs
16. sysinfo Process struct (alternative): https://docs.rs/sysinfo/latest/sysinfo/struct.Process.html

---

## Uncertainty Flags

- **nucleo MPL-2.0 licensing interaction with MIT arcterm**: MPL-2.0 Section 3.3 ("Larger
  Works") permits linking from non-copyleft code without infecting the larger work, so
  arcterm's MIT license is not at risk.  However, this should be confirmed with a legal
  read before shipping a public binary.  Source: https://www.mozilla.org/en-US/MPL/2.0/FAQ/

- **Per-pane CWD on macOS**: The `proc_pidinfo` syscall works via `libproc.h` on macOS
  10.13+.  Its availability on macOS 15 (Sequoia) was not verified in this research.  The
  fallback (use workspace-level `directory` field) is safe.

- **Atomic rename on all target filesystems**: `std::fs::rename` is POSIX-atomic on ext4
  and APFS but not guaranteed atomic across different filesystems (e.g., if `$TMPDIR` and
  `~/.config` are on different volumes).  If workspace files and temp files reside in the
  same directory, this is not an issue.  Writing the `.tmp` file into the same directory as
  the target before renaming ensures same-filesystem atomicity.

- **`PaneNode` serde shape**: The recursive enum `PaneNode { Leaf, HSplit, VSplit }` with
  `Box<PaneNode>` children serializes cleanly with serde's default enum representation.
  The exact TOML shape (inline table vs. array of tables) for nested splits should be
  verified empirically against the TOML spec's inline table restrictions before finalizing
  the schema.  Specifically, TOML 1.0 disallows multi-line inline tables, which could
  conflict with deeply nested trees if `toml::to_string` chooses inline representation.
  This needs a test.
