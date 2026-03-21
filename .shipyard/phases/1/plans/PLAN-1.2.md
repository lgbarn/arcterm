---
phase: ai-feature-hardening
plan: "1.2"
wave: 1
dependencies: []
must_haves:
  - Users can configure ai_backend, ai_model, ai_endpoint, ai_api_key, ai_context_lines in arcterm.lua
  - ai_allow_remote defaults to false; create_backend() returns Err when it is false and backend is Claude
  - All new fields derive FromDynamic/ToDynamic/ConfigMeta and appear in the top-level Config struct
  - GUI call sites read config from config::configuration() instead of AiConfig::default()
files_touched:
  - config/src/config.rs
  - arcterm-ai/src/config.rs
  - arcterm-ai/src/backend/mod.rs
  - wezterm-gui/src/ai_pane.rs
  - wezterm-gui/src/overlay/ai_command_overlay.rs
tdd: false
---

# PLAN-1.2 — Lua Config + Consent Gate

## Context

`AiConfig` in `arcterm-ai/src/config.rs` is a private struct populated only
from `AiConfig::default()`.  Nothing in the top-level `Config` struct
(`config/src/config.rs`) exposes AI settings to Lua.  Both GUI call sites
(`ai_pane.rs:25`, `ai_command_overlay.rs:97`) call `AiConfig::default()`
directly, bypassing user configuration.

`create_backend()` in `arcterm-ai/src/backend/mod.rs:62` constructs a
`ClaudeBackend` unconditionally when `BackendKind::Claude` is selected, with
no consent check.

The design decision is: **config flag only** (`ai_allow_remote = false` by
default).  No interactive prompt.

---

<task id="1" files="config/src/config.rs" tdd="false">
  <action>
    Add six AI-related fields to the `Config` struct in `config/src/config.rs`.
    All must use the `#[dynamic(default)]` or `#[dynamic(default = "fn_name")]`
    attribute pattern already used by other fields, and the struct already
    derives `FromDynamic, ToDynamic, ConfigMeta`, so no derive changes are
    needed.

    Insert the following block inside the `Config` struct, after the last
    existing field before the closing brace (search for a natural grouping,
    e.g. after the `check_for_updates` cluster):

    ```rust
    // --- AI subsystem ---

    /// Which LLM backend to use: "Ollama" or "Claude".
    #[dynamic(default = "default_ai_backend")]
    pub ai_backend: String,

    /// LLM model identifier.
    #[dynamic(default = "default_ai_model")]
    pub ai_model: String,

    /// LLM endpoint URL (used by Ollama backend).
    #[dynamic(default = "default_ai_endpoint")]
    pub ai_endpoint: String,

    /// API key for remote providers. None for Ollama.
    #[dynamic(default)]
    pub ai_api_key: Option<String>,

    /// Number of scrollback lines to include as context.
    #[dynamic(default = "default_ai_context_lines")]
    pub ai_context_lines: u32,

    /// Allow sending data to remote LLM providers (e.g. Claude API).
    /// Must be explicitly set to true to enable non-local backends.
    #[dynamic(default)]
    pub ai_allow_remote: bool,
    ```

    Add the four default-value functions at module scope in `config.rs`:
    ```rust
    fn default_ai_backend() -> String { "Ollama".to_string() }
    fn default_ai_model() -> String { "qwen2.5-coder:7b".to_string() }
    fn default_ai_endpoint() -> String { "http://localhost:11434".to_string() }
    fn default_ai_context_lines() -> u32 { 30 }
    ```
  </action>
  <verify>cargo check --package config</verify>
  <done>`cargo check --package config` exits 0. The new fields are accessible as `config::Config { ai_backend, ai_model, ... }`.</done>
</task>

<task id="2" files="arcterm-ai/src/backend/mod.rs, arcterm-ai/src/config.rs" tdd="false">
  <action>
    1. **Consent gate in `create_backend()`** (`arcterm-ai/src/backend/mod.rs:62`):
       Change the signature to return `anyhow::Result<Box<dyn LlmBackend>>`
       and add a guard before the Claude branch:

       ```rust
       pub fn create_backend(config: &crate::config::AiConfig) -> anyhow::Result<Box<dyn LlmBackend>> {
           match config.backend {
               crate::config::BackendKind::Claude => {
                   if !config.allow_remote {
                       anyhow::bail!(
                           "Remote LLM backend (Claude) requires `ai_allow_remote = true` in arcterm.lua"
                       );
                   }
                   Ok(Box::new(claude::ClaudeBackend::new(
                       config.api_key.clone().unwrap_or_default(),
                       config.model.clone(),
                   )))
               }
               crate::config::BackendKind::Ollama => Ok(Box::new(ollama::OllamaBackend::new(
                   config.endpoint.clone(),
                   config.model.clone(),
               ))),
           }
       }
       ```

    2. **Add `allow_remote` to `AiConfig`** (`arcterm-ai/src/config.rs`):
       Add `pub allow_remote: bool` to the `AiConfig` struct, defaulting to
       `false` in `impl Default for AiConfig`.

    3. **Add a `From<&config::Config>` conversion** in `arcterm-ai/src/config.rs`
       so call sites can build an `AiConfig` from the live global config.
       Add `use config::Config as AppConfig;` at the top and implement:

       ```rust
       impl From<&AppConfig> for AiConfig {
           fn from(c: &AppConfig) -> Self {
               let backend = match c.ai_backend.as_str() {
                   "Claude" => BackendKind::Claude,
                   _ => BackendKind::Ollama,
               };
               Self {
                   backend,
                   endpoint: c.ai_endpoint.clone(),
                   model: c.ai_model.clone(),
                   api_key: c.ai_api_key.clone(),
                   context_lines: c.ai_context_lines,
                   allow_remote: c.ai_allow_remote,
               }
           }
       }
       ```
  </action>
  <verify>cargo check --package arcterm-ai</verify>
  <done>`cargo check --package arcterm-ai` exits 0. `create_backend` now returns `anyhow::Result`.</done>
</task>

<task id="3" files="wezterm-gui/src/ai_pane.rs, wezterm-gui/src/overlay/ai_command_overlay.rs" tdd="false">
  <action>
    Replace the `AiConfig::default()` call sites with live config reads and
    propagate the `Result` from `create_backend`.

    **ai_pane.rs (lines 25-26):**
    Replace:
    ```rust
    let config = AiConfig::default();
    let backend = create_backend(&config);
    ```
    with:
    ```rust
    let ai_config = AiConfig::from(&*config::configuration());
    let backend = create_backend(&ai_config).context("Failed to create AI backend")?;
    ```
    Add `use arcterm_ai::config::AiConfig;` if not already imported (it is, at
    line 9). Add `use anyhow::Context as _;` if not already present.

    **ai_command_overlay.rs (lines 97-98):**
    Replace:
    ```rust
    let config = AiConfig::default();
    let backend = create_backend(&config);
    ```
    with:
    ```rust
    let ai_config = AiConfig::from(&*config::configuration());
    let backend = create_backend(&ai_config).context("Failed to create AI backend")?;
    ```
    Add `use config as app_config;` or adjust the import so `config::configuration()` resolves. The `config` crate is already a dependency of `wezterm-gui` (see `Cargo.toml` line 45). Add `use anyhow::Context as _;` if not already present.

    After `create_backend` returns `Err`, the existing `if !backend.is_available()` guard becomes the fallback for Ollama availability. The consent error from `create_backend` will surface as a returned `Err` from the overlay/pane function, which callers already handle by closing the overlay gracefully.
  </action>
  <verify>cargo check --package wezterm-gui</verify>
  <done>`cargo check --package wezterm-gui` exits 0. No `AiConfig::default()` calls remain in the two files.</done>
</task>
