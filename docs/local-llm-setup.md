# Local LLM Setup

ArcTerm's AI features (AI pane, command overlay, inline suggestions) use an LLM backend. The default is [Ollama](https://ollama.com), which runs models locally with no API key or network required. Claude API is supported as an alternative.

## Installing Ollama

### macOS

```bash
brew install ollama
```

Or download the `.dmg` from https://ollama.com/download.

### Linux

```bash
curl -fsSL https://ollama.com/install.sh | sh
```

The installer adds an `ollama` systemd service that starts automatically.

### Windows

Download the installer from https://ollama.com/download. It installs Ollama as a background service.

## Pulling Models

Pull a model before starting ArcTerm. The default model is `qwen2.5-coder:7b`:

```bash
ollama pull qwen2.5-coder:7b
```

This downloads ~4.7GB. If disk space or RAM is limited, use a smaller model:

| Model | Size | Use case |
|-------|------|----------|
| `qwen2.5-coder:7b` | ~4.7GB | Default. Good balance of quality and speed. |
| `qwen2.5-coder:1.5b` | ~1.0GB | Inline suggestions on constrained hardware. |
| `codellama:7b` | ~3.8GB | Alternative if qwen2.5 feels slow. |
| `phi3:mini` | ~2.3GB | Very fast, lower quality. |

For inline suggestions specifically, a smaller model reduces latency:

```bash
ollama pull qwen2.5-coder:1.5b
```

Then in `arcterm.lua`:

```lua
config.arcterm_suggestions = {
    enabled = true,
    -- Use a smaller, faster model for suggestions
    model = "qwen2.5-coder:1.5b",
}
```

## Verifying Ollama is Running

```bash
ollama list          # Shows downloaded models
curl http://localhost:11434/api/tags   # Should return JSON with model list
```

If `curl` returns "connection refused", start Ollama manually:

```bash
ollama serve
```

On Linux with systemd:

```bash
systemctl status ollama
systemctl start ollama   # if stopped
```

## ArcTerm AI Configuration

Add to `~/.arcterm.lua`:

```lua
config.arcterm_ai = {
    backend = "ollama",                    -- "ollama" or "claude"
    endpoint = "http://localhost:11434",   -- Ollama default
    model = "qwen2.5-coder:7b",
    context_lines = 30,                    -- scrollback lines sent as context
}

config.arcterm_suggestions = {
    enabled = true,
    debounce_ms = 300,      -- milliseconds to wait after keypress
    accept_key = "Tab",     -- key to accept ghost-text suggestion
    context_lines = 10,     -- scrollback lines for suggestion context
}
```

All fields have defaults. You can omit `config.arcterm_ai` entirely and ArcTerm will use the defaults above.

## Using Claude API Instead of Ollama

If you have an Anthropic API key, you can use Claude as the backend:

```lua
config.arcterm_ai = {
    backend = "claude",
    model = "claude-sonnet-4-20250514",     -- or any claude-* model
    api_key = os.getenv("ANTHROPIC_API_KEY"),
}
```

Set the environment variable before launching ArcTerm:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
arcterm   # or however you launch it
```

Note: Claude API requires network access and incurs per-token costs. Inline suggestions with a remote API will have noticeable latency compared to a local Ollama model.

## Troubleshooting

### "Ollama not running" or no AI response

Check if Ollama is listening:

```bash
curl http://localhost:11434/api/tags
```

If it fails, start Ollama:

```bash
ollama serve   # foreground, or
systemctl start ollama   # systemd
```

### Wrong or missing model

List available models and pull the one ArcTerm is configured to use:

```bash
ollama list
ollama pull qwen2.5-coder:7b
```

If ArcTerm logs show `model not found`, the `model` field in your config does not match any installed model name exactly.

### Suggestions are slow

Suggestions fire after a 300ms debounce. If they still feel slow after appearing, the bottleneck is model inference time. Options:

1. Switch to a smaller model: `qwen2.5-coder:1.5b` or `phi3:mini`.
2. Increase `debounce_ms` to reduce how often queries fire (e.g., `500`).
3. Disable suggestions and use only the AI pane: `config.arcterm_suggestions = { enabled = false }`.

### Suggestions trigger inside vim or other editors

Suggestions use OSC 133 shell integration zones to detect when you are at a shell prompt. Without shell integration, ArcTerm falls back to a heuristic that checks the foreground process name. This heuristic may misfire in some setups.

Fix: enable [shell integration](shell-integration.md) so ArcTerm can detect prompt boundaries precisely. With shell integration active, suggestions are suppressed inside editors and other non-shell processes.

### Claude API returns 401

The `ANTHROPIC_API_KEY` environment variable is not set or not visible to ArcTerm. Verify:

```bash
echo $ANTHROPIC_API_KEY   # should print your key
```

If ArcTerm is launched from a GUI (e.g., macOS dock), it may not inherit shell environment variables. Set the key in `arcterm.lua` directly (use a secrets file rather than hardcoding):

```lua
local secrets = dofile(wezterm.home_dir .. "/.config/arcterm/secrets.lua")
config.arcterm_ai = {
    backend = "claude",
    api_key = secrets.anthropic_api_key,
}
```
