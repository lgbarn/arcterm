# CONCERNS.md

## Overview

ArcTerm is a fork of WezTerm at an early rebrand stage (Phase 1 just completed per `docs/plans/2026-03-18-wezterm-fork-plan.md`). The codebase is structurally sound -- it inherits WezTerm's mature, well-tested Rust foundation -- but carries a cluster of interrelated risks: an incomplete rebrand that leaves WezTerm identity in user-visible strings and CI pipelines, CI workflows still wired to upstream WezTerm release infrastructure, and a completely blank slate for the three headline AI/WASM features that are the entire purpose of the fork. There are no security emergencies, but the 855+ unsafe blocks in the non-vendor codebase require ongoing vigilance, and the update checker currently points at WezTerm's GitHub releases for a product now named ArcTerm.

---

## Findings

### 1. Upstream Drift Risk

- **Fork divergence is very low -- one commit of ArcTerm changes on top of upstream**: The most recent commit that diverges from upstream is `e8804618c` (shipyard initialization). All other recent commits (`05343b387` through `d2fc83559`) appear to be upstream WezTerm commits cherry-picked or merged. The rebrand changes are exclusively uncommitted working-tree modifications.
  - Evidence: `git log --oneline upstream/main..HEAD` shows only one ahead commit as of analysis time.
  - [Inferred] The fork was created very recently; merge conflicts are minimal today but will compound as ArcTerm-specific crates are added.

- **Rebrand changes are unstaged**: All 13 files with ArcTerm renaming are unstaged working-tree edits, not committed. They would be lost in a `git restore .` and are invisible to CI.
  - Evidence: `git status --short` lists `CONTRIBUTING.md`, `config/src/config.rs`, `lua-api-crates/termwiz-funcs/src/lib.rs`, `mux/src/domain.rs`, `mux/src/ssh.rs`, `mux/src/termwiztermtab.rs`, `mux/src/tmux_commands.rs`, `term/src/test/mod.rs`, `termwiz/src/caps/mod.rs`, `wezterm-gui/src/commands.rs`, `wezterm-gui/src/main.rs`, `wezterm-gui/src/overlay/confirm_close_pane.rs`, `wezterm-gui/src/update.rs` as modified but not staged.

- **Future merge risk from touching core files**: The rebrand modifies high-traffic upstream files that will receive ongoing changes from `wez/wezterm`. Merge conflicts are guaranteed in at least `config/src/config.rs`, `mux/src/domain.rs`, `mux/src/ssh.rs`, `wezterm-gui/src/main.rs`, and `wezterm-gui/src/commands.rs`.
  - Evidence: All listed in git status above.
  - Remediation: Commit the rebrand now. Where practical, isolate string constants into a single `arcterm-branding` module to minimize future merge surface.

---

### 2. Rebrand Completeness

**Severity: High** -- Several categories of WezTerm identity remain in user-visible strings, deployment artifacts, and CI pipelines.

#### 2a. User-Visible Strings Still Showing "WezTerm"

- **Update checker fetches from `wez/wezterm` GitHub releases and shows "wezterm" User-Agent**: ArcTerm users will be prompted to update to WezTerm releases, not ArcTerm releases. The User-Agent header sent to GitHub API also identifies as `wezterm/wezterm-<version>`.
  - Evidence: `wezterm-gui/src/update.rs` lines 44, 59, 64:
    ```rust
    "User-Agent",
    &format!("wezterm/wezterm-{}", wezterm_version()),
    // ...
    get_github_release_info("https://api.github.com/repos/wezterm/wezterm/releases/latest")
    get_github_release_info("https://api.github.com/repos/wezterm/wezterm/releases/tags/nightly")
    ```
  - [Note: The banner text "ArcTerm Update Available" was changed in the diff, but the URL still points at WezTerm releases.]
  - Remediation: Point at `lgbarn/arcterm` releases or disable the update checker until ArcTerm has its own release pipeline.

- **Font missing-glyph error messages link to `wezterm.org`**: Users seeing font errors are directed to upstream documentation.
  - Evidence: `wezterm-font/src/lib.rs` lines 418, 852:
    ```rust
    let url = "https://wezterm.org/config/fonts.html";
    ```

- **Exit behavior message links to `wezterm.org`**: Displayed inline in the terminal when a program exits.
  - Evidence: `mux/src/localpane.rs` line 269:
    ```
    \x1b]8;;https://wezterm.org/config/lua/config/exit_behavior.html\x1b\\exit_behavior
    ```

- **macOS quit dialog still says "Terminate WezTerm?"**: Internal Objective-C class name unchanged.
  - Evidence: `window/src/os/macos/app.rs` line 30: `let message_text = nsstring("Terminate WezTerm?");`

- **macOS Objective-C class names are "WezTermWindow", "WezTermWindowView", "WezTermAppDelegate"**: These are internal runtime class names but will appear in crash reports and diagnostics.
  - Evidence: `window/src/os/macos/window.rs` lines 1821-1822; `window/src/os/macos/app.rs` line 16; `window/src/os/macos/menu.rs` line 367.

- **macOS notification delegate class named "WezTermNotifDelegate"**: Appears in macOS notification system logs.
  - Evidence: `wezterm-toast-notification/src/macos.rs` line 36.

- **macOS app bundle is still `assets/macos/WezTerm.app/`**: The bundle directory name, its `Info.plist` (14 occurrences of "WezTerm"), and all CI packaging scripts reference the `WezTerm.app` path.
  - Evidence: `assets/macos/WezTerm.app/Contents/Info.plist` (14 occurrences); `ci/deploy.sh` lines 22, 30, 33-36.

- **Windows installer still branded as WezTerm**: The InnoSetup script has hardcoded publisher and URL.
  - Evidence: `ci/windows-installer.iss` lines 7-8:
    ```
    #define MyAppPublisher "Wez Furlong"
    #define MyAppURL "http://wezterm.org"
    ```

- **`build.rs` Windows VERSIONINFO still says "WezTerm"**: Sets the Windows binary's file description and product name metadata.
  - Evidence: `wezterm-gui/build.rs` lines 122, 127:
    ```
    VALUE "FileDescription", "WezTerm - Wez's Terminal Emulator\0"
    VALUE "ProductName",     "WezTerm\0"
    ```
  - Also: `wezterm-gui/build.rs` line 168 references the `WezTerm.app` path for macOS builds.

- **SSH multiplexing enum variant `SshMultiplexing::WezTerm`**: Public API name visible in Lua config.
  - Evidence: `config/src/ssh.rs` line 22 (enum definition); `wezterm-mux-server-impl/src/lib.rs` line 20 (match arm). Renaming this is a breaking change for users' Lua configs.
  - Remediation: Add `SshMultiplexing::ArcTerm` as an alias; deprecate `WezTerm` variant.

- **`strip-ansi-escapes` binary docstring**: Minor, but the utility identifies itself as part of WezTerm.
  - Evidence: `strip-ansi-escapes/src/main.rs` line 11: `/// This utility is part of WezTerm.`

- **Linux desktop entry still branded as WezTerm**: Affects taskbar, app launcher, and Wayland app_id.
  - Evidence: `assets/wezterm.desktop` lines 2, 5-6: `Name=WezTerm`, `Icon=org.wezfurlong.wezterm`, `StartupWMClass=org.wezfurlong.wezterm`.

- **AppStream/Flatpak metadata**: Full upstream identity including `org.wezfurlong.wezterm` app-id.
  - Evidence: `assets/wezterm.appdata.xml`, `assets/flatpak/org.wezfurlong.wezterm.json`.

- **Shell completion scripts embed the old window class string**: Users will see `org.wezfurlong.wezterm` in shell completions.
  - Evidence: `assets/shell-completion/zsh` (multiple lines), `assets/shell-completion/fish` (multiple lines).

#### 2b. CI/CD Pipeline Still Targeting Upstream Infrastructure

**Severity: Critical** -- If tag-based workflows are triggered, they will attempt to push releases to repositories owned by Wez Furlong.

- **macOS tag workflow pushes to `wez/homebrew-wezterm`**: Uses `GH_PAT` secret to checkout and commit to the upstream Homebrew tap.
  - Evidence: `.github/workflows/gen_macos_tag.yml` lines 108-118.

- **Ubuntu tag workflow pushes `.deb` to `push.fury.io/wez/`**: Uses `FURY_TOKEN` to upload packages to the upstream user's Gemfury account.
  - Evidence: `.github/workflows/gen_ubuntu22.04_tag.yml` lines 113-115:
    ```yaml
    run: "for f in wezterm*.deb ; do curl -i -F package=@$f https://$FURY_TOKEN@push.fury.io/wez/ ; done"
    ```

- **Ubuntu tag workflow pushes to `wez/homebrew-wezterm-linuxbrew`**.
  - Evidence: `.github/workflows/gen_ubuntu20.04_tag.yml` line 127.

- **Windows tag workflow creates PR to `wez/winget-pkgs`**.
  - Evidence: `.github/workflows/gen_windows_tag.yml` line 114.

- **Release notes template points at `wezterm.org` changelog and install pages**.
  - Evidence: `ci/create-release.sh` lines 6-13.

- **Deploy script packages under `WezTerm-*.zip` artifact names**.
  - Evidence: `ci/deploy.sh` lines 22-24, 104-110; `.github/workflows/gen_macos.yml` line 98: `path: "WezTerm-*.zip"`.

- **RPM/DEB `deploy.sh` sets Packager to `Wez Furlong <wez@wezfurlong.org>`**.
  - Evidence: `ci/deploy.sh` lines 191, 306.

- **FUNDING.yml still routes donations to upstream maintainer**.
  - Evidence: `.github/FUNDING.yml`: `github: wez`, `patreon: WezFurlong`, `ko_fi: wezfurlong`, `liberapay: wez`.

- **`ci/check-rust-version.sh` error message links to `wezterm.org`**.
  - Evidence: `ci/check-rust-version.sh` line 22: `echo "See https://wezterm.org/install/source.html"`.

---

### 3. Security Concerns

**Severity: Medium** -- No hardcoded credentials found. Concerns are structural.

- **Update checker sends unencrypted version string to GitHub API**: The request includes the ArcTerm/WezTerm version in the User-Agent; this is standard practice for GitHub API callers but worth noting.
  - Evidence: `wezterm-gui/src/update.rs` line 43-45.

- **~855 raw `unsafe` blocks across non-vendor Rust source**: Unsafe is pervasive in font rasterization (HarfBuzz FFI, FreeType FFI), PTY handling, windowing (macOS Objective-C bridge, Wayland, Win32), and the SSH session layer. This is inherited upstream code with a long test history, but any ArcTerm-added unsafe code in new crates must be reviewed carefully.
  - Evidence: `wezterm-font/src/hbwrap.rs`, `wezterm-font/src/ftwrap.rs`, `wezterm-font/src/fcwrap.rs`, `pty/src/unix.rs`, `pty/src/win/`, `window/src/os/macos/`, `wezterm-ssh/src/sessioninner.rs` (multiple).

- **SSH host key verification delegates to libssh2's known_hosts**: This is correct behavior, but the code path at `wezterm-ssh/src/host.rs` line 107 will prompt users interactively when a new key is encountered. There is no automated acceptance mechanism, which is safe but means headless/CI use of SSH is difficult.
  - Evidence: `wezterm-ssh/src/host.rs` lines 107-190.

- **macOS entitlements are overly broad**: The entitlement plist requests Bluetooth, Camera, Audio Input, Contacts, Calendar, Location, and Photos Library access. These were inherited from upstream WezTerm where they were set to allow child processes (shells) to request those permissions. For ArcTerm's AI use cases, this permission set will need to be reviewed -- AI panes connecting to external services may require additional entitlements (e.g., outgoing network connections) while the current blanket grants may prompt macOS to display confusing permission dialogs.
  - Evidence: `ci/macos-entitlement.plist`.

- **`cargo deny` is configured but not run in CI**: The `deny.toml` file exists and is properly structured, but no CI workflow invokes `cargo deny check`. Security advisories for dependencies will not be caught automatically.
  - Evidence: No `cargo deny` step found in any `.github/workflows/*.yml` file. `deny.toml` exists at project root.

- **No `cargo audit` or `cargo clippy` in CI**: Neither security audit nor lint checking is automated.
  - Evidence: Grep of `.github/workflows/` finds no audit or clippy steps. Only `cargo fmt` (in `fmt.yml`) and `cargo nextest run` are automated quality gates.

---

### 4. Build and CI

**Severity: Medium**

- **No `rust-toolchain.toml` or `rust-toolchain` file**: The minimum Rust version is pinned to 1.71.0 in `ci/check-rust-version.sh` as a shell check, but there is no `rust-toolchain.toml` to enforce this via `rustup`. CI uses `dtolnay/rust-toolchain@stable` (fmt workflow) or `dtolnay/rust-toolchain@nightly` without pinning a specific version. Contributors building locally will use whatever `rustc` they have installed.
  - Evidence: `ci/check-rust-version.sh` (minimum `1.71.0`); `fmt.yml` (nightly, unpinned); no `rust-toolchain.toml` found in repo root.

- **CI workflows are generated from Python scripts, not written directly**: The workflow files are prefixed `gen_` and `ci/generate-workflows.py` is the source of truth. Manually editing the `.yml` files will have changes overwritten.
  - Evidence: `ci/generate-workflows.py`; all workflow files named `gen_*.yml`.
  - [Inferred] Any ArcTerm-specific CI changes (e.g., pointing deployment at the correct repos) should be made in `generate-workflows.py`, not in the `.yml` files directly.

- **`MACOSX_DEPLOYMENT_TARGET` is `10.12`**: macOS 10.12 (Sierra, 2016) reached end-of-life in October 2019. Apple dropped support in Xcode 14 (2022). Building with this target on modern Xcode may produce warnings or fail.
  - Evidence: `.github/workflows/gen_macos.yml` line 29; `gen_macos_tag.yml` line 16; `gen_macos_continuous.yml` line 32.

- **`sccache-action` pinned to `v0.0.9`**: The sccache GitHub Action was at a very old version at time of analysis. Pinned action versions can become stale.
  - Evidence: `.github/workflows/gen_macos.yml` line 43: `uses: mozilla-actions/sccache-action@v0.0.9`.

- **Nix flake update workflow uses a PAT (`FLAKE_LOCK_GH_PAT`)**: This PAT would need to be provisioned in the fork's secrets for the flake lock auto-update to work.
  - Evidence: `.github/workflows/nix-update-flake.yml` line 24.

- **No secrets for macOS signing are provisioned in the fork**: The tag and continuous workflows reference `MACOS_CERT`, `MACOS_CERT_PW`, `MACOS_APPLEID`, `MACOS_APP_PW`, `MACOS_TEAM_ID` -- these were the upstream author's Apple Developer credentials and would need to be replaced with ArcTerm's own for code-signed builds.
  - Evidence: `.github/workflows/gen_macos_tag.yml` lines 82-83, 96.
  - Consequence: Without signing, macOS will quarantine the app on first launch and show Gatekeeper warnings. The macOS notification delegate (`WezTermNotifDelegate`) specifically notes signing is required for `UNUserNotificationCenter` to work: `wezterm-toast-notification/src/macos.rs` line 16-17.

---

### 5. Technical Debt (TODO/FIXME/HACK Comments)

**Severity: Low-Medium** -- All inherited from upstream WezTerm; none are ArcTerm-introduced.

- **`mux/src/termwiztermtab.rs` line 113**: Terminal writer is a `Vec::new()` (a black hole) with comment `// FIXME: connect to something?`. This means terminal output from the termwiz tab type is silently discarded rather than sent anywhere useful. This affects the overlay pane type.
  - Evidence: `mux/src/termwiztermtab.rs` line 113.

- **`mux/src/lib.rs` lines 1233, 1240, 1389**: Clipboard integration missing in mux pixel dimension calculations.
  - Evidence: `mux/src/lib.rs` lines 1233, 1240: `// FIXME: clipboard`, `// FIXME: split pane pixel dimensions`.

- **`mux/src/ssh.rs` line 266**: `// FIXME: this isn't useful without a way to talk to the remote mux.`
  - Evidence: `mux/src/ssh.rs` line 266.

- **`wezterm-client/src/client.rs` line 201**: `// FIXME: We currently get a bunch of these` (referring to a class of errors that is logged but not handled).
  - Evidence: `wezterm-client/src/client.rs` line 201.

- **`termwiz/src/render/terminfo.rs`**: Multiple TODO items around image rendering (sixel, iterm protocol) and terminfo feature detection.
  - Evidence: Lines 397, 419, 547, 602, 624, 1083.

- **`wezterm-input-types/src/lib.rs`**: Hyper and Meta modifier keys are not handled.
  - Evidence: Lines 1750, 1893: `// TODO: Hyper and Meta are not handled yet.`

- **Significant dead code suppression in font subsystem**: `wezterm-font/src/hbwrap.rs` and `wezterm-font/src/fcwrap.rs` contain numerous `#[allow(dead_code)]` and `#[allow(unused)]` attributes, suggesting the HarfBuzz and Fontconfig FFI wrappers expose more API surface than is actively used.
  - Evidence: `wezterm-font/src/hbwrap.rs` lines 24, 252, 291, 306, 373, 411, 416, 510, 522, 528, 530, 957, 994, 1013, 1026, 1033, 1073; `wezterm-font/src/fcwrap.rs` line 248.

---

### 6. Dependency Health

**Severity: Low** -- Dependencies are generally current; workspace-level version management is used consistently.

- **`bitflags` version split**: The workspace specifies `bitflags = "1.3"` but the lock file resolves both `1.3.2` and `2.10.0`. This indicates at least one transitive dependency requires bitflags v2. When bitflags v2 was released it had a breaking API change; using both versions adds binary size.
  - Evidence: `Cargo.toml` line 51; `Cargo.lock` (both versions present).

- **`mlua` at `0.9.9`; `mlua 0.10` was released**: mlua 0.10 introduced breaking API changes. Upgrading requires testing all Lua bindings in `lua-api-crates/` and `config/src/lua/`. For ArcTerm's planned Lua extension API for AI features, staying on 0.9 means building on an outdated API.
  - Evidence: `Cargo.toml` line (mlua = "0.9"); `Cargo.lock` (version "0.9.9"). [Inferred from mlua changelog knowledge -- version 0.10 released September 2024.]

- **`ssh2` workspace version `"0.9.3"` but lock resolves `0.9.5`**: Minor version drift between declared and resolved; not a concern but indicates the lock file was updated without updating the workspace manifest.
  - Evidence: `Cargo.toml` line 208; `Cargo.lock` (version "0.9.5").

- **`deny.toml` wildcards policy is `allow`**: The `bans.wildcards = "allow"` setting means wildcard version constraints (e.g., `version = "*"`) will not trigger any warning. Wildcard constraints are a security and reproducibility risk.
  - Evidence: `deny.toml` line 157.

- **`deny.toml` is effectively unconfigured for bans and advisories**: The `ignore`, `deny`, and `skip` lists are all empty/commented out. This is the template file with no project-specific rules applied.
  - Evidence: `deny.toml` lines 72-77, 178-184.

- **Ongoing dependency churn visible in commit history**: Recent commits include automated bumps for `lru`, `time`, `git2`, `bytes`. This is healthy maintenance but the velocity suggests upstream is actively managing security updates that ArcTerm will need to merge regularly.
  - Evidence: Git log (`build(deps): bump lru from 0.12.5 to 0.16.3`, `build(deps): bump time from 0.3.44 to 0.3.47`, etc.).

---

### 7. Performance Concerns

**Severity: Low** -- Inherited from WezTerm with no ArcTerm-specific changes yet.

- **Rendering pipeline has ~85 `Arc`/`Mutex`/`RwLock` instances in `mux/src/` alone**: The mux uses `parking_lot` locks (which are faster than `std::sync` locks) pervasively. High lock contention under AI workloads that inspect multiple panes simultaneously could become a bottleneck.
  - Evidence: `mux/src/lib.rs`, `mux/src/localpane.rs`, `mux/src/termwiztermtab.rs` (all use `parking_lot`). Count from grep: 85 instances.
  - [Inferred] AI cross-pane context reading (planned feature) will require taking read locks on multiple panes; the lock ordering must be established to avoid deadlocks.

- **Synchronous update checker blocks a dedicated thread**: The update check uses `http_req` (a synchronous HTTP library) on a dedicated thread spawned at startup. This is fine for update checks but is not a pattern suitable for AI API calls, which will need async (`reqwest` is already in the workspace for `sync-color-schemes`).
  - Evidence: `wezterm-gui/src/update.rs` lines 4-5 (`use http_req::request`), lines 225-232 (thread spawn).

- **No explicit performance profiling tooling in CI**: There is a `benchmarking` crate used in `wezterm-gui/src/shapecache.rs` (bench for text shaping) but it is not run in CI. No regression gates exist.
  - Evidence: `wezterm-gui/src/shapecache.rs` lines 259-304.

---

### 8. Missing Infrastructure for Planned Features

**Severity: High** -- The three planned ArcTerm extensions have no infrastructure yet.

#### 8a. WASM Plugin System

- **No `wasmtime` or equivalent in the workspace**: The WASM plugin crate (`arcterm-wasm-plugin`) described in `docs/plans/2026-03-18-wezterm-fork-plan.md` does not exist and `wasmtime` is not a workspace dependency. The existing `lua-api-crates/plugin/` is a Lua-based Git plugin loader (fetches Lua plugins from GitHub), not a WASM host.
  - Evidence: `Cargo.toml` -- no `wasmtime`; `lua-api-crates/plugin/src/lib.rs` (Git-based Lua plugin loader only); `deps/harfbuzz/harfbuzz/src/wasm/sample/rust/hello-wasm/` uses `wasm-bindgen 0.2` but only as a harfbuzz sample.
  - Missing: WASM component model loader, capability sandbox, plugin lifecycle management, host API surface.

- **No plugin dispatch hook in the terminal state machine**: To expose terminal state to WASM plugins, `term/src/terminalstate/performer.rs` would need to call plugin callbacks on relevant events (key input, output, OSC sequences). No such hook exists.
  - Evidence: `term/src/terminalstate/performer.rs` -- no plugin or hook call sites visible.

#### 8b. AI Integration Layer

- **No `arcterm-ai` crate**: The AI pane, Ollama client, and cross-pane context system described in `docs/plans/2026-03-17-local-llm-implementation.md` do not exist in any form. The plan references `arcterm-app/src/config.rs` paths from the original (pre-fork) arcterm project, which are not present here.
  - Evidence: `ls /Users/lgbarn/Personal/arcterm/ | grep arcterm` -- no arcterm-prefixed crates found. The plan's Task 1 references `arcterm-app/src/config.rs` which does not exist in this workspace.
  - Missing: Ollama HTTP client, AI config section, AI pane type, cross-pane context reader, command suggestion overlay.

- **`reqwest` is in the workspace but only used for the `sync-color-schemes` utility crate**: The async HTTP client that the AI integration will need is present as a workspace dependency but not wired into the GUI codebase yet.
  - Evidence: `Cargo.toml` line 184: `reqwest = "0.12"`; only `sync-color-schemes/Cargo.toml` uses it.

- **No async runtime in `wezterm-gui`**: The GUI binary uses WezTerm's custom `promise` executor rather than Tokio. Adding Tokio for AI HTTP streaming would require either integrating the two runtimes (possible but complex) or implementing streaming over the `promise` executor.
  - Evidence: `Cargo.toml` `tokio = "1.0"` is a workspace dependency; `wezterm-gui/Cargo.toml` would need to be checked for whether it opts in. [Inferred] The `promise` crate at `promise/` is a custom async executor used by WezTerm's GUI thread.

#### 8c. Structured Output (OSC 7770)

- **OSC 7770 is not registered**: Unknown OSC sequences fall through to `OperatingSystemCommand::Unspecified` and are logged as warnings (if `log_unknown_escape_sequences` is enabled) and dropped.
  - Evidence: `term/src/terminalstate/performer.rs` lines 772-782: `Unspecified` arm logs and discards.
  - Missing: OSC 7770 variant in `OperatingSystemCommand` enum (`wezterm-escape-parser/src/osc.rs`), renderer hook in `wezterm-gui/src/` to render rich content, Lua API for emitting structured content.

- **GPU renderer has no extension point for custom pane content**: The render pipeline in `wezterm-gui/src/termwindow/` renders terminal cell grids. Adding a side channel for structured rich content (syntax-highlighted blocks, diff views) will require non-trivial changes to the render pass architecture.
  - [Inferred] Based on overlay structure in `wezterm-gui/src/overlay/` -- overlays use the same terminal cell model; they do not bypass the cell renderer.

---

## Summary Table

| Concern | Severity | Category | Confidence |
|---------|----------|----------|------------|
| 13 rebrand files uncommitted (at risk from `git restore`) | Critical | Rebrand / Git | Observed |
| Update checker points at `wezterm/wezterm` releases | Critical | Rebrand / UX | Observed |
| Tag CI workflows push to `wez/` repos (Homebrew, Gemfury, winget) | Critical | CI / Rebrand | Observed |
| macOS app bundle is still `WezTerm.app` | High | Rebrand | Observed |
| Windows installer binary metadata says "WezTerm" | High | Rebrand | Observed |
| Quit dialog says "Terminate WezTerm?" | High | Rebrand / UX | Observed |
| `SshMultiplexing::WezTerm` enum variant in public Lua API | High | Rebrand / API | Observed |
| `FUNDING.yml` routes donations to upstream author | High | Rebrand | Observed |
| No WASM plugin infrastructure exists | High | Missing Feature | Observed |
| No AI integration crate exists | High | Missing Feature | Observed |
| OSC 7770 not registered, would be silently dropped | High | Missing Feature | Observed |
| macOS signing secrets not provisioned | High | CI / Build | Observed |
| No `cargo deny check` in CI | Medium | Security | Observed |
| No `cargo audit` or `cargo clippy` in CI | Medium | Security / Quality | Observed |
| `wezterm.org` URLs in user-visible error messages | Medium | Rebrand / UX | Observed |
| macOS Objective-C class names still "WezTerm*" | Medium | Rebrand | Observed |
| `MACOSX_DEPLOYMENT_TARGET` is EOL macOS 10.12 | Medium | Build | Observed |
| No `rust-toolchain.toml` to pin Rust version | Medium | Build | Observed |
| CI workflows generated from Python script (easy to accidentally overwrite) | Medium | CI | Observed |
| `deny.toml` not configured (bans/advisories lists empty) | Medium | Dependency Health | Observed |
| `bitflags` dual version 1.x and 2.x in dependency graph | Low | Dependency Health | Observed |
| `mlua` at 0.9.x; 0.10 available with breaking changes | Low | Dependency Health | Inferred |
| ~855 unsafe blocks inherited from upstream | Low | Security | Observed |
| `termwiztermtab` writer is `Vec::new()` (black hole) | Low | Technical Debt | Observed |
| No async runtime wired into GUI for AI HTTP streaming | Low | Architecture | Inferred |
| No profiling/benchmarking gates in CI | Low | Performance | Observed |
| Lock contention risk for AI cross-pane context reads | Low | Performance | Inferred |

---

## Open Questions

1. **Is there an Apple Developer account for ArcTerm?** Without one, macOS code signing and notarization (required for Gatekeeper) cannot be configured, and the macOS notification system will not function.

2. **Should `SshMultiplexing::WezTerm` be renamed or aliased?** Renaming breaks existing user configs. Adding `ArcTerm` as an alias and deprecating `WezTerm` is the lower-risk path, but requires keeping the old variant indefinitely.

3. **Which Rust async executor will AI HTTP streaming use?** Tokio is in the workspace but the GUI thread uses WezTerm's custom `promise` executor. The integration strategy (bridge, dual runtime, or rewriting the update checker on Tokio as a template) needs a decision before AI crate implementation begins.

4. **Will ArcTerm maintain separate release artifacts from WezTerm?** This determines the urgency of fixing the update checker URL and the CI deployment pipeline. If ArcTerm will distribute builds, the `WezTerm-*.zip` naming, Homebrew tap, and package registries all need to be stood up for the fork.

5. **Will the Linux `.desktop` file and app-id remain `org.wezfurlong.wezterm`?** Changing the app-id is a breaking change for users who have custom keybindings or autostart entries. A phased migration or compatibility alias would be needed.

6. **Will `wasmtime` use the component model or the core module API?** This determines the WASM plugin binary format users will need to target. The component model is newer and more capable but requires WIT interface definitions to be designed up front.
