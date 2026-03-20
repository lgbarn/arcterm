# CONCERNS.md

## Overview

ArcTerm has made substantial progress since the initial analysis: the rebrand is now largely committed and reflected in the codebase, both headline ArcTerm-specific crates (`arcterm-ai` and `arcterm-wasm-plugin`) exist and contain working implementations, and the CI pipeline no longer targets upstream `wez/` infrastructure. The primary risk surface has shifted from "nothing is built" to "prototype-stage security gaps that will become critical the moment the features ship." The most urgent item is the incomplete plugin instantiation wiring in `arcterm-wasm-plugin`: a future commit that wires the guest `Instance` will silently enable the unconditional `terminal:read` grant and any other gaps flagged in AUDIT-main.md if they are not fixed first.

---

## Findings

### 1. Upstream Drift Risk

- **Fork divergence remains manageable but will compound**: ArcTerm-specific code is isolated in `arcterm-ai/`, `arcterm-wasm-plugin/`, and `wezterm-gui/src/ai_pane.rs` plus `wezterm-gui/src/overlay/ai_command_overlay.rs`. The rebrand changes to core upstream files are now committed.
  - Evidence: `CLAUDE.md` describes the upstream merge strategy. `arcterm-ai/` and `arcterm-wasm-plugin/` directories are self-contained.
  - [Inferred] The diff surface in `wezterm-gui/src/termwindow/mod.rs` (AI pane launch) and `wezterm-gui/src/overlay/mod.rs` (overlay registration) will receive upstream changes as WezTerm evolves. Merge conflicts are likely.

- **Future merge risk from touching core GUI files**: `wezterm-gui/src/commands.rs`, `wezterm-gui/src/main.rs`, and `wezterm-gui/src/termwindow/mod.rs` contain ArcTerm-specific additions (AI pane action, agent mode, command overlay) in high-traffic upstream files.
  - Evidence: `wezterm-gui/src/termwindow/mod.rs` line 2392 (`crate::ai_pane::open_ai_pane`); `wezterm-gui/src/overlay/mod.rs` line 9 (`pub mod ai_command_overlay`).
  - Remediation: Continue the pattern of isolating logic in `arcterm-*` crates. Consider adding integration hook points in upstream files that are minimally invasive.

---

### 2. Rebrand Completeness

**Severity: Medium** (was High) -- The critical rebrand items are now addressed. Remaining issues are lower-severity cosmetic or legacy-compat items.

#### 2a. Resolved Rebrand Items [Resolved - 2026-03-19]

- **macOS quit dialog**: Now says "Quit ArcTerm?" and "Detach and close all panes and quit ArcTerm?".
  - Evidence: `window/src/os/macos/app.rs` lines 30-31.

- **macOS Objective-C class names**: Renamed to `ArcTermAppDelegate`, `ArcTermWindow`, `ArcTermWindowView`, `ArcTermNotifDelegate`.
  - Evidence: `window/src/os/macos/app.rs` line 16; `window/src/os/macos/window.rs` lines 1821-1822; `wezterm-toast-notification/src/macos.rs` line 36.

- **macOS app bundle renamed**: `assets/macos/WezTerm.app` is now `assets/macos/ArcTerm.app`.
  - Evidence: `ls /Users/lgbarn/Personal/arcterm/assets/macos/` returns `ArcTerm.app`.

- **Update checker URL corrected**: Now fetches from `lgbarn/arcterm` GitHub releases, not `wezterm/wezterm`.
  - Evidence: `wezterm-gui/src/update.rs` lines 56, 61; User-Agent changed to `arcterm/<version>` at line 42.

- **Linux `.desktop` file rebranded**: Renamed to `arcterm.desktop`, uses `com.lgbarn.arcterm` app-id.
  - Evidence: `assets/arcterm.desktop` lines 1-10.

- **AppStream/Flatpak metadata rebranded**: Files renamed to `arcterm.appdata.xml` and `com.lgbarn.arcterm.*`.
  - Evidence: `assets/arcterm.appdata.xml`; `assets/flatpak/com.lgbarn.arcterm.json`.

- **FUNDING.yml updated**: Now routes to `lgbarn` only.
  - Evidence: `.github/FUNDING.yml` line 1: `github: lgbarn`.

- **CI tag workflows repointed**: `gen_macos_tag.yml` now uploads `ArcTerm-*.zip` to `lgbarn/arcterm` releases. No references to `wez/homebrew-wezterm`, `push.fury.io/wez/`, or `wez/winget-pkgs` found in any workflow file.
  - Evidence: `.github/workflows/gen_macos_tag.yml` lines 111, 121.

- **WASM plugin infrastructure**: `arcterm-wasm-plugin` crate now exists with capability enforcement, loader, lifecycle manager, and host API registration.
  - Evidence: `arcterm-wasm-plugin/src/` (7 source files).

- **AI integration crate**: `arcterm-ai` crate now exists with Ollama and Claude backends, context extraction, suggestions, agent session, destructive detection.
  - Evidence: `arcterm-ai/src/` (10 source files).

- **Structured output (OSC 7770) removed**: `arcterm-structured-output` crate is completely absent from the workspace.
  - Evidence: Glob for `arcterm-structured-output/**` returns no files; grep for `7770` in `*.rs` and `*.toml` returns no matches.

- **Path traversal in WASM capability check**: The `..` component bypass documented in AUDIT-main.md [I1] has been fixed.
  - Evidence: `arcterm-wasm-plugin/src/capability.rs` lines 127-129: `requested.components().any(|c| matches!(c, std::path::Component::ParentDir))` — a test `test_path_traversal_blocked` covers both traversal patterns at lines 240-259.

- **`SshMultiplexing` enum default updated**: Default is now `ArcTerm`; `WezTerm` variant is retained as a legacy alias with a deprecation comment.
  - Evidence: `config/src/ssh.rs` lines 22-28.

#### 2b. Remaining Rebrand Issues

- **Windows installer publisher still "Wez Furlong"**: The `MyAppPublisher` define in the InnoSetup script was not updated; only the URL was changed.
  - Evidence: `ci/windows-installer.iss` line 7: `#define MyAppPublisher "Wez Furlong"`.

- **Windows binary `CompanyName` still "Wez Furlong"**: The VERSIONINFO resource in `build.rs` has `ProductName = "ArcTerm"` and `FileDescription = "ArcTerm - AI-Powered Terminal Emulator"` (updated), but `CompanyName` and `LegalCopyright` still attribute to Wez Furlong. These appear as Windows file properties in the binary.
  - Evidence: `wezterm-gui/build.rs` lines 121, 124.
  - [Inferred] This is likely intentional attribution rather than an oversight, but could confuse Windows users inspecting the binary.

- **Shell completion scripts still reference `org.wezfurlong.wezterm` window class**: The generated fish and zsh completions describe the `--class` flag as defaulting to `org.wezfurlong.wezterm`. This misleads users trying to configure Wayland `app_id` targeting.
  - Evidence: `assets/shell-completion/fish` line 21; `assets/shell-completion/zsh` (multiple lines).

- **Nautilus integration script still uses `org.wezfurlong.wezterm`**: The GNOME Files right-click extension identifies the app with the old class name and icon.
  - Evidence: `assets/wezterm-nautilus.py` lines 49, 60.

- **`wezterm.org` links remain in the Help menu**: The Documentation and Discussions menu entries in commands.rs still link to `wezterm.org` and `github.com/wezterm/wezterm/discussions/`.
  - Evidence: `wezterm-gui/src/commands.rs` lines 1674, 2166-2167.

- **Release notes and docs URLs are placeholder stubs**: The update checker changelog link and the AppStream/Flatpak homepage URL contain `TODO.arcterm.dev` placeholder domains that will 404 if a user clicks them.
  - Evidence: `wezterm-gui/src/update.rs` lines 92, 191; `assets/arcterm.appdata.xml` lines 30-31; `ci/create-release.sh` lines 6-13.

---

### 3. Security Concerns

**Severity: High (for prototype-stage gaps that become critical on first real plugin execution)**

#### 3a. From AUDIT-main.md — Still Open

- **[AUDIT I2] LLM response rendered as raw terminal text (escape injection)**: Tokens from the LLM backend are passed directly to `Change::Text()` without stripping ANSI escape sequences. A compromised or adversarially prompted LLM could inject terminal control sequences (window title change, clipboard write via OSC 52, alternate screen switch).
  - Evidence: `wezterm-gui/src/ai_pane.rs` line 221: `term.render(&[Change::Text(display_token)])?;` — no sanitization. `wezterm-gui/src/overlay/ai_command_overlay.rs` line 183: `result.push_str(token)` (collected and then later passed to the overlay display path without stripping).
  - Remediation: The `strip-ansi-escapes` crate is already in the workspace; apply it to all LLM token streams before rendering.

- **[AUDIT I4] Terminal scrollback sent to remote LLM without explicit user consent gate**: `open_ai_pane` and `show_command_overlay` both call `AiConfig::default()` which hardcodes the Ollama backend — safe today. However, there is no architectural gate preventing a user config (when the Lua AI config API is added) from silently switching to the Claude backend, which would transmit terminal scrollback to `https://api.anthropic.com/v1/messages` without a consent prompt.
  - Evidence: `wezterm-gui/src/ai_pane.rs` lines 25-26; `arcterm-ai/src/backend/claude.rs` lines 44-49: `ureq::post(CLAUDE_API_URL)...send_json(&body)` — no consent check.
  - Remediation: Add `ai.allow_remote_backend = false` config key; enforce in `create_backend`; add runtime prompt before first remote call.

- **[AUDIT A1] `sk-ant-test` test fixture matches Anthropic key prefix pattern**: The string `sk-ant-test` appears in four source locations and matches the `sk-ant-` prefix used by real Anthropic API keys. GitHub secret scanning and tools like truffleHog will flag this as a potential credential leak.
  - Evidence: `arcterm-ai/src/backend/claude.rs` lines 69, 88; `arcterm-ai/src/config.rs` line 55; `arcterm-ai/tests/backend_tests.rs` line 20.
  - Remediation: Replace with `"test-key-not-real"` or similar clearly synthetic value.

- **[AUDIT A2] `terminal:read` unconditionally granted to every WASM plugin**: Every plugin automatically receives `terminal:read` regardless of declared capabilities. This violates least privilege — a plugin declared with only `net:connect:api.example.com:443` can also read terminal output.
  - Evidence: `arcterm-wasm-plugin/src/capability.rs` lines 97-110: `// Ensure terminal:read is always granted`.
  - Note: This is currently safe because actual WASM callbacks are not dispatched (guest `Instance` is not stored; see §8a below). It becomes a security gap the moment instantiation is wired.
  - Remediation: Remove the unconditional default; require explicit `terminal:read` declaration.

- **[AUDIT A3] No upper bound on `memory_limit_mb` from Lua config**: A user config can set an arbitrarily large `memory_limit_mb`, potentially allowing a plugin to exhaust all available host RAM. The `checked_mul` in `loader.rs:146` catches overflow for values that overflow `usize`, but a value of `65536` (64 GB) passes through and creates a `StoreLimitsBuilder` with a 64 GB ceiling.
  - Evidence: `lua-api-crates/plugin/src/lib.rs` line 254: `let memory_limit_mb: u32 = config_table.get("memory_limit_mb").unwrap_or(64)` — no cap.
  - Remediation: Clamp to a configurable maximum (suggested: 512 MB) in the Lua registration function.

- **[AUDIT A4] Plugin name not validated for log injection**: The `name` field from Lua config is used directly in `log::info!("[plugin/{}] ...")` calls. A plugin name containing newlines, ANSI codes, or syslog control characters could corrupt log output or confuse log parsers.
  - Evidence: `arcterm-wasm-plugin/src/lifecycle.rs` line 49: `log::info!("Plugin '{}': {} → {}", self.name, ...)`; `arcterm-wasm-plugin/src/host_api.rs` lines 61, 68, 74.
  - Remediation: Validate plugin names to `[a-zA-Z0-9_-]` at registration time in `lua-api-crates/plugin/src/lib.rs`.

- **[AUDIT A5] CI deploy script echoes macOS certificate password hash and uses `eval`**: Line 59 of `deploy.sh` writes `SHA-1(MACOS_PW)` to CI build logs. Line 62 uses `eval` on keychain output, which is exploitable if the keychain name contains shell metacharacters.
  - Evidence: `ci/deploy.sh` lines 59, 62.
  - Remediation: Remove the `shasum` line; replace `eval echo $(...)` with `$(... | tr -d '"')`.

#### 3b. Inherited (from WezTerm)

- **~855 raw `unsafe` blocks across non-vendor Rust source**: Unsafe is pervasive in font rasterization (HarfBuzz FFI, FreeType FFI), PTY handling, windowing (macOS Objective-C bridge, Wayland, Win32), and the SSH session layer. This is inherited upstream code with a long test history, but any ArcTerm-added unsafe code in new crates must be reviewed carefully.
  - Evidence: `wezterm-font/src/hbwrap.rs`, `wezterm-font/src/ftwrap.rs`, `pty/src/unix.rs`, `window/src/os/macos/`, `wezterm-ssh/src/sessioninner.rs` (multiple).

- **`cargo deny` is configured but not run in CI**: The `deny.toml` file exists and is properly structured, but no CI workflow invokes `cargo deny check`.
  - Evidence: No `cargo deny` step found in any `.github/workflows/*.yml` file; `deny.toml` exists at project root.

- **No `cargo audit` or `cargo clippy` in CI**: Neither security audit nor lint checking is automated.
  - Evidence: Grep of `.github/workflows/` finds no audit or clippy steps. Only `cargo fmt` (in `fmt.yml`) and `cargo nextest run` are automated quality gates.

- **macOS entitlements are overly broad**: The entitlement plist requests Bluetooth, Camera, Audio Input, Contacts, Calendar, Location, and Photos Library access. For ArcTerm's AI use cases, outgoing network connections to Ollama and optionally Anthropic should be explicitly audited against this grant.
  - Evidence: `ci/macos-entitlement.plist`.

- **No macOS signing secrets provisioned**: Tag and continuous workflows reference `MACOS_CERT`, `MACOS_CERT_PW`, `MACOS_APPLEID`, `MACOS_APP_PW`, `MACOS_TEAM_ID`. Without these, macOS Gatekeeper will quarantine the app and the notification delegate will not function.
  - Evidence: `.github/workflows/gen_macos_tag.yml` (signing section); `wezterm-toast-notification/src/macos.rs` lines 16-17.

---

### 4. Build and CI

**Severity: Medium**

- **No `rust-toolchain.toml` to pin Rust version**: The minimum Rust version is enforced as a shell check in `ci/check-rust-version.sh` (minimum `1.71.0`) but there is no `rust-toolchain.toml`. Contributors building locally will use whatever `rustc` they have installed.
  - Evidence: `ci/check-rust-version.sh` line 11; no `rust-toolchain.toml` found in repo root.

- **CI workflows are generated from a Python script**: The `gen_*.yml` workflow files are generated by `ci/generate-workflows.py`. Manually editing `.yml` files will have changes overwritten on the next generation pass.
  - Evidence: `ci/generate-workflows.py`; all workflow files named `gen_*.yml`.
  - [Inferred] ArcTerm-specific CI changes should be made in `generate-workflows.py`.

- **`MACOSX_DEPLOYMENT_TARGET` is EOL macOS 10.12**: macOS 10.12 (Sierra, 2016) reached end-of-life in October 2019. Building with this target on modern Xcode may produce warnings or fail.
  - Evidence: `.github/workflows/gen_macos.yml` line 29; `gen_macos_tag.yml` line 16; `gen_macos_continuous.yml` line 32.

- **`sccache-action` pinned to `v0.0.9`**: The sccache GitHub Action is pinned to a very old version.
  - Evidence: `.github/workflows/gen_macos.yml` line 43.

- **Nix flake update workflow uses a PAT (`FLAKE_LOCK_GH_PAT`)**: This PAT must be provisioned in the fork's secrets for the flake lock auto-update to work.
  - Evidence: `.github/workflows/nix-update-flake.yml` line 24.

---

### 5. Technical Debt (TODO/FIXME/HACK Comments)

**Severity: Low-Medium**

#### 5a. ArcTerm-Introduced TODOs

- **WASM plugin instantiation is not wired** (High impact when addressed): `load_single_plugin` compiles the WASM component and creates the Store but discards the `_loaded` result. The guest `Instance` is never actually instantiated against the host API linker. No WASM callbacks are dispatched today. The `shutdown_all` method similarly notes it cannot call `instance.call_destroy()` yet.
  - Evidence: `arcterm-wasm-plugin/src/lifecycle.rs` lines 139-148 (`let _loaded = ...; // TODO: Store _loaded`), lines 177-179 (`// TODO: Call instance.call_destroy`).
  - Risk: When this wiring is completed, the security gaps flagged in §3a (unconditional `terminal:read` grant, plugin name log injection, memory limit) become immediately exploitable.

- **Network host API is a stub**: `http-get` and `http-post` host functions pass the capability check but return `Err("network not yet implemented")`. No actual HTTP client is wired.
  - Evidence: `arcterm-wasm-plugin/src/host_api.rs` lines 209-210, 234-235.

- **Terminal write host API is a stub**: `send-text` and `inject-output` pass the capability check but only log; no actual pane routing is wired.
  - Evidence: `arcterm-wasm-plugin/src/host_api.rs` lines 311-312, 337-338.

- **AI config is not wired to Lua user config**: Both `open_ai_pane` and `show_command_overlay` call `AiConfig::default()` directly, bypassing any user configuration. There is no path from the user's `arcterm.lua` to configure `backend`, `api_key`, or `model`.
  - Evidence: `wezterm-gui/src/ai_pane.rs` line 25; `wezterm-gui/src/overlay/ai_command_overlay.rs` line 97.
  - Risk: The Claude backend can only be activated by a code change today — but when Lua config integration is added, the missing consent gate (§3a, AUDIT I4) will become immediately reachable.

- **Release notes URLs are `TODO.arcterm.dev` placeholders**: These will 404 if clicked by any user who receives an update notification.
  - Evidence: `wezterm-gui/src/update.rs` line 92; `ci/create-release.sh` lines 6-13.

- **`SshMultiplexing::WezTerm` wiki URL is a TODO**: The deprecation comment links to a non-existent wiki page.
  - Evidence: `config/src/ssh.rs` line 26: `// See: https://github.com/lgbarn/arcterm/wiki/Configuration (TODO: update URL)`.

#### 5b. Inherited from WezTerm

- **`mux/src/termwiztermtab.rs` terminal writer is a black hole**: The terminal writer is `Vec::new()` with comment `// FIXME: connect to something?`. Terminal output from the termwiz tab type is silently discarded.
  - Evidence: `mux/src/termwiztermtab.rs` line 113.

- **`mux/src/lib.rs` clipboard and split pane pixel dimension FIXMEs**:
  - Evidence: `mux/src/lib.rs` lines 1233, 1240.

- **`wezterm-input-types/src/lib.rs` Hyper and Meta modifier keys unhandled**:
  - Evidence: Lines 1750, 1893.

- **Significant dead code suppression in font subsystem**: `#[allow(dead_code)]` and `#[allow(unused)]` attributes throughout HarfBuzz and Fontconfig FFI wrappers.
  - Evidence: `wezterm-font/src/hbwrap.rs` (16+ attributes); `wezterm-font/src/fcwrap.rs` line 248.

---

### 6. Dependency Health

**Severity: Low** -- Dependencies are generally current; workspace-level version management is used consistently.

- **`bitflags` version split**: Workspace specifies `bitflags = "1.3"` but lock file resolves both `1.3.2` and `2.10.0`.
  - Evidence: `Cargo.toml` line 51; `Cargo.lock` (both versions present).

- **`mlua` at `0.9.9`; `mlua 0.10` was released**: mlua 0.10 introduced breaking API changes. Upgrading requires testing all Lua bindings in `lua-api-crates/` and `config/src/lua/`.
  - Evidence: `Cargo.toml` (`mlua = "0.9"`); `Cargo.lock` (version "0.9.9"). [Inferred from mlua changelog knowledge.]

- **`deny.toml` wildcards policy is `allow`**: The `bans.wildcards = "allow"` setting means wildcard version constraints will not trigger warnings.
  - Evidence: `deny.toml` line 157.

- **`deny.toml` is effectively unconfigured for bans and advisories**: The `ignore`, `deny`, and `skip` lists are all empty/commented out.
  - Evidence: `deny.toml` lines 72-77, 178-184.

- **Ongoing dependency churn from upstream**: Automated bumps for `lru`, `time`, `git2`, `bytes` will continue arriving from upstream and must be merged regularly.
  - Evidence: Recent git log entries (`build(deps): bump lru`, `build(deps): bump time`, etc.).

---

### 7. Performance Concerns

**Severity: Low** -- Inherited from WezTerm with no ArcTerm-specific changes yet.

- **High `Arc`/`Mutex`/`RwLock` density in mux**: AI cross-pane context reads will require taking read locks on multiple panes simultaneously. Lock ordering must be established to avoid deadlocks.
  - Evidence: `mux/src/lib.rs`, `mux/src/localpane.rs`, `mux/src/termwiztermtab.rs` (all use `parking_lot`). ~85 instances in `mux/src/` alone.
  - [Inferred] The agent session's multi-step execution model adds a new pattern where AI pane and host pane share state across async event loop turns.

- **Synchronous HTTP in AI backends**: Both `OllamaBackend` and `ClaudeBackend` use `ureq` (synchronous blocking HTTP) for streaming responses. The streaming loop in `ai_pane.rs` runs on a dedicated TermWiz thread (acceptable), but any future move to async will require re-evaluation.
  - Evidence: `arcterm-ai/src/backend/ollama.rs` line 34 (`ureq::post`); `arcterm-ai/src/backend/claude.rs` line 44 (`ureq::post`); `wezterm-gui/src/ai_pane.rs` line 192 (`BufReader::new`).

- **No explicit performance profiling tooling in CI**: The benchmarking crate is used for text shaping but not run in CI. No regression gates exist.
  - Evidence: `wezterm-gui/src/shapecache.rs` lines 259-304.

---

### 8. Operational Gaps

**Severity: Low** -- No health checks, metrics, or structured logging added for ArcTerm-specific code.

- **No health check endpoint for Ollama connectivity**: The `is_available()` check uses a 2-second timeout `GET /api/tags`, which blocks the TermWiz thread briefly on startup. If Ollama is slow, this causes a noticeable delay opening the AI pane.
  - Evidence: `arcterm-ai/src/backend/ollama.rs` lines 42-49.

- **Destructive command detection is advisory, not a security boundary**: The `is_destructive()` function uses substring matching and is bypassable (e.g., shell aliases, obfuscation, `base64 | bash`). The UI displays a red warning without a disclaimer that it is heuristic-only.
  - Evidence: `arcterm-ai/src/destructive.rs` lines 44-48; `wezterm-gui/src/ai_pane.rs` lines 139-143.
  - Recommendation: Add "Warning is advisory only — review before running" text adjacent to the red warning.

- **No structured logging in arcterm-ai or arcterm-wasm-plugin**: Both crates use `log::info!`/`log::warn!`/`log::error!` (unstructured). Correlating plugin events with AI responses in production logs will be difficult without a request ID or plugin ID in each log line.
  - Evidence: `arcterm-wasm-plugin/src/host_api.rs` (all log calls are plain string format); `arcterm-ai/src/agent.rs` (no log calls at all).

---

## Summary Table

| Concern | Severity | Category | Status | Confidence |
|---------|----------|----------|--------|------------|
| WASM plugin instantiation not wired (callbacks never dispatch) | High | Architecture | Open | Observed |
| LLM response rendered without ANSI escape stripping | High | Security | Open | Observed |
| No consent gate before sending scrollback to remote LLM backend | High | Security | Open | Observed |
| `terminal:read` unconditionally granted to all WASM plugins | High | Security | Open | Observed |
| AI config not wired to Lua user configuration | High | Architecture | Open | Observed |
| `sk-ant-test` in test fixtures matches real key prefix pattern | Medium | Security | Open | Observed |
| No upper bound on plugin `memory_limit_mb` from Lua config | Medium | Security | Open | Observed |
| Plugin name not validated for log injection | Medium | Security | Open | Observed |
| CI deploy script echoes password hash, uses `eval` on keychain output | Medium | Security | Open | Observed |
| Network and terminal-write host API functions are stubs | Medium | Architecture | Open | Observed |
| Release notes URLs are `TODO.arcterm.dev` placeholders (will 404) | Medium | UX | Open | Observed |
| Shell completion scripts reference `org.wezfurlong.wezterm` | Low | Rebrand | Open | Observed |
| Nautilus integration script uses old `org.wezfurlong.wezterm` class | Low | Rebrand | Open | Observed |
| Help menu links to `wezterm.org` and `github.com/wezterm` | Low | Rebrand / UX | Open | Observed |
| Windows installer `MyAppPublisher` still "Wez Furlong" | Low | Rebrand | Open | Observed |
| `SshMultiplexing::WezTerm` legacy alias (wiki URL is a TODO) | Low | Rebrand | Open | Observed |
| No `cargo deny check` in CI | Medium | Security | Open | Observed |
| No `cargo audit` or `cargo clippy` in CI | Medium | Security / Quality | Open | Observed |
| No `rust-toolchain.toml` to pin Rust version | Medium | Build | Open | Observed |
| `MACOSX_DEPLOYMENT_TARGET` is EOL macOS 10.12 | Medium | Build | Open | Observed |
| CI workflows generated from Python script | Medium | CI | Open | Observed |
| macOS signing secrets not provisioned | High | CI / Build | Open | Observed |
| `deny.toml` not configured (bans/advisories lists empty) | Medium | Dependency Health | Open | Observed |
| Synchronous HTTP in AI backends (ureq on TermWiz thread) | Low | Performance | Open | Observed |
| Lock contention risk for AI cross-pane context reads | Low | Performance | Inferred |
| ~855 unsafe blocks inherited from upstream | Low | Security | Open | Observed |
| `termwiztermtab` writer is `Vec::new()` (black hole) | Low | Technical Debt | Open | Observed |
| `bitflags` dual version 1.x and 2.x in dependency graph | Low | Dependency Health | Open | Observed |
| `mlua` at 0.9.x; 0.10 available with breaking changes | Low | Dependency Health | Inferred |
| Destructive command detection advisory but UI implies certainty | Low | UX / Security | Open | Observed |
| Path traversal via `..` in WASM filesystem capability | Critical | Security | **[Resolved - 2026-03-19]** | Observed |
| 13 rebrand files uncommitted (at risk from `git restore`) | Critical | Rebrand / Git | **[Resolved - 2026-03-19]** | Observed |
| Update checker pointed at `wezterm/wezterm` releases | Critical | Rebrand / UX | **[Resolved - 2026-03-19]** | Observed |
| Tag CI workflows pushed to `wez/` repos | Critical | CI / Rebrand | **[Resolved - 2026-03-19]** | Observed |
| macOS quit dialog said "Terminate WezTerm?" | High | Rebrand / UX | **[Resolved - 2026-03-19]** | Observed |
| macOS Objective-C class names were "WezTerm*" | Medium | Rebrand | **[Resolved - 2026-03-19]** | Observed |
| macOS app bundle was `WezTerm.app` | High | Rebrand | **[Resolved - 2026-03-19]** | Observed |
| `FUNDING.yml` routed donations to upstream author | High | Rebrand | **[Resolved - 2026-03-19]** | Observed |
| No WASM plugin infrastructure existed | High | Missing Feature | **[Resolved - 2026-03-19]** | Observed |
| No AI integration crate existed | High | Missing Feature | **[Resolved - 2026-03-19]** | Observed |
| Structured output (OSC 7770) crate | N/A | Removed Feature | **[Resolved - 2026-03-19]** | Observed |

---

## Open Questions

1. **When will WASM plugin instantiation be wired?** The TODO at `arcterm-wasm-plugin/src/lifecycle.rs:147` is the gate for all plugin security concerns. The security fixes (unconditional `terminal:read`, plugin name validation, memory cap) must land before or in the same commit as instantiation wiring.

2. **Is there an Apple Developer account for ArcTerm?** Without one, macOS code signing and notarization (required for Gatekeeper) cannot be configured, and the macOS notification system will not function. This blocks any official macOS distribution.

3. **When will AI config be exposed to Lua?** The current `AiConfig::default()` hardcode is safe, but the moment Lua config integration is added, the consent gate for remote backends (AUDIT I4) must be in place simultaneously. This is a sequencing risk.

4. **Will ArcTerm publish its own documentation domain?** Resolving the `TODO.arcterm.dev` placeholders in update notifications, AppStream metadata, and release notes requires a real domain and documentation site. Until then, any user who receives an update notification and clicks the changelog link gets a 404.

5. **Should `SshMultiplexing::WezTerm` be deprecated on a timeline?** The variant is now a documented legacy alias with a deprecation comment. A sunset date and a migration guide (once the wiki URL is real) would complete this transition.

6. **Will the `ureq` synchronous HTTP model for AI backends remain long-term?** Both backends use blocking `ureq` on TermWiz-spawned threads. If AI responses need to be integrated into the main GUI event loop in the future (e.g., for inline suggestions dispatching key events), switching to async will require significant refactoring.
