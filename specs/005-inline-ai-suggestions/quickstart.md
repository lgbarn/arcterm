# Quickstart: Inline AI Command Suggestions

## Prerequisites

- ArcTerm built from `005-inline-ai-suggestions` branch
- Ollama running with a model pulled (`ollama pull qwen2.5-coder:7b`)
- A shell with OSC 133 support (Fish, Zsh with integration, or Bash with PROMPT_COMMAND)

## Test Basic Suggestion

1. Start ArcTerm: `cargo run --bin wezterm-gui`
2. At the shell prompt, type `git` and pause for ~500ms
3. Verify: dimmed gray text appears after the cursor suggesting a git subcommand
4. Press Tab: the suggestion is inserted into the command line
5. Press Enter or backspace to edit

## Test Dismiss

1. Type `ls` and pause — a suggestion appears
2. Press Escape — ghost text disappears
3. Type `ls -la` (continuing to type) — old suggestion disappears, new one may appear after pause

## Test Context Awareness

1. Run `cargo build` (with an error in your project)
2. At the next prompt, type `cargo` and pause
3. Verify: suggestion is context-aware (e.g., `build --message-format=short` or `fix`)

## Test Tab-Completion Coexistence

1. Ensure no AI suggestion is visible
2. Press Tab — normal shell completion fires (file/command completion)
3. Now type `git` and pause until suggestion appears
4. Press Tab — AI suggestion is accepted (not shell completion)

## Test Without Ollama

1. Stop Ollama
2. Type at the prompt — no suggestions appear, no errors, typing works normally

## Test Configuration

In `arcterm.lua`:
```lua
wezterm.ai_suggestions = {
  enabled = true,
  debounce_ms = 500,        -- longer delay
  context_lines = 5,        -- less context
}
```
