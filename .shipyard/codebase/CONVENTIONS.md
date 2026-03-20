# CONVENTIONS.md

## Overview

ArcTerm is a large Rust workspace (~60 crates) descended from WezTerm. Conventions are largely inherited from that upstream project and are applied consistently across crates. The dominant patterns are: `anyhow` for error propagation, `thiserror` for typed error definitions, the `log` crate for runtime logging, a custom `wezterm-dynamic` type-erasure layer for config/Lua bridging, and `parking_lot` mutexes preferred over `std::sync::Mutex` in hot paths. Formatting is enforced via `cargo +nightly fmt` using a shared `.rustfmt.toml`. The two ArcTerm-specific crates (`arcterm-ai`, `arcterm-wasm-plugin`) follow all inherited conventions faithfully and introduce a small set of new patterns: synchronous streaming HTTP via `ureq`, WASM Component Model integration via `wasmtime`, and `tempfile` for test fixture creation.

---

## Findings

### Rust Edition and Formatting

- **Edition**: The workspace `rustfmt.toml` specifies `edition = "2018"`. The two ArcTerm-specific crates use `edition = "2021"` in their own `Cargo.toml` manifests — this is not a formatting discrepancy (rustfmt uses the workspace `.rustfmt.toml`), but it is worth noting the mismatch.
  - Evidence: `.rustfmt.toml` (line 3: `edition = "2018"`), `arcterm-ai/Cargo.toml` (line 4: `edition = "2021"`), `arcterm-wasm-plugin/Cargo.toml` (line 4: `edition = "2021"`)
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

- **`anyhow` for propagation**: Essentially all public functions that can fail return `anyhow::Result<T>`. `anyhow::bail!` is used for early exits with descriptive messages. `.context("...")` is chained after `?` to add call-site context. Both ArcTerm crates follow this faithfully.
  - Evidence: `mux/src/domain.rs` (lines 13, 62, 84–98, 112)
  - Evidence: `arcterm-wasm-plugin/src/loader.rs` (lines 107, 137, 148: `.with_context(|| format!(...))`)
  - Evidence: `arcterm-wasm-plugin/src/capability.rs` (line 44: `anyhow::bail!(...)`)
  - ```rust
    let pane = self.spawn_pane(size, command, command_dir)
        .await
        .context("spawn")?;
    ```
- **`thiserror` for typed errors**: Library-boundary crates define typed error enums using `thiserror::Error`. Found in at least: `codec`, `wezterm-blob-leases`, `wezterm-cell`, `wezterm-dynamic`, `wezterm-escape-parser`, `wezterm-font`, `wezterm-ssh`, `wezterm-client`, `termwiz`, `filedescriptor`, `window`, `promise`, `mux`, and `arcterm-wasm-plugin`.
  - Evidence: `wezterm-ssh/src/session.rs` (line 50), `codec/src/lib.rs` (line 35)
  - Evidence: `arcterm-wasm-plugin/src/capability.rs` (lines 32–36):
    ```rust
    #[derive(Debug, thiserror::Error)]
    #[error("Plugin denied capability {resource}:{operation} — not granted")]
    pub struct CapabilityDenied { ... }
    ```
- **`anyhow::anyhow!` for one-off errors**: Used inline when a structured error type is not warranted.
  - Evidence: `mux/src/domain.rs` (lines 112, 119, 319, 340)
  - Evidence: `arcterm-ai/src/backend/ollama.rs` (line 37: `anyhow::anyhow!("Ollama request failed: {}", e)`)
- **Error format specifier**: Errors are formatted with `{:#}` (the alternate form) throughout the codebase, which prints the full chain of error context. The ArcTerm crates follow this.
  - Evidence: `wezterm-gui/src/main.rs` (lines 326, 420, 641, 665)
  - Evidence: `arcterm-wasm-plugin/src/lifecycle.rs` (line 105: `format!("{:#}", e)`)
  - Evidence: `arcterm-wasm-plugin/src/loader.rs` (line 246: `format!("{:#}", result.err().unwrap())` in test)

### Logging Patterns

- **Crate**: The `log` crate (v0.4) is the universal logging facade. `tracing` is not used anywhere in the codebase. Both ArcTerm crates use `log` exclusively.
  - Evidence: `Cargo.toml` (line 133: `log = "0.4"`); `arcterm-ai/Cargo.toml` (line 6: `log = { workspace = true }`); `arcterm-wasm-plugin/Cargo.toml` (line 11: `log = { workspace = true }`)
- **Log levels**: All five levels are used conventionally. The ArcTerm crates follow the same semantics:
  - `log::trace!` — fine-grained internal state
  - `log::debug!` — diagnostic info for developers
  - `log::info!` — normal operational events (plugin load completions)
  - `log::warn!` — recoverable unexpected conditions (invalid capability strings, path traversal attempts)
  - `log::error!` — failures that should be surfaced to the user (mutex poisoning, engine creation failure)
  - Evidence: `arcterm-wasm-plugin/src/capability.rs` (line 130: `log::warn!("Capability denied: path traversal...")`), `arcterm-wasm-plugin/src/lifecycle.rs` (lines 85, 103, 118), `arcterm-wasm-plugin/src/config.rs` (line 65: `log::error!(...)`)
- **Plugin log prefixing**: In `arcterm-wasm-plugin`, log messages from the host API always include the plugin name via `[plugin/<name>]` prefix to distinguish between multiple loaded plugins.
  - Evidence: `arcterm-wasm-plugin/src/host_api.rs` (line 61: `log::info!("[plugin/{}] {}", ctx.data().name, msg)`)
- **Runtime logging infrastructure**: `env-bootstrap` crate provides a custom in-memory ring-buffer logger layered on top of `env_logger`. Log entries (timestamp, level, target, message) are held in a fixed-size ring for display in the UI.
  - Evidence: `env-bootstrap/src/ringlog.rs`
- **Test logging initialization**: All test modules that need logging call `env_logger::Builder::new().is_test(true).filter_level(log::LevelFilter::Trace).try_init()` at the start of the test helper constructor. The `try_init` (not `init`) avoids panics when multiple tests share a process.
  - Evidence: `term/src/test/mod.rs` (lines 61–64), `config/src/lua.rs` (lines 872–875), `wezterm-ssh/tests/sshd.rs` (line 258)
  - Note: The ArcTerm crates do not call `try_init` in their test modules — they rely on the default test harness behavior without explicit log setup. This is consistent with tests that only assert on return values, not log output.

### Naming Conventions

- **Crate names**: `snake_case` with `wezterm-` prefix for library crates (e.g., `wezterm-cell`, `wezterm-font`). ArcTerm-specific extensions use an `arcterm-` prefix (`arcterm-ai`, `arcterm-wasm-plugin`). The `arcterm-structured-output` crate has been removed from this branch.
  - Evidence: `Cargo.toml` workspace members (lines 3–4: `"arcterm-ai"`, `"arcterm-wasm-plugin"`)
- **Module names**: `snake_case`, matching file names (standard Rust).
- **Type names**: `PascalCase` for all types. Enums, structs, and traits follow this consistently across all crates.
  - Evidence: `mux/src/domain.rs` (`DomainState`, `SplitSource`), `arcterm-ai/src/agent.rs` (`AgentStep`, `AgentState`, `StepStatus`), `arcterm-wasm-plugin/src/capability.rs` (`CapabilityResource`, `CapabilitySet`)
- **Function names**: `snake_case`.
- **Constants and statics**: `SCREAMING_SNAKE_CASE` for global statics and module-level constants.
  - Evidence: `mux/src/domain.rs` (line 27: `static DOMAIN_ID`), `arcterm-ai/src/backend/claude.rs` (lines 6–7: `CLAUDE_API_URL`, `ANTHROPIC_VERSION`), `arcterm-ai/src/destructive.rs` (lines 4, 7: `WARNING_LABEL`, `DESTRUCTIVE_PATTERNS`)
- **Trait method naming**: Getter methods use `get_` prefix (e.g., `get_cursor_position`, `get_lines`, `get_metadata`). Boolean-returning predicates use `is_` (e.g., `is_file`, `is_running`, `is_finished`, `is_destructive`).
  - Evidence: `mux/src/pane.rs` (lines 168–200), `arcterm-ai/src/agent.rs` (lines 154: `is_finished`), `arcterm-wasm-plugin/src/lifecycle.rs` (line 61: `is_running`)
- **ID types**: Plain `usize` type aliases with a `Id` suffix (e.g., `DomainId`, `PaneId`, `TabId`, `WindowId`). Allocated via module-level atomic counters with `alloc_*_id()` functions.
  - Evidence: `mux/src/domain.rs` (lines 27–38), `mux/src/pane.rs`

### Derive Macro Patterns

- **Config types** consistently derive `Debug, Clone, FromDynamic, ToDynamic`, often with `ConfigMeta`. This applies to the upstream crates. The ArcTerm config types use a simpler subset.
  - Evidence: `config/src/config.rs` (line 51)
- **ArcTerm config types** derive `Debug, Clone` only — they do not implement `FromDynamic`/`ToDynamic` because they are not yet wired into the Lua config bridge. `Default` is implemented manually (not derived) to allow setting meaningful non-zero defaults.
  - Evidence: `arcterm-ai/src/config.rs` (lines 4, 11: `#[derive(Debug, Clone, PartialEq, Eq)]`, `#[derive(Debug, Clone)]`); `impl Default for AiConfig` (line 25)
  - Evidence: `arcterm-ai/src/suggestions.rs` (line 17: `#[derive(Debug, Clone)]`; `impl Default for SuggestionConfig` at line 29)
- **Data structures** derive `Debug, Clone, PartialEq, Eq` or subsets depending on whether comparison is needed.
  - Evidence: `mux/src/domain.rs` (lines 30, 40), `arcterm-ai/src/agent.rs` (line 20: `#[derive(Debug, Clone, PartialEq, Eq)]` on `StepStatus`)
- **Command/message types** derive `Debug, Clone`.
  - Evidence: `wezterm-gui/src/commands.rs` (lines 18, 71), `arcterm-ai/src/backend/mod.rs` (line 18: `#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]` on `Message`)
- **Wire/serialization types**: Types crossing the wire (LLM API message bodies) additionally derive `serde::Serialize, serde::Deserialize`.
  - Evidence: `arcterm-ai/src/backend/mod.rs` (lines 9–18: `Role` and `Message` both derive `serde::Serialize, serde::Deserialize`)
- **`serde` attribute**: Enum variants use `#[serde(rename_all = "lowercase")]` so they serialize as `"system"`, `"user"`, `"assistant"` — matching the LLM API wire format.
  - Evidence: `arcterm-ai/src/backend/mod.rs` (line 10: `#[serde(rename_all = "lowercase")]`)
- **Enum ordering in derives**: The order generally follows `Debug, Clone, Copy, PartialEq, Eq, Default` — with `Copy` included when the type is small and value semantics are natural.

### HTTP Client Pattern (New — ArcTerm AI Crates)

The `arcterm-ai` crate introduces synchronous blocking HTTP as the mechanism for calling LLM backends. This is a deliberate choice for the streaming use case.

- **`ureq` for blocking HTTP**: `ureq` v2 with the `json` feature is used for all HTTP calls. It is a synchronous (blocking) client used directly on background threads. This is distinct from the rest of the codebase, which uses `async` I/O with `smol`. The choice avoids introducing an additional async runtime for a subsystem that runs off the main thread.
  - Evidence: `arcterm-ai/Cargo.toml` (line 14: `ureq = { version = "2", features = ["json"] }`)
- **Streaming via `Box<dyn Read + Send>`**: The `LlmBackend::chat` trait method returns `Box<dyn Read + Send>` — a streaming reader over NDJSON response lines. Callers consume this reader incrementally to display tokens as they arrive.
  - Evidence: `arcterm-ai/src/backend/mod.rs` (line 42: `fn chat(&self, messages: &[Message]) -> anyhow::Result<Box<dyn Read + Send>>`)
- **`response.into_reader()`**: Both backend implementations convert the ureq response body into a reader with `.into_reader()`. This avoids buffering the full response in memory.
  - Evidence: `arcterm-ai/src/backend/ollama.rs` (line 39), `arcterm-ai/src/backend/claude.rs` (line 51)
- **JSON body construction with `serde_json::json!`**: Request bodies are built with the `serde_json::json!` macro rather than typed structs with `#[derive(Serialize)]`. This avoids creating throwaway structs for one-off API payloads.
  - Evidence: `arcterm-ai/src/backend/ollama.rs` (lines 28–32), `arcterm-ai/src/backend/claude.rs` (lines 36–43)
- **Timeout for availability checks**: `is_available()` calls use an explicit `std::time::Duration::from_secs(2)` timeout to prevent blocking the UI on unresponsive backends.
  - Evidence: `arcterm-ai/src/backend/ollama.rs` (line 44: `.timeout(std::time::Duration::from_secs(2))`)

### WASM Integration Patterns (New — `arcterm-wasm-plugin`)

- **`wasmtime` Component Model**: Plugin loading uses `wasmtime::component::Component` and a `wasmtime::component::Linker<PluginStoreData>`. The `PluginStoreData` type (the `T` in `Store<T>`) carries the plugin's capability set and memory limit configuration.
  - Evidence: `arcterm-wasm-plugin/src/loader.rs` (lines 21–28, 76–85)
- **Per-plugin store isolation**: Each plugin gets its own `wasmtime::Store` instance. Stores are not shared. Memory and fuel limits are enforced per-store via `StoreLimitsBuilder` and `ResourceLimiter`.
  - Evidence: `arcterm-wasm-plugin/src/loader.rs` (lines 155–167: `StoreLimitsBuilder`, `store.limiter(...)`, `store.set_fuel(...)`)
- **`anyhow::Context` on wasmtime calls**: Every call into the wasmtime API that can fail is chained with `.with_context(|| format!(...))` so error messages always include the plugin name and the operation that failed.
  - Evidence: `arcterm-wasm-plugin/src/loader.rs` (lines 129–167: all wasmtime calls wrapped with `.with_context(...)`)
- **Host function registration pattern**: Each host API interface has a dedicated `register_*_functions(linker)` free function. A `create_default_linker` convenience function calls all four registration functions in sequence. This mirrors the Lua `register(lua)` pattern used in upstream lua-api-crates.
  - Evidence: `arcterm-wasm-plugin/src/host_api.rs` (lines 54, 99, 183, 285, 354)
- **Capability check before privileged I/O**: Every host function checks `ctx.data().capabilities.check(&required)` before performing any filesystem, network, or terminal-write operation. On denial the function returns `Err(String)` — it does not trap the WASM guest.
  - Evidence: `arcterm-wasm-plugin/src/host_api.rs` (lines 118–123, 146–151, 203–208, 305–309)
- **`std::sync::Mutex` for plugin registry**: The global plugin registration list uses `std::sync::Mutex` (not `parking_lot::Mutex`), paired with `lazy_static!`. This is the same pattern used in older upstream utilities. Poisoning is handled explicitly by recovering the guard.
  - Evidence: `arcterm-wasm-plugin/src/config.rs` (lines 54–72)

### Configuration Patterns

- **Config struct**: The root `Config` struct in `config/src/config.rs` is a single large struct with all configuration fields. It derives `FromDynamic`, `ToDynamic`, and `ConfigMeta` (a custom proc-macro from `wezterm-config-derive`).
  - Evidence: `config/src/config.rs` (lines 51–100)
- **`#[dynamic(...)]` attribute**: Fields on `Config` use `#[dynamic(default = "fn_name")]`, `#[dynamic(validate = "fn_name")]`, and `#[dynamic(try_from = "SomeType")]` attributes to control deserialization behavior and provide defaults.
  - Evidence: `config/src/config.rs` (lines 54–98)
- **`wezterm-dynamic` type bridge**: The project uses a bespoke intermediate value type (`wezterm_dynamic::Value`) that bridges between Lua values, TOML-deserialized values, and Rust types. Types that participate implement `FromDynamic` and `ToDynamic`. This avoids coupling directly to `serde`.
  - Evidence: `Cargo.toml` (line 249: `wezterm-dynamic`), `config/src/config.rs` (line 44), `luahelper/src/lib.rs`
- **Config access**: The config is retrieved via a `configuration()` free function that returns a `ConfigHandle` (an `Arc`-backed snapshot). Code across the codebase calls `config::configuration()` to get the current config.
  - Evidence: `wezterm-gui/src/update.rs` (line 72: `configuration().check_for_updates`)
- **[Inferred] ArcTerm config not yet in Lua bridge**: `AiConfig` and `WasmPluginConfig` do not implement `FromDynamic`/`ToDynamic` and are not registered with the main `Config` struct. They appear to be constructed ad hoc. Integration with the Lua config system is pending.
  - Evidence: `arcterm-ai/src/config.rs` (derives only `Debug, Clone`; no `FromDynamic`/`ToDynamic`)

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
- **`LlmBackend` trait**: The `arcterm-ai` crate introduces an `LlmBackend: Send + Sync` trait with a `chat` method. This follows the same `dyn`-compatible + `Send + Sync` bounds pattern used for core domain traits. The factory function `create_backend` returns `Box<dyn LlmBackend>`.
  - Evidence: `arcterm-ai/src/backend/mod.rs` (lines 39–59, 62–73)
- **`async_trait`**: Async methods on traits use the `#[async_trait(?Send)]` form (note `?Send` — the executor is single-threaded per-crate). `async-trait` 0.1 is the workspace dependency. The `arcterm-ai` crate does not use `async_trait` — its HTTP calls are synchronous blocking calls intended for background thread execution.
  - Evidence: `mux/src/domain.rs` (line 49), `mux/src/termwiztermtab.rs` (line 49)
- **`downcast-rs`**: Traits that need runtime downcasting (e.g., `Domain`, `Pane`, `Texture2d`) bound on `Downcast` and use `impl_downcast!(TraitName)`.
  - Evidence: `mux/src/domain.rs` (lines 17, 50, 200), `window/src/bitmaps/mod.rs` (line 16)
- **Visitor/callback traits**: Complex iteration is handled via single-method callback traits (e.g., `WithPaneLines`, `ForEachPaneLogicalLine`) passed as `&mut dyn Trait` parameters. This avoids `dyn Fn` boxes and allows returning references.
  - Evidence: `mux/src/pane.rs` (lines 352–370)
- **`ResourceLimiter` for WASM stores**: `PluginStoreData` implements `wasmtime::ResourceLimiter` directly, delegating to an owned `StoreLimits` instance. This is the idiomatic wasmtime pattern for per-store memory caps.
  - Evidence: `arcterm-wasm-plugin/src/loader.rs` (lines 47–65)

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
- **No unsafe in ArcTerm crates**: Neither `arcterm-ai` nor `arcterm-wasm-plugin` contain `unsafe` blocks. wasmtime manages the safety boundary for WASM guest execution internally.
- **No `#![deny(unsafe_code)]`**: There is no workspace-level unsafe denial. Unsafe blocks appear without explicit justification comments in most cases.

### Synchronization Conventions

- **`parking_lot::Mutex`** is preferred over `std::sync::Mutex` in the multiplexer and domain code. `std::sync::Mutex` appears mainly in older utilities and the connui module.
  - Evidence: `mux/src/domain.rs` (line 18), `mux/src/termwiztermtab.rs` (line 20)
- **`std::sync::Mutex` in `arcterm-wasm-plugin`**: The plugin registry (`REGISTERED_PLUGINS`) uses `std::sync::Mutex` (not `parking_lot`). Mutex poisoning is explicitly handled.
  - Evidence: `arcterm-wasm-plugin/src/config.rs` (lines 54–72)
- **`Arc<Mutex<T>>`** is the standard shared-mutable-state pattern.
- **`lazy_static!`** macro is used for global singletons. The newer `std::sync::LazyLock` appears in some newer code (`wezterm-toast-notification`, `wezterm-ssh/tests`). `arcterm-wasm-plugin` uses `lazy_static!`.
  - Evidence: `mux/src/lib.rs` (line 366), `wezterm-toast-notification/src/macos.rs` (lines 96–97), `arcterm-wasm-plugin/src/config.rs` (line 56)

### Documentation Style

- **Module-level docs**: Key modules use `//!` (inner doc comments) to explain the purpose and design rationale of the module. These are often multi-paragraph prose, not just one-liners. Both ArcTerm crates follow this consistently.
  - Evidence: `termwiz/src/caps/mod.rs` (lines 1–57), `arcterm-ai/src/lib.rs` (lines 1–4), `arcterm-wasm-plugin/src/host_api.rs` (lines 1–25: includes an interface table and WIT import example)
- **Section dividers in long files**: `arcterm-wasm-plugin` introduces a visual separator comment style (`// ── Section Name ───`) to divide long files into named sections. This is not present in upstream crates and is unique to the new ArcTerm code.
  - Evidence: `arcterm-wasm-plugin/src/loader.rs` (lines 13, 67, 97, 111, 185, 216), `arcterm-wasm-plugin/src/host_api.rs` (lines 31, 38, 83, 163, 266, 347, 366)
- **Item-level docs**: Public trait methods and struct fields are documented with `///` comments. Private or self-explanatory items are often undocumented.
  - Evidence: `mux/src/domain.rs` (lines 51, 151–196), `arcterm-ai/src/backend/mod.rs` (lines 8, 17, 38–58)
- **Numbered step comments in functions**: Complex multi-step operations in `arcterm-wasm-plugin` are annotated with `// 1. ... // 2. ...` inline comments keyed to the function's doc comment steps, creating a cross-reference between documentation and implementation.
  - Evidence: `arcterm-wasm-plugin/src/loader.rs` (lines 128–167 in `load_plugin`)
- **URL references**: External specifications are linked in doc comments with `<https://...>` syntax.
  - Evidence: `termwiz/src/caps/mod.rs` (line 66: `NO_COLOR` spec), `config/src/lua.rs` (line 260)
- **WIT examples in doc comments**: `arcterm-wasm-plugin/src/host_api.rs` includes a `# Component Model import paths` section with a WIT code block inside a doc comment. This is the only occurrence of non-Rust code blocks in rustdoc within the codebase.
  - Evidence: `arcterm-wasm-plugin/src/host_api.rs` (lines 17–24)
- **Clippy suppressions**: `#[allow(clippy::...)]` is used sparingly and at the smallest applicable scope. No blanket `allow(clippy::all)` exists. The ArcTerm crates have no clippy suppressions.
  - Evidence: `term/src/screen.rs` (line 1), `wezterm-gui/src/quad.rs` (line 3), `wezterm-gui/src/glyphcache.rs` (line 701)

---

## Summary Table

| Item | Detail | Confidence |
|------|--------|------------|
| Rust edition | 2018 (workspace `.rustfmt.toml`); 2021 in `arcterm-ai` and `arcterm-wasm-plugin` manifests | Observed |
| Formatter | `cargo +nightly fmt`, `imports_granularity = "Module"`, 4-space indent | Observed |
| Error propagation | `anyhow::Result<T>` with `.context(...)` / `.with_context(...)` chains | Observed |
| Typed errors | `thiserror::Error` derive at library boundaries (incl. `CapabilityDenied`) | Observed |
| Logging | `log` crate (v0.4); all 5 levels used; plugin log lines prefixed with `[plugin/<name>]` | Observed |
| Config bridge | Custom `wezterm-dynamic` DynValue layer; ArcTerm AI configs not yet wired in | Observed |
| Lua bindings | `mlua`; `impl_lua_conversion_dynamic!` macro; `register(lua)` entry point per crate | Observed |
| Trait dispatch | `Arc<dyn Trait>` with `downcast-rs` for runtime downcasting; `Box<dyn LlmBackend>` in arcterm-ai | Observed |
| Async traits | `async_trait(?Send)` — single-threaded executor model; arcterm-ai uses blocking IO instead | Observed |
| HTTP client | `ureq` v2 (blocking, `json` feature); `Box<dyn Read + Send>` for NDJSON streaming | Observed |
| WASM integration | `wasmtime` v36 Component Model; per-plugin `Store<PluginStoreData>`; capability check before all I/O | Observed |
| Synchronization | `parking_lot::Mutex` preferred upstream; `std::sync::Mutex` + `lazy_static!` in `arcterm-wasm-plugin` | Observed |
| Unsafe code | Localized to GPU buffer handling, Win32 FFI, one lifetime-extension workaround; none in ArcTerm crates | Observed |
| Clippy suppressions | Targeted, per-item; no blanket suppressions; none in ArcTerm crates | Observed |
| Section dividers | `// ── Name ───` ASCII box-drawing style in arcterm-wasm-plugin only | Observed |
| Numbered step comments | `// 1. ... // 2. ...` cross-referenced with doc comment in `load_plugin` | Observed |
| `serde` on wire types | `serde::Serialize/Deserialize` + `#[serde(rename_all = "lowercase")]` on LLM message types | Observed |
| `tempfile` for test fixtures | `tempfile::NamedTempFile` used in `arcterm-wasm-plugin` tests to create invalid WASM fixtures | Observed |
| Cargo deny | `deny.toml` present; license allowlist enforced; multiple-versions = warn | Observed |

## Open Questions

- There is no `clippy.toml` or `[workspace.metadata.clippy]` configuration. [Inferred] The project relies on default clippy lints plus the scattered `#[allow]` suppressions rather than a curated lint profile.
- The `wezterm-dynamic` / `FromDynamic` / `ToDynamic` system is central but underdocumented. Understanding how it diverges from `serde` in capability or intent requires deeper reading of `wezterm-dynamic/src/`.
- The `config/derive/` and `wezterm-dynamic/derive/` proc-macro crates generate significant behavior (`ConfigMeta`, `FromDynamic`, `ToDynamic`). These are not sampled in this analysis — the full attribute surface is not documented here.
- `AiConfig` and `WasmPluginConfig` are not yet wired into the Lua config bridge (`FromDynamic`/`ToDynamic` not implemented). It is unknown whether they will use the `wezterm-dynamic` bridge or a direct `mlua` approach when integrated.
- The `arcterm-wasm-plugin` network functions (`http-get`, `http-post`) are marked as placeholders returning `Err("network not yet implemented")`. The planned HTTP client for plugin use is not yet determined (ureq, reqwest, or another crate).
  - Evidence: `arcterm-wasm-plugin/src/host_api.rs` (lines 209–210, 235–236)
