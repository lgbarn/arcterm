# Contributing to ArcTerm

ArcTerm is a fork of [WezTerm](https://github.com/wez/wezterm) by Wez Furlong, extended with AI-powered features.

## Upstream Relationship

- **Upstream repository**: https://github.com/wez/wezterm
- **License**: MIT (same as upstream)
- **Remote setup**: `upstream` points to wez/wezterm, `origin` points to lgbarn/arcterm

We periodically merge upstream changes to stay current with WezTerm improvements. ArcTerm-specific code is kept in clearly separated modules/crates to minimize merge conflicts.

### Syncing with upstream

```console
$ git fetch upstream
$ git merge upstream/main
```

## ArcTerm-Specific Features

The following are unique to ArcTerm and not present in upstream WezTerm. Keep all contributions to these features inside their dedicated crates.

### 1. Rebrand (`wezterm-gui/`, `config/`, `mux/`)

User-facing strings, env vars, and platform identifiers use "ArcTerm". Internal crate names remain `wezterm-*` to keep upstream merges clean. When adding new user-visible strings, use "ArcTerm", not "WezTerm".

### 2. WASM Plugin System (`arcterm-wasm-plugin/`)

Capability-based sandboxed plugin execution via wasmtime v36 Component Model.

- Plugins are `.wasm` files built against the WIT interface in `arcterm-wasm-plugin/wit/plugin-host-api.wit`.
- Each plugin declares capability strings in `arcterm.lua` (e.g., `"fs:read:/home/user"`). The `terminal:read` capability is always granted.
- `loader.rs` compiles and stores plugins with per-plugin memory and fuel limits.
- `host_api.rs` provides the host-side implementation of the WIT imports.
- `event_router.rs` dispatches terminal events to plugin callbacks.

### 3. AI Integration (`arcterm-ai/`)

LLM backend abstraction, pane context extraction, and inline suggestion logic.

- `backend/` — `LlmBackend` trait with `OllamaBackend` and `ClaudeBackend` implementations.
- `config.rs` — `AiConfig` struct; default model is `qwen2.5-coder:7b` at `http://localhost:11434`.
- `context.rs` — `PaneContext`: scrollback snapshot, CWD, foreground process, dimensions.
- `suggestions.rs` — `is_at_shell_prompt`, `build_suggestion_query`, `clean_suggestion` for ghost-text completions.
- `destructive.rs` — detects destructive commands before running.
- `prompts.rs` — system prompt templates.

The GUI integration lives in `wezterm-gui/src/ai_pane.rs` (interactive pane) and `wezterm-gui/src/overlay/ai_command_overlay.rs` (command overlay). Key assignments `OpenAiPane`, `ToggleCommandOverlay`, `AcceptAiSuggestion`, and `DismissAiSuggestion` are defined in `config/src/keyassignment.rs` but have no default bindings — users assign them in `arcterm.lua`.

### 4. Inline AI Suggestions (`wezterm-gui/`, `arcterm-ai/`)

Ghost-text completions triggered after 300ms of inactivity at a shell prompt. Detection uses OSC 133 semantic zones (primary) with a heuristic fallback (cursor on last row + shell process name). `clean_suggestion` in `arcterm-ai/src/suggestions.rs` strips markdown, backticks, and repeated prefixes from LLM output before rendering.

## Development

### Building

```console
$ cargo build --release
```

### Running in debug mode

```console
$ cargo run --bin wezterm-gui
```

### Running tests

```console
$ cargo test --all
```

### Code formatting

```console
$ rustup component add rustfmt-preview
$ cargo fmt --all
```

## Where to Find Things

- `term/` — Core terminal model (VT parsing, escape sequences)
- `wezterm-gui/` — GUI renderer
- `config/` — Configuration system (Lua plugins)
- `mux/` — Multiplexer
- `arcterm-*` — ArcTerm-specific crates (WASM plugins, AI integration)

## Developing WASM Plugins

Plugins are WASM components built against the WIT interface at `arcterm-wasm-plugin/wit/plugin-host-api.wit`. The interface uses the [WebAssembly Component Model](https://component-model.bytecodealliance.org/).

### Minimal plugin in Rust

```toml
# Cargo.toml
[package]
name = "my-plugin"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.35"
```

```rust
// src/lib.rs
wit_bindgen::generate!({
    world: "arcterm-plugin",
    path: "path/to/plugin-host-api.wit",
});

struct MyPlugin;

impl Guest for MyPlugin {
    fn init() -> Result<(), String> {
        log::info("my-plugin loaded");
        Ok(())
    }
    fn destroy() {}
}

export!(MyPlugin);
```

Build and register:

```bash
cargo build --target wasm32-wasip2 --release
# Copy target/wasm32-wasip2/release/my_plugin.wasm to your plugin directory
```

```lua
-- arcterm.lua
wezterm.plugin.register_wasm({
    name = "my-plugin",
    path = "/path/to/my_plugin.wasm",
    capabilities = { "terminal:read" },
})
```

### Capability strings

| String | What it grants |
|--------|---------------|
| `terminal:read` | Read visible text, cursor, CWD, dimensions. Always granted. |
| `terminal:write` | Send keystrokes and inject output. |
| `fs:read:/path` | Read files under `/path` (path traversal blocked). |
| `fs:write:/path` | Write files under `/path`. |
| `net:connect:host:port` | HTTP requests to `host:port`. |
| `keybinding:register` | Register global key bindings. |

## Submitting a Pull Request

1. Fork the repository
2. Create a feature branch
3. Ensure tests pass: `cargo test --all`
4. Ensure formatting: `cargo fmt --all`
5. Submit your PR with a clear description of the changes
