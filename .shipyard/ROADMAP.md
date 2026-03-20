# ArcTerm AI Hardening Roadmap

## Phase 1: AI Feature Hardening

**Goal:** Eliminate escape injection risk from LLM output, make AI backend selection and consent explicit via Lua config, and expose AI capabilities to config scripts through a first-class `wezterm.ai` Lua API.

**Success Criteria:**
- `sanitize_llm_output()` strips all ANSI/VT escape sequences before any LLM token reaches the terminal surface; fuzz test passes with adversarial payloads
- Selecting `BackendKind::Claude` without `ai_allow_remote = true` in `arcterm.lua` falls back to Ollama and emits a `log::warn!`; no `AiConfig::default()` call sites remain in GUI code
- `wezterm.ai.is_available()`, `wezterm.ai.query()`, and `wezterm.ai.get_config()` are callable from `arcterm.lua`; `query()` returns an error (not a panic) when `ai_allow_remote = false` and Claude is configured
- `cargo test --package arcterm-ai --package ai-funcs` passes with no failures

### Scope

Three tightly-coupled changes delivered together as a single hardening pass:

1. **Escape injection fix** — New `sanitize_llm_output(input: &str) -> String` in `arcterm-ai/src/lib.rs` using the termwiz VT parser to walk all sequences and retain only printable text. Applied at `wezterm-gui/src/ai_pane.rs:221` and `wezterm-gui/src/overlay/ai_command_overlay.rs:183`. All ANSI/VT sequences are dropped.

2. **Lua config + consent gate** — `ai_backend`, `ai_model`, `ai_endpoint`, `ai_context_lines`, `ai_allow_remote`, and `ai_api_key` fields added to the `Config` struct in `config/src/config.rs`. `AiConfig` construction in `wezterm-gui` switches from `AiConfig::default()` to reading these fields. Claude backend requires `ai_allow_remote = true`; absence triggers fallback to Ollama with a `log::warn!`.

3. **Lua AI API** — New crate `lua-api-crates/ai-funcs` mirroring the structure of `lua-api-crates/color-funcs`. Registers `wezterm.ai.is_available()`, `wezterm.ai.query(prompt)`, and `wezterm.ai.get_config()` via `get_or_create_sub_module`. Consent gate checked inside `query()` before dispatching to the backend. Crate wired into the module registration chain in `wezterm-gui/src/scripting/`.

### Out of Scope

- Changes to the Ollama or Claude HTTP transport layers
- Agent mode (`arcterm-ai/src/agent.rs`) behavior changes
- Inline suggestion logic (`arcterm-ai/src/suggestions.rs`)
- WASM plugin system (`arcterm-wasm-plugin/`)
- Structured output (`arcterm-structured-output/`)
- Upstream WezTerm merge or rebrand surface changes
- UI changes to the AI pane or command overlay beyond the sanitization call site
