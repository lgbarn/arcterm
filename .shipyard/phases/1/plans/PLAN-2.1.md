---
phase: ai-feature-hardening
plan: "2.1"
wave: 2
dependencies: ["1.2"]
must_haves:
  - wezterm.ai Lua sub-module is available in arcterm.lua after require("wezterm")
  - wezterm.ai.is_available() returns boolean
  - wezterm.ai.query(prompt) returns string or raises Lua error
  - wezterm.ai.get_config() returns table with backend, model, endpoint, allow_remote keys
  - New crate is in workspace members and registered via add_context_setup_func
files_touched:
  - Cargo.toml
  - lua-api-crates/ai-funcs/Cargo.toml
  - lua-api-crates/ai-funcs/src/lib.rs
  - wezterm-gui/Cargo.toml
  - wezterm-gui/src/main.rs
tdd: false
---

# PLAN-2.1 — Lua AI API

## Context

This plan introduces the `ai-funcs` lua-api crate that exposes a
`wezterm.ai` sub-module to Lua. It follows the exact pattern used by every
other crate in `lua-api-crates/`:

- Crate `lib.rs` exports `pub fn register(lua: &Lua) -> anyhow::Result<()>`.
- `register` calls `get_or_create_sub_module(lua, "ai")` to get/create the
  `wezterm.ai` table, then attaches functions to it.
- The new crate is added as a workspace member in the root `Cargo.toml` and
  as a dependency in `wezterm-gui/Cargo.toml`.
- `wezterm-gui/src/main.rs` registers it via
  `config::lua::add_context_setup_func(ai_funcs::register)` alongside the
  existing `window_funcs::register` call at line 1224.

This plan depends on PLAN-1.2 because `AiConfig::from(&AppConfig)` and the
`anyhow::Result`-returning `create_backend()` must exist before this code
can compile.

No streaming. No history. Blocking call for `query()` (runs on the Lua
callback thread, acceptable for config-time or status-bar use; not for
high-frequency event handlers).

---

<task id="1" files="Cargo.toml, lua-api-crates/ai-funcs/Cargo.toml, lua-api-crates/ai-funcs/src/lib.rs" tdd="false">
  <action>
    1. **Create the crate directory and source file.**

       `lua-api-crates/ai-funcs/Cargo.toml`:
       ```toml
       [package]
       name = "ai-funcs"
       version = "0.1.0"
       edition = "2021"
       publish = false

       [dependencies]
       anyhow.workspace = true
       arcterm-ai = { path = "../../arcterm-ai" }
       config.workspace = true
       log.workspace = true
       luahelper.workspace = true
       ```

       `lua-api-crates/ai-funcs/src/lib.rs`:
       Implement three Lua functions attached to the `wezterm.ai` sub-module.
       Follow the `battery/src/lib.rs` pattern exactly — use
       `get_or_create_module` to get `wezterm`, then
       `get_or_create_sub_module` to get/create `wezterm.ai`:

       ```rust
       use arcterm_ai::backend::create_backend;
       use arcterm_ai::config::AiConfig;
       use config::lua::get_or_create_sub_module;
       use config::lua::mlua::{self, Lua};

       pub fn register(lua: &Lua) -> anyhow::Result<()> {
           let ai_mod = get_or_create_sub_module(lua, "ai")?;

           ai_mod.set("is_available", lua.create_function(lua_is_available)?)?;
           ai_mod.set("query", lua.create_function(lua_query)?)?;
           ai_mod.set("get_config", lua.create_function(lua_get_config)?)?;
           Ok(())
       }
       ```

       **`lua_is_available`**: Build `AiConfig::from(&*config::configuration())`,
       call `create_backend(&ai_config)`. If `create_backend` returns `Err`
       (e.g. consent denied), return `false`. Otherwise call
       `backend.is_available()` and return the result.

       **`lua_query`**: Accept one `String` argument (`prompt`). Build
       `AiConfig::from(&*config::configuration())`, call
       `create_backend(&ai_config).map_err(mlua::Error::external)?`, call
       `backend.generate(&prompt, "")` to get a reader, drain the NDJSON
       stream with the same `BufRead` + `serde_json` loop used in
       `ai_command_overlay.rs:collect_streaming_response`, and return the
       concatenated plain string. On any error, propagate via
       `mlua::Error::external`.

       **`lua_get_config`**: Read `config::configuration()`, build a Lua table
       with keys `backend` (String), `model` (String), `endpoint` (String),
       `allow_remote` (bool), and return it.

    2. **Register the crate in the workspace root `Cargo.toml`:**
       - Add `"lua-api-crates/ai-funcs"` to the `[workspace] members` array.
       - Add `ai-funcs = { path = "lua-api-crates/ai-funcs" }` to
         `[workspace.dependencies]` (follow `battery`, `color-funcs` etc.).
  </action>
  <verify>cargo check --package ai-funcs</verify>
  <done>`cargo check --package ai-funcs` exits 0 with no errors.</done>
</task>

<task id="2" files="wezterm-gui/Cargo.toml, wezterm-gui/src/main.rs" tdd="false">
  <action>
    Wire the new crate into the GUI binary so it is available in every Lua
    context.

    1. **`wezterm-gui/Cargo.toml`**: Add `ai-funcs.workspace = true` to
       `[dependencies]` (alphabetical position, between the existing
       `arcterm-ai` and `bitflags` lines).

    2. **`wezterm-gui/src/main.rs`**: Add the registration call immediately
       after the existing `window_funcs::register` line (line 1224):
       ```rust
       config::lua::add_context_setup_func(window_funcs::register);
       config::lua::add_context_setup_func(ai_funcs::register);  // add this line
       config::lua::add_context_setup_func(crate::scripting::register);
       ```
       No `use` import needed because `add_context_setup_func` takes a
       function pointer and Rust resolves `ai_funcs::register` via the crate
       dependency.
  </action>
  <verify>cargo check --package wezterm-gui</verify>
  <done>`cargo check --package wezterm-gui` exits 0. `wezterm.ai` sub-module is reachable from Lua configs loaded by `make_lua_context`.</done>
</task>
