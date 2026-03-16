# Arcterm Roadmap

> AI Command Center Terminal Emulator -- from empty directory to full AI-native terminal.
>
> Each phase is independently shippable. At the end of every phase, arcterm is a usable (if limited) terminal.

---

## Phase 1: Foundation -- Window, Shell, Pixels

**Goal:** Get a GPU-rendered window on screen running an interactive shell session with text input and output.

**Success Criteria:**
- `cargo build` produces a binary that opens a native window on macOS
- Typing characters in the window sends them to a PTY-backed shell and displays the output
- Basic VT100 sequences (cursor movement, color, erase) render correctly
- `ls`, `vim`, `top`, and `htop` produce usable (not necessarily perfect) output
- Key-to-screen latency is measurable and under 16ms (one frame at 60 FPS)
- Cold start is under 500ms (tightened in later phases)
- CI runs `cargo build`, `cargo test`, and `cargo clippy` on every push

**Scope:**
- Cargo workspace setup (`arcterm-core`, `arcterm-vt`, `arcterm-pty`, `arcterm-render`, `arcterm-app`)
- PTY allocation and shell spawning via `tokio` (fork/exec on Unix, ConPTY on Windows)
- VT parser -- custom Rust implementation referencing `vte` crate, covering VT100/VT220 core sequences
- Terminal grid model (cells, rows, scrollback buffer, dirty tracking)
- wgpu renderer -- window creation via `winit`, glyph atlas via `swash` + `cosmic-text`, solid cursor
- Input handling -- keyboard events mapped to terminal input bytes, basic mouse reporting
- CI pipeline (GitHub Actions: build + test + clippy for macOS, Linux, Windows)

**Risk:** wgpu surface creation and font rasterization are the highest-uncertainty items. Tackle renderer first to surface platform issues early.

**Estimated Scope:** ~30% of total project effort. This phase is the largest because it establishes every foundational subsystem.

---

## Phase 2: Terminal Fidelity and Configuration

**Goal:** Achieve terminal emulation quality sufficient for daily-driver use with a single pane, plus a TOML configuration system.

**Success Criteria:**
- Passes 90%+ of `vttest` basic and cursor movement tests
- 256-color and truecolor (24-bit) rendering works correctly
- Neovim, tmux, and SSH sessions render without visual artifacts
- Scrollback buffer supports 10,000+ lines with smooth GPU-accelerated scroll
- `~/.config/arcterm/config.toml` controls font family, font size, color scheme, keybindings, and shell path
- Selection and clipboard (copy/paste) work via mouse drag and keyboard shortcuts
- Frame rate exceeds 120 FPS during fast `cat`-of-large-file output

**Scope:**
- Extended VT parsing -- full xterm-256color compatibility, DEC private modes, alternate screen buffer, bracketed paste
- Truecolor and 256-color palette support in renderer
- Scrollback buffer with viewport management and GPU-accelerated smooth scrolling
- TOML configuration loading with hot-reload on file change
- Text selection model (character, word, line) with system clipboard integration
- URL detection and clickable links (OSC 8 hyperlinks)
- Performance profiling and optimization pass (glyph cache tuning, dirty-rect rendering)

**Risk:** VT compatibility is a long tail. Define "daily-driver ready" by the specific programs listed in success criteria rather than chasing 100% vttest compliance.

**Estimated Scope:** ~20% of total project effort.

---

## Phase 3: Multiplexer -- Panes, Tabs, and Navigation

**Goal:** Transform arcterm from a single-pane terminal into a vim-navigable multiplexer with splits, tabs, and Neovim-aware pane crossing.

**Success Criteria:**
- Horizontal and vertical splits create independent PTY-backed panes
- `Ctrl+h/j/k/l` navigates between panes
- When the focused pane runs Neovim, directional keys traverse Neovim splits first, then cross to arcterm panes
- Tabs group pane layouts; switching tabs is instant
- `Leader+n` splits, `Leader+q` closes, `Leader+z` zooms (fullscreen toggle)
- Pane resize via `Leader+arrow` or mouse drag
- No measurable latency regression from Phase 2 single-pane performance

**Scope:**
- Pane tree layout engine (binary splits with configurable ratios)
- Tab model with independent pane trees per tab
- Vim-style keybinding layer with configurable leader key
- Neovim-aware pane crossing (detect Neovim process, coordinate via `--remote-expr` or socket)
- Pane border rendering and focus indicator in wgpu
- Command palette foundation (`Ctrl+Space`) -- initially just pane/tab management commands

**Risk:** Neovim-aware crossing requires detecting the Neovim process and communicating with it. Start with process-name detection and Neovim's `--listen` socket; fall back gracefully if Neovim is not detected.

**Estimated Scope:** ~15% of total project effort.

---

## Phase 4: Structured Output and Smart Rendering

**Goal:** Implement the Structured Output Protocol (OSC 7770) and auto-detection fallback so that AI tool output renders as rich, interactive content.

**Success Criteria:**
- A test CLI emitting OSC 7770 sequences renders code blocks with syntax highlighting, diffs with side-by-side view, and collapsible JSON trees
- Fenced code blocks in plain output auto-detect as `code_block` and render with syntax highlighting
- Unified diff output auto-detects and renders with file headers and color
- JSON blobs auto-detect and render as collapsible trees
- Markdown headings, lists, and inline code render with formatting
- Non-protocol tools (standard shell commands, legacy programs) render identically to Phase 2 -- zero interference
- Kitty graphics protocol displays inline images

**Scope:**
- OSC 7770 parser integrated into VT parser (start/end delimiters, typed content dispatch)
- Rich content renderers: code block (syntax highlighting via `syntect` or `tree-sitter`), diff, JSON tree, markdown, error, progress
- Auto-detection engine for non-protocol output (regex-based pattern matching on output stream)
- Kitty graphics protocol implementation (image upload, placement, display via wgpu textures)
- Content interaction: code block copy button, JSON node collapse/expand, diff navigation

**Risk:** Auto-detection of unstructured output can produce false positives. Use conservative heuristics and let users disable auto-detection per-pane. Kitty graphics is complex; implement basic raster image display first.

**Estimated Scope:** ~12% of total project effort.

---

## Phase 5: Workspaces and Session Persistence

**Goal:** Enable project-aware workspaces that restore full terminal layouts, commands, environment, and scrollback from TOML workspace files.

**Success Criteria:**
- `arcterm open <workspace>` reads a TOML workspace file and restores the defined pane layout, commands, working directories, and environment variables
- Session persistence survives process exit and reboot -- reopening arcterm restores the last session
- `Leader+w` opens a fuzzy workspace switcher listing available workspace files
- Workspace TOML files are human-readable, git-committable, and manually editable
- Workspace restore completes in under 500ms for a 4-pane layout

**Scope:**
- Workspace TOML schema definition and parser (panes, positions, commands, environment, metadata)
- Session serialization -- capture current state (pane tree, working directories, scrollback references) to disk
- Session restore on startup (crash recovery and intentional reopen)
- Fuzzy workspace switcher UI (rendered in wgpu, keyboard navigable)
- `arcterm open`, `arcterm save`, `arcterm list` CLI subcommands
- Scrollback persistence (optional, configurable -- serialize to disk or discard)

**Risk:** Session persistence across reboots requires careful serialization of PTY state. Accept that running processes cannot be restored -- only layout, directories, and commands are replayed.

**Estimated Scope:** ~8% of total project effort.

---

## Phase 6: WASM Plugin System

**Goal:** Ship a wasmtime-based plugin runtime where third-party WASM modules can render UI in panes, subscribe to events, and register tools.

**Success Criteria:**
- A "hello world" WASM plugin (compiled from Rust) loads in under 50ms, renders text in a pane, and responds to keyboard input
- Plugin manifest (TOML) declares permissions; arcterm enforces capability-based sandbox (filesystem paths, network, pane read access)
- Plugins receive events: pane opened, pane closed, command executed, workspace switched
- `arcterm plugin install <path>`, `arcterm plugin dev ./my-plugin` work for local development
- A non-trivial example plugin (e.g., system monitor dashboard) demonstrates the full API surface
- Memory overhead per plugin is under 10MB

**Scope:**
- wasmtime integration -- WASM module loading, instance lifecycle, memory limits
- Plugin host API (wit-bindgen interface): pane rendering, event subscription, configuration access
- Capability-based permission system parsing from plugin manifest TOML
- Plugin UI rendering surface (pane type `plugin` backed by WASM-driven draw commands)
- Plugin manager CLI (`install`, `list`, `remove`, `dev`)
- Plugin event bus (pub/sub for terminal events routed to interested plugins)

**Risk:** Designing a stable plugin API is the hardest part. Ship a `v0` API with explicit instability guarantees. Use wit-bindgen for the interface definition to get type-safe bindings across languages.

**Estimated Scope:** ~8% of total project effort.

---

## Phase 7: AI Integration Layer

**Goal:** Make AI CLI tools first-class citizens -- automatic detection, cross-pane context sharing, MCP-compatible plugin tool discovery, and the plan status layer.

**Success Criteria:**
- Claude Code, Codex CLI, and Gemini CLI are automatically detected as AI agents when launched in a pane (pane type switches to `ai-agent`)
- AI panes can read structured context from sibling panes (last command, exit code, working directory, recent output)
- WASM plugins expose MCP-compatible tool schemas; an AI agent can discover and invoke them
- `Leader+p` toggles the plan status strip showing current phase/task progress from `.shipyard/` or `PLAN.md`
- `Leader+a` jumps focus to the most recently active AI pane
- Error bridging: a build failure in one pane surfaces as structured context available to AI panes

**Scope:**
- AI tool detection engine (process name matching, protocol handshake via OSC 7770 self-identification, heuristic fallback)
- Cross-pane context API (per-pane metadata: CWD, last command, exit code, recent output ring buffer)
- MCP tool registry -- plugins register tools, AI agents query available tools via structured protocol
- Plan status layer (ambient strip renderer, `.shipyard/` and `PLAN.md` file watcher, `Leader+p` expanded view)
- AI pane context injection (format sibling context for AI tool consumption)
- `Leader+a` keybinding and AI pane tracking

**Risk:** Cross-pane context sharing must be opt-in and privacy-respecting. Default to sharing only metadata (CWD, exit code); require explicit configuration for output sharing. MCP compatibility may evolve -- implement the core tool discovery/invocation pattern, not the full MCP spec.

**Estimated Scope:** ~5% of total project effort.

---

## Phase 8: Config Overlays, Polish, and Release

**Goal:** Complete the config overlay system, hit all performance targets, and ship a public release.

**Success Criteria:**
- AI config overlay workflow works end-to-end: AI writes pending overlay, `Leader+o` shows diff, user accepts/rejects/edits
- `arcterm config flatten` exports the fully resolved config as a single TOML file
- Key-to-screen latency is under 5ms (measured with input latency tooling)
- Cold start is under 100ms
- Memory baseline is under 50MB with zero panes, under 60MB with 4 panes
- Frame rate exceeds 120 FPS during fast output scrolling
- All CI checks pass on macOS, Linux, and Windows
- Binary builds are available for macOS (aarch64, x86_64), Linux (x86_64), and Windows (x86_64)
- Search across all pane output (`Leader+/`) works with regex support

**Scope:**
- Config overlay system (overlay directory, pending/accepted states, diff renderer, accept/reject/edit flow)
- `arcterm config flatten` CLI subcommand
- Cross-pane search (`Leader+/`) with regex, rendered inline with match highlighting
- Performance optimization pass (latency profiling, memory audit, startup time reduction)
- Release packaging (cross-compilation, binary signing for macOS, installer for Windows)
- Documentation (man page, `--help` completeness, example configs, example workspace files, plugin authoring guide)

**Risk:** Performance targets (5ms latency, 100ms cold start) require disciplined profiling. Allocate time for measurement-driven optimization rather than guessing. Config overlay UX needs user testing -- the diff/accept flow must feel natural.

**Estimated Scope:** ~2% new feature code, but significant polish, testing, and packaging effort.

---

## Phase Dependency Graph

```
Phase 1 ──> Phase 2 ──> Phase 3 ──> Phase 4
                                       │
                                       v
                          Phase 5 ──> Phase 6 ──> Phase 7 ──> Phase 8
```

Phases are sequential by design. Each phase builds directly on the subsystems established in the prior phase. Within each phase, individual plans will define parallel waves where possible.

## Cumulative Milestones

| After Phase | Arcterm Is... |
|---|---|
| 1 | A basic GPU-rendered terminal that runs a shell |
| 2 | A daily-driver single-pane terminal with config |
| 3 | A vim-navigable multiplexer rivaling tmux |
| 4 | A terminal that renders AI output as rich content |
| 5 | A project-aware workspace manager |
| 6 | An extensible terminal with a plugin ecosystem |
| 7 | An AI command center with intelligent pane coordination |
| 8 | A polished, performant, release-ready product |

---

# v0.1.1 — Stabilization Release

> Fix all known bugs, security gaps, and functional stubs from the v0.1.0 review cycle.
> No new features. Every existing feature becomes robust, correct, and safe.
>
> Phases 9-11 resolve 13 open issues (ISSUES.md) + 2 High + 6 Medium concerns (CONCERNS.md).
> Low-severity concerns are deferred to v0.2.0.

---

## Phase 9: Foundation Fixes — Grid, VT, PTY, Plugin (Parallel)

**Goal:** Fix all bugs in the four independent crates (`arcterm-core`, `arcterm-vt`, `arcterm-pty`, `arcterm-plugin`) simultaneously. These crates share no source files and can be worked in parallel.

**Groups included:**
- Group 1 — Grid fixes (`arcterm-core/src/grid.rs`): ISSUE-007, ISSUE-008, ISSUE-009, ISSUE-010
- Group 2 — VT/parser fixes (`arcterm-vt/src/processor.rs`, `arcterm-vt/src/handler.rs`): ISSUE-011, ISSUE-012, ISSUE-013
- Group 4 — PTY fix (`arcterm-pty/src/session.rs`): ISSUE-001
- Group 5 — Plugin fixes (`arcterm-plugin/src/runtime.rs`, `arcterm-plugin/src/manager.rs`, `arcterm-plugin/src/manifest.rs`): H-1, H-2, M-1, M-2, M-6

**Success Criteria:**
- `set_scroll_region()` rejects invalid bounds (top >= bottom, out-of-range values) without panic — ISSUE-007
- `resize()` resizes `alt_grid` when present — ISSUE-008
- `scroll_offset` is a private field with a validated setter that clamps to scrollback length — ISSUE-009
- Scroll operations use in-place copy instead of O(n*rows) remove/insert loops — ISSUE-010
- `esc_dispatch` returns early when intermediates are non-empty — ISSUE-011
- Modes 47, 1047, 1000, 1002, 1003, 1006 are handled in `set_mode`/`reset_mode` — ISSUE-012
- Unreachable newline clamp removed; test covers cursor-above-scroll-region behavior — ISSUE-013
- `PtySession.writer` is `Option<Box<dyn Write + Send>>`, shutdown uses `.take()`, writes after shutdown return `BrokenPipe` — ISSUE-001
- `engine.increment_epoch()` ticks on a background task; stores set `epoch_deadline` before WASM calls — H-1
- `call_tool()` dispatches to the actual WASM function instead of returning a stub — H-2
- `KeyInput` event kind returns a dedicated variant or `unreachable!()` instead of `PaneOpened` — M-1
- `wasm` field in `plugin.toml` is validated against path traversal (`..`, absolute paths, backslashes) — M-2
- `copy_plugin_files` rejects symlinks in the source directory — M-6
- Each fix includes at least one regression test
- `cargo test -p arcterm-core -p arcterm-vt -p arcterm-pty -p arcterm-plugin` passes
- `cargo clippy -p arcterm-core -p arcterm-vt -p arcterm-pty -p arcterm-plugin -- -D warnings` clean

**Scope:**
- 4 crates, 4 independent work streams (can be 4 parallel plans)
- ~15 individual code changes across 8 source files
- Grid fixes are pure logic with strong TDD potential
- VT fixes are state-machine corrections with clear test vectors
- PTY fix is a type-level API change with straightforward test
- Plugin fixes span security (M-2, M-6), correctness (M-1), and functionality (H-1, H-2)

**Risk:** H-2 (real WASM tool dispatch) is the highest-effort item in this phase. It requires adding a WIT export for tool handling and wiring the dispatch through wasmtime. If it blocks, the other 3 groups can still ship independently. Scope H-2 to dispatch-only (call the function, return its result) without retry or timeout logic.

**Estimated Scope:** ~60% of v0.1.1 effort. This phase contains the majority of fixes but benefits from full parallelism across 4 crates.

---

## Phase 10: Application Input and UX Fixes

**Goal:** Fix all bugs in `arcterm-app` that do not overlap with `arcterm-core` changes. These are input handling, error recovery, and visual feedback fixes.

**Groups included:**
- Group 3 — App/input fixes (`arcterm-app/src/main.rs`, `arcterm-app/src/input.rs`, `arcterm-render/src/text.rs`): ISSUE-002, ISSUE-003, ISSUE-004, ISSUE-005, ISSUE-006

**Success Criteria:**
- Keyboard input triggers `request_redraw()` so typed characters appear immediately without waiting for PTY echo — ISSUE-002
- `Ctrl+\` sends 0x1c (SIGQUIT) and `Ctrl+]` sends 0x1d — ISSUE-003
- PTY creation failure logs an error and exits cleanly instead of panicking — ISSUE-004
- Shell exit displays a visible "Shell exited" indicator in the terminal window — ISSUE-005
- Cursor renders as a visible block on blank/space cells (not invisible inverse-video of whitespace) — ISSUE-006
- Each fix includes at least one regression test
- `cargo test -p arcterm-app` passes (plus all Phase 9 tests still pass)
- `cargo clippy -p arcterm-app -- -D warnings` clean
- Manual verification: launch arcterm, type characters, confirm immediate redraw; press Ctrl+\, confirm SIGQUIT delivery; exit shell, confirm "Shell exited" overlay; move cursor to empty cell, confirm visible cursor block

**Scope:**
- 1 crate (`arcterm-app`), plus a renderer touch (`arcterm-render/src/text.rs` for ISSUE-006)
- 5 fixes, mostly small and localized (1-10 lines each except ISSUE-005 which adds a state field + overlay render path)
- ISSUE-005 is the largest item: requires a `shell_exited` flag in `AppState`, detection logic in the PTY recv loop, and a text overlay in the redraw path
- ISSUE-006 touches the renderer but only the cursor-drawing logic (no structural change to `arcterm-render`)

**Risk:** ISSUE-005 (shell exit indicator) and ISSUE-006 (cursor visibility) both touch the render path. If the cursor fix requires a new wgpu draw pass (solid rectangle), scope it to a block-character glyph substitute first (U+2588) and defer the dedicated quad pass to v0.2.0.

**Estimated Scope:** ~20% of v0.1.1 effort. All fixes are small and well-scoped.

---

## Phase 11: Config and Runtime Hardening

**Goal:** Fix the remaining concerns that touch `arcterm-app` and `arcterm-core` — async image decode, config validation, and GPU initialization safety. This phase is serialized after Phase 9 (arcterm-core changes) and Phase 10 (arcterm-app changes) to avoid file conflicts.

**Groups included:**
- Group 6 — Config/runtime fixes (`arcterm-app/src/terminal.rs`, `arcterm-app/src/config.rs`, `arcterm-render/src/gpu.rs`, `arcterm-core/src/grid.rs`): M-3, M-4, M-5

**Success Criteria:**
- Kitty image decode runs on `tokio::task::spawn_blocking` instead of inline in the PTY processing loop — M-3
- `scrollback_lines` is capped at 1,000,000 (or a configurable maximum) during config load, with a warning logged when clamped — M-4
- `GpuState::new()` returns `Result` instead of panicking; callers display a user-facing error message on GPU init failure — M-5
- Each fix includes at least one regression test (M-4: config parse test with extreme value; M-5: unit test that `GpuState::new` returns `Err` on invalid adapter request if testable, otherwise integration-level assertion)
- Full workspace passes: `cargo test --workspace` succeeds
- `cargo clippy --workspace -- -D warnings` clean
- No remaining `.expect()` or `.unwrap()` on fallible operations in `arcterm-app` or `arcterm-render` runtime code paths (build scripts and tests excluded)

**Scope:**
- 3 fixes across 4 files in 3 crates (`arcterm-app`, `arcterm-render`, `arcterm-core`)
- M-3 (async image decode) is the most invasive: changes the return type of image processing from synchronous `Vec<PendingImage>` to a channel-based async drain. Requires touching `terminal.rs` and the PTY drain loop in `main.rs`
- M-4 (scrollback cap) is a 5-line config validation change
- M-5 (GPU init) is a signature change from `-> GpuState` to `-> Result<GpuState, String>` that propagates through `Renderer::new()` and `AppState::new()`

**Risk:** M-3 (async image decode) changes the data flow for Kitty images. The synchronous path is simple; the async replacement must handle the case where decoded images arrive one frame late. Ensure the render loop drains completed decode tasks before each frame to minimize visual delay. M-5 changes constructor signatures in `arcterm-render` which propagate to `arcterm-app` — verify all call sites are updated.

**Estimated Scope:** ~20% of v0.1.1 effort. Fewer items but higher per-item complexity due to API signature changes.

---

## v0.1.1 Phase Dependency Graph

```
Phases 1-8 (v0.1.0, all complete)
          │
          v
     ┌─ Phase 9 ─────────────────────────────────────────┐
     │  Group 1: arcterm-core  (ISSUE 7,8,9,10)          │
     │  Group 2: arcterm-vt    (ISSUE 11,12,13)          │  ← 4 parallel work streams
     │  Group 4: arcterm-pty   (ISSUE 1)                 │
     │  Group 5: arcterm-plugin (H-1,H-2,M-1,M-2,M-6)   │
     └────────────────┬──────────────────────────────────┘
                      │
                      v
              ┌─ Phase 10 ──────────────────────────┐
              │  Group 3: arcterm-app (ISSUE 2-6)    │  ← serialized (shares arcterm-app)
              └──────────────┬───────────────────────┘
                             │
                             v
                     ┌─ Phase 11 ──────────────────────────────┐
                     │  Group 6: arcterm-app + arcterm-core     │  ← serialized (shares both)
                     │          (M-3, M-4, M-5)                 │
                     └──────────────────────────────────────────┘
```

**Phase 9** has no dependencies on other v0.1.1 phases. Its 4 groups (touching 4 separate crates) can each be a separate plan executed in parallel.

**Phase 10** depends on Phase 9 completing. Grid fixes (ISSUE-007 through ISSUE-010) and VT fixes (ISSUE-011 through ISSUE-013) change APIs in `arcterm-core` and `arcterm-vt` that `arcterm-app` imports. Building Phase 10 against unstable Phase 9 APIs risks rework.

**Phase 11** depends on both Phase 9 and Phase 10. It touches `arcterm-core/src/grid.rs` (M-4 interacts with scrollback) and `arcterm-app/src/terminal.rs` (M-3), both of which are modified in earlier phases.

## v0.1.1 Cumulative Milestones

| After Phase | Arcterm Is... |
|---|---|
| 9 | Grid, VT, PTY, and plugin subsystems are individually correct and hardened |
| 10 | User-facing input, error handling, and visual feedback are reliable |
| 11 | Runtime resource limits, async safety, and GPU init resilience are in place — v0.1.1 ships |

## v0.1.1 Release Criteria

All of the following must be true before tagging v0.1.1:

- All 13 ISSUES.md items moved to "Resolved" section
- Both High concerns (H-1, H-2) resolved
- All 6 Medium concerns (M-1 through M-6) resolved
- `cargo test --workspace` passes with test count higher than v0.1.0 baseline (558+)
- `cargo clippy --workspace -- -D warnings` clean
- No new `.expect()` or `.unwrap()` on fallible operations in runtime code
- No new `#[allow(dead_code)]` suppressions added
