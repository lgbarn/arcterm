# AI Feature Hardening Design

**Date:** 2026-03-19
**Status:** Approved
**Branch:** TBD (create from `main` after structured-output PR merges)

## Problem

ArcTerm's AI features have three security and usability gaps:

1. **Escape injection:** LLM response tokens are rendered as raw terminal text without sanitizing ANSI/VT escape sequences. A compromised or adversarially prompted LLM could change window titles, enter alternate screen mode, or write to the clipboard via OSC 52.

2. **No consent gate:** The Claude backend silently sends terminal scrollback (which may contain passwords, API keys, or confidential data) to `api.anthropic.com`. Today this is safe only because `AiConfig::default()` hardcodes Ollama, but a single code change could enable data exfiltration with zero user awareness.

3. **No user configuration:** All AI entry points call `AiConfig::default()`. Users cannot configure backend, model, endpoint, or API key from `arcterm.lua`.

## Solution

### 1. Escape Injection Fix

New `sanitize_llm_output()` function in `arcterm-ai/src/sanitize.rs`:

```rust
pub fn sanitize_llm_output(input: &str) -> String {
    use termwiz::escape::parser::Parser;
    use termwiz::escape::{Action, ControlCode};

    let mut output = String::with_capacity(input.len());
    let mut parser = Parser::new();
    parser.parse(input.as_bytes(), |action| match action {
        Action::Print(c) => output.push(c),
        Action::Control(c) => match c {
            ControlCode::HorizontalTab
            | ControlCode::LineFeed
            | ControlCode::CarriageReturn => output.push(c as u8 as char),
            _ => {}
        },
        _ => {}
    });
    output
}
```

Applied at two call sites:
- `wezterm-gui/src/ai_pane.rs:221` — sanitize `display_token` before `Change::Text`
- `wezterm-gui/src/overlay/ai_command_overlay.rs:183` — sanitize `token` before `push_str`

### 2. Lua Config Integration + Consent Gate

Add AI fields to `config/src/config.rs` on the main `Config` struct:

```rust
#[dynamic(default)]
pub ai_backend: AiBackend,        // "ollama" (default) | "claude"
#[dynamic(default)]
pub ai_model: Option<String>,     // None = backend default
#[dynamic(default)]
pub ai_endpoint: Option<String>,  // None = backend default
#[dynamic(default)]
pub ai_api_key: Option<String>,   // Required for Claude
#[dynamic(default = "default_ai_context_lines")]
pub ai_context_lines: u32,        // Default: 30
#[dynamic(default)]
pub ai_allow_remote: bool,        // Default: false
```

`AiBackend` enum derives `FromDynamic`, `ToDynamic` with `#[dynamic(rename = "lowercase")]`.

Consent gate logic in `arcterm-ai/src/backend/mod.rs::create_backend()`:

- If `backend == Claude && !allow_remote` → log warning, fall back to Ollama
- If `backend == Claude && api_key.is_none()` → log warning, fall back to Ollama

Replace `AiConfig::default()` calls in `ai_pane.rs:25` and `ai_command_overlay.rs:97` with config-driven construction:

```rust
let user_config = config::configuration();
let ai_config = AiConfig::from_user_config(&user_config);
```

The mapping function lives in `wezterm-gui` to keep `arcterm-ai` decoupled from the `config` crate.

### 3. Lua AI API

New crate: `lua-api-crates/ai-funcs/`

Exposes `wezterm.ai` module:

```lua
-- Check if the AI backend is reachable
wezterm.ai.is_available()  -- returns bool

-- One-shot query (blocking, returns string)
wezterm.ai.query("explain this error", {
  context = "optional terminal context string",
})

-- Get current AI config as a table (read-only)
wezterm.ai.get_config()
-- returns { backend = "ollama", model = "qwen2.5-coder:7b", ... }
```

Constraints:
- No streaming API (Lua not suited for it)
- No conversation history (each query is stateless)
- No pane manipulation (that's `wezterm.mux`)
- Consent gate enforced: `query()` with Claude + `ai_allow_remote = false` returns error

## Files to Create

- `arcterm-ai/src/sanitize.rs` — sanitize function + tests
- `lua-api-crates/ai-funcs/Cargo.toml` — crate manifest
- `lua-api-crates/ai-funcs/src/lib.rs` — Lua AI API implementation

## Files to Modify

- `arcterm-ai/src/lib.rs` — export sanitize module
- `config/src/config.rs` — add `ai_*` fields and `AiBackend` enum
- `arcterm-ai/src/backend/mod.rs` — consent gate in `create_backend()`
- `wezterm-gui/src/ai_pane.rs` — sanitize tokens + load from user config
- `wezterm-gui/src/overlay/ai_command_overlay.rs` — sanitize tokens + load from user config
- `wezterm-gui/src/main.rs` — register `ai-funcs` Lua API
- `wezterm-gui/Cargo.toml` — add `ai-funcs` dependency
- `Cargo.toml` — add `lua-api-crates/ai-funcs` to workspace members

## User-Facing Config Example

```lua
-- arcterm.lua
local config = {}

-- AI defaults to Ollama on localhost, no config needed for basic use
config.ai_backend = "ollama"
config.ai_model = "qwen2.5-coder:7b"

-- To use Claude instead:
-- config.ai_backend = "claude"
-- config.ai_api_key = "sk-ant-..."
-- config.ai_allow_remote = true   -- required, explicit opt-in

-- Optional tuning
-- config.ai_context_lines = 50
-- config.ai_endpoint = "http://custom-ollama:11434"

return config
```

## Verification

1. **Escape injection:** Write a test that passes `"\x1b]0;evil\x07hello\x1b[31mworld"` through `sanitize_llm_output()` and asserts output is `"helloworld"` (title change and SGR stripped).
2. **Consent gate:** Test that `create_backend()` with `Claude` + `allow_remote = false` returns an Ollama backend. Test that it works with `allow_remote = true` + valid API key.
3. **Config integration:** Set `ai_backend = "claude"` without `ai_allow_remote = true` in a test config, verify fallback behavior.
4. **Lua API:** Call `wezterm.ai.is_available()` from a test Lua context, verify it returns a boolean. Call `wezterm.ai.get_config()` and verify the table structure.
5. **End-to-end:** Build and run `cargo run --bin wezterm-gui`, open AI pane (Ctrl+A, i), verify responses render cleanly without escape artifacts.
