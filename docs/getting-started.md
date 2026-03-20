# Getting Started with ArcTerm

ArcTerm is a GPU-accelerated terminal emulator built on WezTerm, with AI-powered completions, an interactive AI pane, and a WASM plugin system.

## Prerequisites

- **Rust** — stable toolchain, Rust 2021 edition. Install via [rustup](https://rustup.rs/).
- **Ollama** — required for AI features. See [local-llm-setup.md](local-llm-setup.md) for installation.
- **Platform build deps** — on macOS: Xcode command-line tools. On Linux: see WezTerm's [build dependencies](https://wezfurlong.org/wezterm/install/linux.html).

## Building from Source

```bash
git clone https://github.com/lgbarn/arcterm.git
cd arcterm
cargo build --release
```

The binary is at `target/release/wezterm-gui`. The build takes 5–10 minutes on a fresh checkout.

For iterating on changes, use a debug build:

```bash
cargo run --bin wezterm-gui
```

To type-check without a full build:

```bash
cargo check --package wezterm-gui
```

## First Run

```bash
./target/release/wezterm-gui
```

On first launch, ArcTerm looks for a config file at:
1. `~/.arcterm.lua`
2. `$XDG_CONFIG_HOME/arcterm/arcterm.lua`
3. `~/.wezterm.lua` (fallback)
4. `$XDG_CONFIG_HOME/wezterm/wezterm.lua` (fallback)

Without a config file, ArcTerm starts with defaults. Create `~/.arcterm.lua` to customize it:

```lua
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

config.font_size = 14.0
config.color_scheme = 'Catppuccin Mocha'

return config
```

## Setting Up AI Features

AI features require Ollama. If you haven't installed it yet, do that first — see [local-llm-setup.md](local-llm-setup.md).

Once Ollama is running with a model pulled, add AI config to `~/.arcterm.lua`:

```lua
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

config.arcterm_ai = {
    backend = "ollama",             -- or "claude"
    endpoint = "http://localhost:11434",
    model = "qwen2.5-coder:7b",
    context_lines = 30,             -- scrollback lines sent as context
}

config.arcterm_suggestions = {
    enabled = true,
    debounce_ms = 300,              -- wait 300ms after keypress before querying
    accept_key = "Tab",
    context_lines = 10,
}

-- Bind the AI pane and command overlay to keys
config.keys = {
    { key = 'a', mods = 'CTRL|SHIFT', action = wezterm.action.OpenAiPane },
    { key = 'Space', mods = 'CTRL', action = wezterm.action.ToggleCommandOverlay },
}

return config
```

## Using the AI Pane

The AI pane is an interactive chat interface that has read access to your active terminal's scrollback and current working directory.

1. Press your configured key (e.g., `Ctrl+Shift+A`) to open the AI pane alongside your active pane.
2. Type a question or paste an error message.
3. Press Enter to send. The response streams in.
4. Type another message to continue the conversation, or close the pane when done.

The AI pane automatically captures context from the sibling pane — you can ask "what does this error mean?" without pasting anything.

## Using the Command Overlay

The command overlay generates a shell command from a natural language description.

1. Press your configured key (e.g., `Ctrl+Space`) to open the overlay.
2. Describe what you want: `find all rust files modified in the last 7 days`.
3. Press Enter. The generated command appears below the input.
4. Press Enter again to execute it, or Escape to dismiss.

## Using Inline Suggestions

Inline suggestions appear as ghost text after your cursor while you type a shell command. They require no setup beyond having AI configured and Ollama running.

- **Triggering**: Type a partial command and pause for 300ms (configurable).
- **Accepting**: Press Tab to insert the suggestion.
- **Dismissing**: Press Escape or keep typing to replace it.

Suggestions work best with [shell integration](shell-integration.md) enabled, which lets ArcTerm detect exactly when you are at a shell prompt. Without it, ArcTerm falls back to a heuristic: cursor on the last row + the foreground process is a known shell.

## Configuring WASM Plugins

WASM plugins are `.wasm` files that run inside ArcTerm with explicit capability grants.

```lua
-- arcterm.lua
wezterm.plugin.register_wasm({
    name = "my-plugin",
    path = wezterm.home_dir .. "/.config/arcterm/plugins/my_plugin.wasm",
    capabilities = {
        "terminal:read",
        "fs:read:" .. wezterm.home_dir .. "/projects",
    },
    memory_limit_mb = 64,           -- default
    fuel_per_callback = 1000000,    -- default
})
```

See [CONTRIBUTING.md](../CONTRIBUTING.md) for how to build WASM plugins.

## Next Steps

- [local-llm-setup.md](local-llm-setup.md) — Ollama setup, model selection, troubleshooting
- [CONTRIBUTING.md](../CONTRIBUTING.md) — Developing plugins and ArcTerm itself
