---
phase: config-overlays-polish-release
plan: "8.3"
wave: 2
dependencies: ["8.1", "8.2"]
must_haves:
  - Key-to-screen latency <5ms
  - Cold start <100ms
  - Memory <50MB baseline, <60MB with 4 panes
  - Frame rate >120 FPS
  - CI passes on macOS, Linux, Windows
  - Binary builds for macOS (aarch64, x86_64), Linux (x86_64), Windows (x86_64)
files_touched:
  - arcterm-render/src/structured.rs
  - arcterm-app/src/main.rs
  - arcterm-app/Cargo.toml
  - arcterm-app/build.rs
  - .github/workflows/ci.yml
  - .github/workflows/release.yml
  - Cargo.toml
  - dist.toml
tdd: false
---

# Plan 8.3 -- Performance Optimization + CI/Release Packaging

**Wave 2** | Depends on Plans 8.1 and 8.2 | Final integration and release readiness

## Goal

Hit all performance targets (latency, cold start, memory, FPS), extend CI to run
`arcterm-app` tests on all three platforms, set up `cargo-dist` release workflow for
cross-platform binary builds, and generate the man page via `clap_mangen`. This plan
depends on Wave 1 completion because performance measurement must cover the overlay
and search features, and the release binary must include all Phase 8 code.

---

<task id="1" files="arcterm-render/src/structured.rs, arcterm-app/src/main.rs" tdd="false">
  <action>
    Optimize cold start and latency:

    1. **Lazy syntect initialization**: In `arcterm-render/src/structured.rs`, change
       `HighlightEngine` to use lazy initialization. Replace the eager `SyntaxSet::load_defaults_newlines()`
       and `ThemeSet::load_defaults()` calls in `HighlightEngine::new()` with
       `std::sync::OnceLock<SyntaxSet>` and `std::sync::OnceLock<ThemeSet>` that are populated
       on first call to `highlight_code()` or `highlight_markdown()`. Add a `fn ensure_loaded(&self)`
       private method that initializes the OnceLock fields. Update `HighlightEngine::new()` to
       create an empty struct (no loading). This moves the ~23ms syntect load cost out of the
       cold start path.

    2. **Deferred plugin loading**: In `main.rs`, move the `PluginManager::new()` and
       `load_installed_plugins()` calls from the `AppState` constructor into a deferred
       initialization block that runs after the first frame is rendered. Add a
       `plugins_loaded: bool` flag to `AppState`. In `about_to_wait`, after the first
       `render_multipane` call (when `fps_frame_count == 1`), load plugins if
       `!self.plugins_loaded`. This ensures the first frame renders before plugin WASM
       compilation begins.

    3. **Latency trace completion**: In `main.rs`, under `#[cfg(feature = "latency-trace")]`,
       add a full round-trip measurement: capture `key_press_t0: Option<Instant>` on
       `KeyboardInput` events, and log `[latency] key → frame presented: {:?}` after the
       `gpu.present_frame()` call in the same render cycle. This closes the measurement gap
       identified in RESEARCH.md.

    4. **Memory audit**: In `arcterm-core/src/grid.rs`, ensure `Grid::new()` does not
       pre-allocate scrollback capacity beyond 256 rows. Check that
       `scrollback: VecDeque::with_capacity(min(max_scrollback, 256))` is used rather than
       allocating for the full `max_scrollback` (10,000 rows) upfront. If it currently
       pre-allocates the full amount, change to incremental growth.

    Verify all targets by running:
    ```
    cargo run --package arcterm-app --features latency-trace --release 2>&1 | grep '\[latency\]'
    ```
    and checking: cold start < 100ms, key-to-frame < 5ms.

    Check memory with:
    ```
    /usr/bin/time -l cargo run --package arcterm-app --release 2>&1 | grep 'maximum resident'
    ```
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app --release 2>&1 | tail -5 && cargo build --package arcterm-render 2>&1 | tail -5</verify>
  <done>Release build succeeds. `HighlightEngine::new()` no longer loads syntect eagerly (OnceLock deferred). Plugin loading deferred to after first frame. Latency trace covers key-to-frame-presented. Grid scrollback does not pre-allocate beyond 256 rows.</done>
</task>

<task id="2" files="arcterm-app/Cargo.toml, arcterm-app/build.rs, .github/workflows/ci.yml" tdd="false">
  <action>
    Add man page generation and extend CI:

    1. **Man page**: Add `clap_mangen = "0.2"` to `[build-dependencies]` in
       `arcterm-app/Cargo.toml`. Create `arcterm-app/build.rs` with:
       - Import the `Cli` struct via `include!("src/main.rs")` is not feasible due to
         dependencies. Instead, use a build script pattern: add a `cli_command()` function
         in `main.rs` that returns `clap::Command` (call `Cli::command()` from the
         `CommandFactory` trait). In `build.rs`, use `clap_mangen::Man::new(cmd).render()`
         to write `OUT_DIR/arcterm.1`. If `build.rs` cannot import `Cli` directly, create
         a standalone `cli.rs` module that defines just the clap structs (no runtime deps)
         and is shared between `build.rs` and `main.rs`. The simplest approach: in `build.rs`,
         construct a manual `clap::Command` mirroring the derive struct, generate the man page,
         and write to `OUT_DIR/arcterm.1`. Add a `man` cargo alias or document the path.

    2. **CI extension**: Update `.github/workflows/ci.yml`:
       - In the `check` job, add `arcterm-app` to the test command:
         `cargo test --package arcterm-core --package arcterm-vt --package arcterm-pty --package arcterm-app`
         (arcterm-app tests are unit tests that do not require GPU).
       - Add a `clippy-all` step that runs clippy on the full workspace including arcterm-app
         to catch warnings from the new overlay and search modules.
       - Verify the matrix already covers `ubuntu-latest`, `macos-latest`, `windows-latest`
         (it does per current ci.yml). Ensure `arcterm-app` builds on Windows by checking
         for any Unix-only imports guarded by `#[cfg(unix)]`.

    3. **Example configs**: Create `examples/config/` directory with:
       - `examples/config/base.toml`: a fully commented example `config.toml` showing all fields
       - `examples/config/overlay-font.toml`: example overlay that changes font_size and font_family
       - `examples/config/overlay-colors.toml`: example overlay that sets a custom color scheme
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app 2>&1 | tail -5 && ls arcterm-app/build.rs && cat .github/workflows/ci.yml | head -30</verify>
  <done>`build.rs` exists and generates man page to `OUT_DIR/arcterm.1`. CI yml includes `arcterm-app` in test matrix. Example config files exist in `examples/config/`. Build succeeds on local platform.</done>
</task>

<task id="3" files="Cargo.toml, dist.toml, .github/workflows/release.yml" tdd="false">
  <action>
    Set up cargo-dist release workflow:

    1. Install cargo-dist locally: `cargo install cargo-dist`.

    2. Run `cargo dist init` in the workspace root. When prompted:
       - Select targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`,
         `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`.
       - Select CI: GitHub Actions.
       - Select installers: shell (Unix), powershell (Windows).
       This generates `dist.toml` in the workspace root and
       `.github/workflows/release.yml`.

    3. Review and adjust the generated `dist.toml`:
       - Set `[dist]` `cargo-dist-version` to the installed version.
       - Ensure `targets` lists all four targets.
       - Set `ci = ["github"]`.
       - Set `allow-dirty = ["ci"]` if needed for the initial setup.

    4. Review `.github/workflows/release.yml`:
       - Verify it triggers on tag push (`on: push: tags: ["v*"]`).
       - Verify it builds on native runners for each target (macos-latest for aarch64,
         macos-13 or equivalent for x86_64, ubuntu-latest for linux, windows-latest for
         windows).
       - Verify it creates a GitHub Release with checksums.

    5. Test locally: `cargo dist build --artifacts=local` to verify the local binary is
       produced successfully. Check the output includes the man page if available.

    6. Update the workspace `Cargo.toml` with any metadata cargo-dist requires:
       - Ensure `[workspace.package]` has `description`, `repository`, `homepage` fields
         (cargo-dist uses these for release notes).
       - Add `description = "GPU-rendered AI terminal emulator"` if absent.
       - Add `repository = "https://github.com/lgbarn/arcterm"` (or correct URL).
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && test -f dist.toml && test -f .github/workflows/release.yml && cargo dist build --artifacts=local 2>&1 | tail -15</verify>
  <done>`dist.toml` exists with all four targets configured. `release.yml` triggers on version tags and builds on native runners for macOS (aarch64 + x86_64), Linux, and Windows. `cargo dist build --artifacts=local` produces a binary successfully. Workspace Cargo.toml has description and repository metadata.</done>
</task>
