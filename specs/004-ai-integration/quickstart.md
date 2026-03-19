# Quickstart: AI Integration Layer

## Prerequisites

- ArcTerm built from `004-ai-integration` branch
- Ollama installed and running (`ollama serve`)
- A model pulled (`ollama pull qwen2.5-coder:7b`)

## Verify Ollama is Running

```bash
curl http://localhost:11434/api/tags
# Should return JSON with available models
```

## Test AI Pane

1. Start ArcTerm: `cargo run --bin wezterm-gui`
2. Run a command that produces output: `ls -la`
3. Open AI pane: press `leader + i` (default: `Ctrl+A` then `i`)
4. Type: "What files are in this directory?"
5. Verify: AI responds with awareness of the `ls -la` output

## Test AI Pane with Error Context

1. In a terminal pane, run: `cargo build` (with a known syntax error)
2. Open AI pane: `leader + i`
3. Type: "Why did that fail?"
4. Verify: AI references the specific error from the build output

## Test Command Overlay

1. Press `Ctrl+Space` — overlay should appear at top of screen
2. Type: "find all rust files modified today"
3. Verify: a single `find` command appears (no explanation)
4. Press Enter — command should paste into the active pane
5. Press `Ctrl+Space` again, type something, press Escape — should dismiss

## Test Context Refresh

1. Open AI pane alongside a terminal pane
2. Run a new command in the terminal pane
3. Press `leader + c` to refresh context
4. Ask the AI about the new output
5. Verify: AI references the updated command output

## Test Without Ollama

1. Stop Ollama: `systemctl stop ollama` or kill the process
2. Open AI pane and send a message
3. Verify: "LLM unavailable — is Ollama running?" appears (no crash)
4. Press `Ctrl+Space` and submit a query
5. Verify: overlay shows "LLM unavailable" (no crash)

## Test Claude Backend

1. Add to `arcterm.lua`:
```lua
wezterm.ai = {
  model = "claude",
  api_key = "sk-ant-...",
}
```
2. Open AI pane, ask a question
3. Verify: response comes from Claude (different style than Ollama)

## Test Destructive Command Warning

1. Open AI pane
2. Type: "delete all files in current directory"
3. Verify: response includes a "DESTRUCTIVE" warning before the `rm` command
4. Open command overlay (`Ctrl+Space`)
5. Type: "force push to main"
6. Verify: returned command is highlighted with a warning
