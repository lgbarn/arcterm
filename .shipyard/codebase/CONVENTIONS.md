# CONVENTIONS.md

## Overview

ArcTerm is a large Rust workspace (~60 crates) descended from WezTerm. Conventions are largely inherited from that upstream project and are applied consistently across crates. The dominant patterns are: `anyhow` for error propagation, `thiserror` for typed error definitions, the `log` crate for runtime logging, a custom `wezterm-dynamic` type-erasure layer for config/Lua bridging, and `parking_lot` mutexes preferred over `std::sync::Mutex` in hot paths. Formatting is enforced via `cargo +nightly fmt` using a shared `.rustfmt.toml`.

---

## Findings

### Rust Edition and Formatting

- **Edition**: The workspace `rustfmt.toml` specifies `edition = "2018"`. All workspace members inherit this.
  - Evidence: `.rustfmt.toml` (line 3)
- **Formatting tool**: `cargo +nightly fmt --all` is run in CI on every push/PR touching `*.rs` files. The nightly toolchain is required because `imports_granularity` is a nightly-only option.
  - Evidence: `.github/workflows/fmt.yml` (lines 24–30)
- **Formatting settings** (from `.rustfmt.toml`):
  ```toml
  edition = "2018"
  imports_granularity = "Module"
  tab_spaces = 4
  ```
  `imports_granularity = "Module"` means `use` statements are grouped by crate but not split per-item. Comments in the file note that items should be kept in alphabetical order.
  - Evidence: `.rustfmt.toml`
- **Lua formatting**: A `ci/stylua.toml` is present, indicating Lua config examples in the repo are formatted with StyLua.
  - Evidence: `ci/stylua.toml`

### Error Handling Patterns

- **`anyhow` for propagation**: Essentially all public functions that can fail return `anyhow::Result<T>`. `anyhow::bail!` is used for early exits with descriptive messages. `.context("...")` is chained after `?` to add call-site context.
  - Evidence: `mux/src/domain.rs` (lines 13, 62, 84–98, 112)
  - ```rust
    let pane = self.spawn_pane(size, command, command_dir)
        .await
        .context("spawn")?;
    ```
- **`thiserror` for typed errors**: Library-boundary crates define typed error enums using `thiserror::Error`. Found in at least: `codec`, `wezterm-blob-leases`, `wezterm-cell`, `wezterm-dynamic`, `wezterm-escape-parser`, `wezterm-font`, `wezterm-ssh`, `wezterm-client`, `termwiz`, `filedescriptor`, `window`, `promise`, and `mux`.
  - Evidence: `wezterm-ssh/src/session.rs` (line 50), `codec/src/lib.rs` (line 35), `mux/src/lib.rs` (line 31)
  - ```rust
    #[derive(thiserror::Error, Debug)]
    pub struct HostVerificationFailed { ... }
    ```
- **`anyhow::anyhow!` for one-off errors**: Used inline when a structured error type is not warranted.
  - Evidence: `mux/src/domain.rs` (lines 112, 119, 319, 340)
- **Error format specifier**: Errors are formatted with `{:#}` (the alternate form) throughout the codebase, which prints the full chain of error context.
  - Evidence: `wezterm-gui/src/main.rs` (lines 326, 420, 641, 665)

### Logging Patterns

- **Crate**: The `log` crate (v0.4) is the universal logging facade. `tracing` is not used anywhere in the codebase.
  - Evidence: `Cargo.toml` (line 133: `log = "0.4"`); `tracing::` grep returned no results.
- **Log levels**: All five levels are used. Usage follows conventional semantics:
  - `log::trace!` — fine-grained internal state (config dumps, per-iteration data)
  - `log::debug!` — diagnostic info for developers
  - `log::info!` — normal operational events (SSH config selected)
  - `log::warn!` — recoverable unexpected conditions
  - `log::error!` — failures that should be surfaced to the user
  - Evidence: `mux/src/domain.rs` (lines 428, 437, 700), `mux/src/ssh.rs` (lines 227, 405, 844, 889)
- **Runtime logging infrastructure**: `env-bootstrap` crate provides a custom in-memory ring-buffer logger layered on top of `env_logger`. Log entries (timestamp, level, target, message) are held in a fixed-size ring for display in the UI.
  - Evidence: `env-bootstrap/src/ringlog.rs`
- **Test logging initialization**: All test modules that need logging call `env_logger::Builder::new().is_test(true).filter_level(log::LevelFilter::Trace).try_init()` at the start of the test helper constructor. The `try_init` (not `init`) avoids panics when multiple tests share a process.
  - Evidence: `term/src/test/mod.rs` (lines 61–64), `config/src/lua.rs` (lines 872–875), `wezterm-ssh/tests/sshd.rs` (line 258)

### Naming Conventions

- **Crate names**: `snake_case` with `wezterm-` prefix for library crates (e.g., `wezterm-cell`, `wezterm-font`). ArcTerm-specific extensions are planned under an `arcterm-` prefix (e.g., `arcterm-wasm-plugin`, `arcterm-ai`) — not yet present in the codebase.
  - Evidence: `Cargo.toml` workspace members
- **Module names**: `snake_case`, matching file names (standard Rust).
- **Type names**: `PascalCase` for all types. Enums, structs, and traits follow this consistently.
  - Evidence: `mux/src/domain.rs` (`DomainState`, `SplitSource`), `mux/src/tmux_commands.rs` (`PaneItem`, `WindowItem`)
- **Function names**: `snake_case`.
- **Constants and statics**: `SCREAMING_SNAKE_CASE` for global statics.
  - Evidence: `mux/src/domain.rs` (line 27: `static DOMAIN_ID`), `wezterm-gui/src/main.rs` (line 63: `static ALLOC`)
- **Trait method naming**: Getter methods use `get_` prefix (e.g., `get_cursor_position`, `get_lines`, `get_metadata`). Boolean-returning predicates occasionally use `is_` (e.g., `is_file`).
  - Evidence: `mux/src/pane.rs` (lines 168–200)
- **ID types**: Plain `usize` type aliases with a `Id` suffix (e.g., `DomainId`, `PaneId`, `TabId`, `WindowId`). Allocated via module-level atomic counters with `alloc_*_id()` functions.
  - Evidence: `mux/src/domain.rs` (lines 27–38), `mux/src/pane.rs`

### Configuration Patterns

- **Config struct**: The root `Config` struct in `config/src/config.rs` is a single large struct with all configuration fields. It derives `FromDynamic`, `ToDynamic`, and `ConfigMeta` (a custom proc-macro from `wezterm-config-derive`).
  - Evidence: `config/src/config.rs` (lines 51–100)
- **`#[dynamic(...)]` attribute**: Fields on `Config` use `#[dynamic(default = "fn_name")]`, `#[dynamic(validate = "fn_name")]`, and `#[dynamic(try_from = "SomeType")]` attributes to control deserialization behavior and provide defaults.
  - Evidence: `config/src/config.rs` (lines 54–98)
- **`wezterm-dynamic` type bridge**: The project uses a bespoke intermediate value type (`wezterm_dynamic::Value`) that bridges between Lua values, TOML-deserialized values, and Rust types. Types that participate implement `FromDynamic` and `ToDynamic`. This avoids coupling directly to `serde`.
  - Evidence: `Cargo.toml` (line 249: `wezterm-dynamic`), `config/src/config.rs` (line 44), `luahelper/src/lib.rs`
- **Config access**: The config is retrieved via a `configuration()` free function that returns a `ConfigHandle` (an `Arc`-backed snapshot). Code across the codebase calls `config::configuration()` to get the current config.
  - Evidence: `wezterm-gui/src/update.rs` (line 72: `configuration().check_for_updates`)
- **Lua config file loading**: `config/src/lua.rs::make_lua_context` creates a new `mlua::Lua` instance, registers the `wezterm` module, patches `package.searchers` to add file watching, then loads the user's config file. All Lua API modules register themselves by calling `get_or_create_module(lua, "wezterm")` and setting functions on it.
  - Evidence: `config/src/lua.rs` (lines 211–267)

### Lua Binding Patterns

- **Module registration**: Each `lua-api-crates/*` crate exposes a `pub fn register(lua: &Lua) -> anyhow::Result<()>` function. These are called from `config/src/lua.rs::make_lua_context`. Functions are registered via `lua.create_function(...)` and set on the `wezterm` module table.
  - Evidence: `lua-api-crates/termwiz-funcs/src/lib.rs` (lines 14–51)
  - ```rust
    pub fn register(lua: &Lua) -> anyhow::Result<()> {
        let wezterm_mod = get_or_create_module(lua, "wezterm")?;
        wezterm_mod.set("format", lua.create_function(format)?)?;
        ...
    }
    ```
- **`impl_lua_conversion_dynamic!` macro**: Types that implement `FromDynamic + ToDynamic` use this macro (defined in `luahelper`) to automatically derive `mlua::IntoLua` and `mlua::FromLua` implementations via the intermediate `DynValue` layer. This is the standard way to make a type usable as a Lua function parameter or return value.
  - Evidence: `luahelper/src/lib.rs` (lines 41–61), `lua-api-crates/termwiz-funcs/src/lib.rs` (line 4: `use luahelper::impl_lua_conversion_dynamic`)
- **`mlua::UserData` for opaque types**: Types that should be Lua objects (not plain tables) implement `mlua::UserData` and expose methods via `add_methods`. Used for `NerdFonts` in `termwiz-funcs` and similar wrappers.
  - Evidence: `lua-api-crates/termwiz-funcs/src/lib.rs` (lines 53–66)
- **Sub-modules**: Some APIs are grouped under `wezterm.<sub>` tables using `get_or_create_sub_module`.
  - Evidence: `config/src/lua.rs` (lines 55–73)

### Trait Patterns

- **Core domain traits**: `Domain`, `Pane`, `WindowOps`, `ConnectionOps` are the main abstraction points. They are `dyn`-compatible and stored as `Arc<dyn Trait>`.
  - Evidence: `mux/src/domain.rs` (line 50), `mux/src/pane.rs` (line 167), `window/src/lib.rs` (line 251)
- **`async_trait`**: Async methods on traits use the `#[async_trait(?Send)]` form (note `?Send` — the executor is single-threaded per-crate). `async-trait` 0.1 is the workspace dependency.
  - Evidence: `mux/src/domain.rs` (line 49), `mux/src/termwiztermtab.rs` (line 49)
- **`downcast-rs`**: Traits that need runtime downcasting (e.g., `Domain`, `Pane`, `Texture2d`) bound on `Downcast` and use `impl_downcast!(TraitName)`.
  - Evidence: `mux/src/domain.rs` (lines 17, 50, 200), `window/src/bitmaps/mod.rs` (line 16)
- **Visitor/callback traits**: Complex iteration is handled via single-method callback traits (e.g., `WithPaneLines`, `ForEachPaneLogicalLine`) passed as `&mut dyn Trait` parameters. This avoids `dyn Fn` boxes and allows returning references.
  - Evidence: `mux/src/pane.rs` (lines 352–370)

### Derive Macro Patterns

- **Config types** consistently derive `Debug, Clone, FromDynamic, ToDynamic`, often with `ConfigMeta`.
  - Evidence: `config/src/config.rs` (line 51)
- **Data structures** derive `Debug, Clone, PartialEq, Eq` or subsets depending on whether comparison is needed.
  - Evidence: `mux/src/domain.rs` (lines 30, 40)
- **Command/message types** derive `Debug, Clone`.
  - Evidence: `wezterm-gui/src/commands.rs` (lines 18, 71)
- **Wire types** (codec, SSH) derive `Debug, Serialize, Deserialize, Clone`.
  - Evidence: `wezterm-gui/src/update.rs` (lines 19, 28)
- **Enum ordering in derives**: The order generally follows `Debug, Clone, Copy, PartialEq, Eq, Default` — with `Copy` included when the type is small and value semantics are natural.

### Unsafe Code Patterns

Unsafe code exists but is localized to specific integration points:

- **GPU/window handle creation**: `unsafe` required to construct raw window/display handles for `wgpu` surface creation and for calling Win32 `GetClientRect`.
  - Evidence: `wezterm-gui/src/termwindow/webgpu.rs` (lines 56, 62, 231, 539)
- **Pixel buffer transmutation**: `std::slice::from_raw_parts` used to reinterpret vertex float arrays as bytes for GPU upload.
  - Evidence: `wezterm-gui/src/quad.rs` (line 369)
- **Lifetime extension**: A private `unsafe trait ExtendStatic` with `unsafe impl` blocks is used in `renderstate.rs` to unsafely extend lifetimes of `RefCell` borrows. This is a documented workaround and is not a general pattern.
  - Evidence: `wezterm-gui/src/renderstate.rs` (lines 341–353)
- **`libc::getpid()`**: Called without safety concerns since `getpid` is always safe, but still wrapped in `unsafe {}` as required by the FFI binding.
  - Evidence: `wezterm-gui/src/main.rs` (lines 414, 1173), `wezterm-gui/src/scripting/guiwin.rs` (line 40)
- **No `#![deny(unsafe_code)]`**: There is no workspace-level unsafe denial. Unsafe blocks appear without explicit justification comments in most cases.

### Synchronization Conventions

- **`parking_lot::Mutex`** is preferred over `std::sync::Mutex` in the multiplexer and domain code. `std::sync::Mutex` appears mainly in older utilities and the connui module.
  - Evidence: `mux/src/domain.rs` (line 18), `mux/src/termwiztermtab.rs` (line 20)
- **`Arc<Mutex<T>>`** is the standard shared-mutable-state pattern.
- **`lazy_static!`** macro is used for global singletons. The newer `std::sync::LazyLock` appears in some newer code (`wezterm-toast-notification`, `wezterm-ssh/tests`).
  - Evidence: `mux/src/lib.rs` (line 366), `wezterm-toast-notification/src/macos.rs` (lines 96–97)

### Documentation Style

- **Module-level docs**: Key modules use `//!` (inner doc comments) to explain the purpose and design rationale of the module. These are often multi-paragraph prose, not just one-liners.
  - Evidence: `termwiz/src/caps/mod.rs` (lines 1–57), `mux/src/domain.rs` (lines 1–6), `mux/src/termwiztermtab.rs` (lines 1–4)
- **Item-level docs**: Public trait methods and struct fields are documented with `///` comments. Private or self-explanatory items are often undocumented.
  - Evidence: `mux/src/domain.rs` (lines 51, 151–196)
- **URL references**: External specifications are linked in doc comments with `<https://...>` syntax.
  - Evidence: `termwiz/src/caps/mod.rs` (line 66: `NO_COLOR` spec), `config/src/lua.rs` (line 260)
- **Clippy suppressions**: `#[allow(clippy::...)]` is used sparingly and at the smallest applicable scope (file-level `#![allow]` only for pervasive issues like `range_plus_one`). No blanket `allow(clippy::all)` exists.
  - Evidence: `term/src/screen.rs` (line 1), `wezterm-gui/src/quad.rs` (line 3), `wezterm-gui/src/glyphcache.rs` (line 701)

---

## Summary Table

| Item | Detail | Confidence |
|------|--------|------------|
| Rust edition | 2018 (set in `.rustfmt.toml`) | Observed |
| Formatter | `cargo +nightly fmt`, `imports_granularity = "Module"`, 4-space indent | Observed |
| Error propagation | `anyhow::Result<T>` with `.context(...)` chains | Observed |
| Typed errors | `thiserror::Error` derive at library boundaries | Observed |
| Logging | `log` crate (v0.4); all 5 levels used; custom ring-buffer appender in `env-bootstrap` | Observed |
| Config bridge | Custom `wezterm-dynamic` DynValue layer; `FromDynamic`/`ToDynamic` traits | Observed |
| Lua bindings | `mlua`; `impl_lua_conversion_dynamic!` macro; `register(lua)` entry point per crate | Observed |
| Trait dispatch | `Arc<dyn Trait>` with `downcast-rs` for runtime downcasting | Observed |
| Async traits | `async_trait(?Send)` — single-threaded executor model | Observed |
| Synchronization | `parking_lot::Mutex` preferred; `lazy_static!` for globals (some `LazyLock` migration) | Observed |
| Unsafe code | Localized to GPU buffer handling, Win32 FFI, and one lifetime-extension workaround | Observed |
| Clippy suppressions | Targeted, per-item; no blanket suppressions | Observed |
| Cargo deny | `deny.toml` present; license allowlist enforced; multiple-versions = warn | Observed |

## Open Questions

- There is no `clippy.toml` or `[workspace.metadata.clippy]` configuration. [Inferred] The project relies on default clippy lints plus the scattered `#[allow]` suppressions rather than a curated lint profile.
- The `wezterm-dynamic` / `FromDynamic` / `ToDynamic` system is central but underdocumented. Understanding how it diverges from `serde` in capability or intent requires deeper reading of `wezterm-dynamic/src/`.
- The `config/derive/` and `wezterm-dynamic/derive/` proc-macro crates generate significant behavior (`ConfigMeta`, `FromDynamic`, `ToDynamic`). These are not sampled in this analysis — the full attribute surface is not documented here.
