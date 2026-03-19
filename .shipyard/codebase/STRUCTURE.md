# STRUCTURE.md

## Overview

ArcTerm is organized as a single Cargo workspace with approximately 45 crates at the repository root. The crates span four concerns: binaries (user-facing CLI and GUI), core terminal logic, multiplexer infrastructure, and utility/support libraries. ArcTerm-specific divergence from upstream WezTerm today is limited to string rebranding; no new crates exist yet.

---

## Findings

### Workspace Organization

The workspace is declared in `Cargo.toml` (root). Members include the following (non-exhaustive; all entries verified from the `members` array):

| Crate (directory) | Package name | Role |
|---|---|---|
| `wezterm-gui/` | `wezterm-gui` | Main GUI binary. Window creation, rendering, key/mouse handling. |
| `wezterm/` | `wezterm` | CLI binary. Delegates GUI subcommands to `wezterm-gui` via exec. |
| `wezterm-mux-server/` | `wezterm-mux-server` | Standalone headless mux server binary. |
| `mux/` | `mux` | Multiplexer library: Pane, Tab, Window, Domain abstractions. |
| `term/` | `wezterm-term` | Pure terminal VT state machine. No GUI dependency. |
| `pty/` | `portable-pty` | Cross-platform PTY allocation (Unix `openpty` / Windows ConPTY). |
| `config/` | `config` | Configuration loading, Lua context, key assignment types. |
| `termwiz/` | `termwiz` | Terminal capabilities, escape sequence parser, surface model. |
| `vtparse/` | `vtparse` | Low-level VT state machine parser (used by termwiz). |
| `window/` | `window` | Cross-platform windowing: event loop, OpenGL/EGL, input. |
| `wezterm-font/` | `wezterm-font` | Font loading, shaping (HarfBuzz), rasterization (FreeType). |
| `wezterm-ssh/` | `wezterm-ssh` | SSH session management (libssh2 or libssh-rs backends). |
| `wezterm-client/` | `wezterm-client` | Client-side mux protocol: connects to running GUI instance. |
| `wezterm-mux-server-impl/` | *(lib only)* | Server-side mux session handler, PDU dispatch. |
| `codec/` | `codec` | LEB128-framed PDU codec for the multiplexer wire protocol. |
| `wezterm-surface/` | `wezterm-surface` | Surface/line model shared between `term` and `termwiz`. |
| `wezterm-cell/` | `wezterm-cell` | Cell attribute types (color, text style, decoration). |
| `wezterm-input-types/` | `wezterm-input-types` | Keyboard and mouse event types. |
| `wezterm-escape-parser/` | `wezterm-escape-parser` | Escape sequence parser wrapper crate. |
| `wezterm-dynamic/` | `wezterm-dynamic` | Dynamic value type for Lua ↔ Rust config bridging. |
| `promise/` | `promise` | Future/Promise primitives + custom GUI-aware spawn scheduler. |
| `wezterm-blob-leases/` | `wezterm-blob-leases` | Temporary blob storage for image data transfers. |
| `wezterm-gui-subcommands/` | `wezterm-gui-subcommands` | Shared CLI subcommand types between `wezterm` and `wezterm-gui`. |
| `wezterm-open-url/` | `wezterm-open-url` | Platform URL opener. |
| `wezterm-toast-notification/` | `wezterm-toast-notification` | OS toast/notification API. |
| `wezterm-uds/` | `wezterm-uds` | Unix domain socket listener. |
| `wezterm-version/` | `wezterm-version` | Build version string helper. |
| `lua-api-crates/` | *(multiple)* | Lua API extension crates (see below). |
| `bidi/` | `wezterm-bidi` | Bidirectional text algorithm. |
| `procinfo/` | `procinfo` | Process info (foreground process, CWD). |
| `filedescriptor/` | `filedescriptor` | Cross-platform file descriptor utilities. |
| `env-bootstrap/` | `env-bootstrap` | Environment variable setup on startup. |
| `luahelper/` | `luahelper` | Lua-Rust conversion helpers. |
| `frecency/` | `frecency` | Frecency ranking (used for launcher). |
| `strip-ansi-escapes/` | `strip-ansi-escapes` | Strip ANSI escapes from strings. |
| `color-types/` | `wezterm-color-types` | Color type definitions. |
| `rangeset/` | `rangeset` | Range set data structure. |
| `ratelim/` | `ratelim` | Rate limiting. |
| `lfucache/` | `lfucache` | LFU cache. |

**Crates excluded from the workspace** (in `exclude`):
- `termwiz/codegen`
- `wezterm-char-props/codegen`

Evidence: `Cargo.toml` lines 1-24.

### Lua API Extension Crates (`lua-api-crates/`)

These crates expose Rust functionality to the Lua config/scripting layer. Each registers itself via `config::lua::add_context_setup_func`.

| Subcrate | Exposed as |
|---|---|
| `battery/` | `wezterm.battery` |
| `color-funcs/` | Color functions |
| `filesystem/` | `wezterm.filesystem` |
| `logging/` | `wezterm.log` |
| `mux/` | `wezterm.mux` |
| `plugin/` | `wezterm.plugin` — downloads/loads Lua plugins from GitHub repos into `DATA_DIR/plugins/` |
| `procinfo-funcs/` | `wezterm.procinfo` |
| `serde-funcs/` | JSON/YAML helpers |
| `share-data/` | Shared mutable Lua state |
| `spawn-funcs/` | Spawn helpers |
| `ssh-funcs/` | SSH config inspection |
| `termwiz-funcs/` | Termwiz surface helpers |
| `time-funcs/` | Time helpers |
| `url-funcs/` | URL parsing |
| `window-funcs/` | Window scripting API |

Evidence: `lua-api-crates/` directory listing; `lua-api-crates/plugin/src/lib.rs` lines 95-96, 158-160.

### Binary Entry Points

| Binary | Source | Purpose |
|---|---|---|
| `wezterm-gui` | `wezterm-gui/src/main.rs` | Primary GUI process. Parses CLI, initializes mux, starts event loop. |
| `wezterm` | `wezterm/src/main.rs` | Thin CLI wrapper. GUI subcommands (`start`, `ssh`, etc.) are delegated via `exec` to `wezterm-gui`. |
| `wezterm-mux-server` | `wezterm-mux-server/` | Headless multiplexer server (no GUI). |

- The `wezterm` binary's `delegate_to_gui` function resolves `wezterm-gui` from the same directory as the running executable and `exec`s it.
  - Evidence: `wezterm/src/main.rs` lines 768-813
- The `wezterm-gui` binary's `main()` calls `run()` → `run_terminal_gui()` → `build_initial_mux()` → `GuiFrontEnd::try_new()` → `gui.run_forever()`.
  - Evidence: `wezterm-gui/src/main.rs` lines 829-841, 720-791

### Module Dependency Graph (Simplified)

```
wezterm-gui ──► mux ──► term (wezterm-term)
            │       └──► pty (portable-pty)
            │       └──► wezterm-ssh
            │       └──► config
            │       └──► termwiz
            ├──► window ──► wezterm-input-types
            ├──► wezterm-font
            ├──► wezterm-client ──► codec ──► mux
            ├──► wezterm-mux-server-impl ──► codec
            ├──► config ──► wezterm-dynamic
            │           └──► luahelper ──► mlua
            ├──► promise (no GUI deps; pure async scheduler)
            └──► lua-api-crates/* ──► config, mux, window
```

Key observations:
- `promise` has no dependency on `window` or GUI; it only provides the scheduling abstraction.
- `codec` depends on `mux` (for type definitions) but not on `wezterm-gui`.
- `term` has no dependency on `mux`, `window`, or `wezterm-gui` — it is a pure state machine.
- `config` depends on `mlua` (Lua runtime) and `wezterm-dynamic` but not on `mux` or `window`.

Evidence: Cross-checked via `Cargo.toml` `[dependencies]` sections in `wezterm-gui/Cargo.toml`, `mux/Cargo.toml`, `term/Cargo.toml`.

### Configuration Loading Path

1. **CLI parsing**: `wezterm-gui/src/main.rs` → `Opt::parse()` extracts `--config-file`, `--config name=value`, `--skip-config` flags.
2. **`config::common_init`** (`config/src/lib.rs` line 344): stores the override path and any `--config` overrides in global statics, then triggers a config reload.
3. **`Config::load_with_overrides`** (`config/src/config.rs` line 1003): searches for the config file in priority order:
   - (Windows only) `<exe_dir>/wezterm.lua`
   - `$WEZTERM_CONFIG_FILE` env var
   - `--config-file` CLI override
   - `~/.wezterm.lua`
   - `$XDG_CONFIG_HOME/wezterm/wezterm.lua` (and other `CONFIG_DIRS` entries)
   - Falls back to built-in defaults if none found.
   - Evidence: `config/src/config.rs` lines 1009-1056
4. **Lua execution**: `config::lua::make_lua_context` (`config/src/lua.rs` line 211) creates an `mlua::Lua` instance, sets up `package.path` to include `CONFIG_DIRS`, and runs all registered `add_context_setup_func` callbacks (including `window-funcs::register` and `crate::scripting::register`).
   - Evidence: `wezterm-gui/src/main.rs` lines 1205-1207
5. **Live reload**: `config::subscribe_to_config_reload` registers callbacks; `notify` crate watches the config file for changes. [Inferred] Reloads are triggered from a background thread and scheduled onto the main thread via `promise::spawn_into_main_thread`.

**Note**: The CLI `--skip-config` help text in `wezterm-gui/src/main.rs` (line 75) says "Skip loading arcterm.lua (or wezterm.lua for backwards compatibility)", but the actual file search in `config/src/config.rs` still only looks for `wezterm.lua`. The `arcterm.lua` name is documented but not yet implemented in the config resolution code.
- Evidence: `config/src/config.rs` lines 1009-1011 (only `wezterm.lua` referenced)

### Plugin Loading Path

The existing Lua plugin system (not the planned WASM system) works as follows:

1. User calls `wezterm.plugin.require("https://github.com/owner/repo")` in their config.
2. `lua-api-crates/plugin/src/lib.rs::require_plugin` (line 187) downloads the repo as a tarball.
3. Extracted into `$DATA_DIR/plugins/<component>/` directory.
4. Plugin's `plugin.lua` is loaded via the Lua `require` mechanism.
   - Evidence: `lua-api-crates/plugin/src/lib.rs` lines 62-100, 158-187

### Test Organization

| Location | Type | Coverage |
|---|---|---|
| `term/src/test/` | Unit tests — VT state machine | Extensive: `c0.rs`, `c1.rs`, `csi.rs`, `keyboard.rs`, `kitty.rs`, `iterm.rs`, `sixel.rs`, `mouse.rs`, `image.rs`, `selection.rs` |
| `wezterm-dynamic/tests/` | Unit tests — dynamic value serde | `fromdynamic.rs`, `todynamic.rs` |
| `bidi/tests/` | Conformance tests — bidi algorithm | `conformance.rs` |
| `wezterm-ssh/tests/` | Integration / e2e tests — SSH | Requires a running sshd; `e2e/sftp/`, `e2e/agent_forward.rs` |
| Inline `#[test]` | Scattered throughout crates | Common in `codec/`, `config/`, `rangeset/` |

- The main command to run all tests is `cargo test --all` (from `CLAUDE.md`).
- No CI configuration file was found in the repository at time of analysis, so CI test execution cannot be verified.
- Evidence: `find` output locating test files; `CLAUDE.md` build instructions.

### Shared / Common Code Locations

| Purpose | Location |
|---|---|
| Config types and Lua bridge | `config/src/` |
| Mux notification bus | `mux/src/lib.rs` (`MuxNotification` enum, `Mux::subscribe`) |
| Terminal cell model | `wezterm-cell/src/` + `wezterm-surface/src/` |
| Input event types | `wezterm-input-types/src/` |
| Dynamic Lua ↔ Rust values | `wezterm-dynamic/src/` |
| Async spawn primitives | `promise/src/spawn.rs` |
| Cross-platform file descriptors | `filedescriptor/src/` |

---

## Summary Table

| Item | Detail | Confidence |
|------|--------|------------|
| Total top-level crate directories | ~45 | Observed |
| Workspace members (declared) | 18 (top-level) + lua-api-crates not all listed | Observed |
| GUI binary entry point | `wezterm-gui/src/main.rs::main` | Observed |
| CLI binary entry point | `wezterm/src/main.rs::main` (delegates to GUI) | Observed |
| Config file searched | `wezterm.lua` (not `arcterm.lua` yet) | Observed |
| Plugin directory | `$DATA_DIR/plugins/` | Observed |
| Test framework | Rust built-in `#[test]` + `rstest` (workspace dep) | Observed |
| SSH test requires external sshd | Yes | Observed |
| ArcTerm-specific crates | None yet; all code is in upstream crate names | Observed |
| `arcterm.lua` config name | Documented in CLI help but not in search path | Observed |

## Open Questions

- The workspace `members` array in `Cargo.toml` does not include all of the `lua-api-crates/*` subcrates individually — they may be referenced only as path dependencies from other crates. Their exact workspace membership status needs verification via `cargo metadata`.
- No `.github/workflows/` or `ci/` directory was found containing CI pipeline definitions, making it impossible to confirm how tests are run in CI.
- `wezterm-char-props/` is present at the root but its `codegen` subcrate is excluded from the workspace; the relationship and build order are not fully traced.
- The `docs/` and `docs/plans/` directories appeared in `git status` as untracked — their content may contain planned architecture details that supersede inferences made here.
