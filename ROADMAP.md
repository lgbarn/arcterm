# ArcTerm Roadmap

ArcTerm is a fork of WezTerm with AI-powered features. This roadmap tracks what's shipped, what's in progress, and what's planned — benchmarked against Warp terminal's feature set.

---

## Shipped

### Rebrand
WezTerm → ArcTerm across all user-visible surfaces (menus, dialogs, env vars, window class). Internal crate names preserved for upstream merge hygiene.

### WASM Plugin System
Wasmtime v36 Component Model with capability-based sandboxing. Plugins are `.wasm` files declared in `arcterm.lua` with per-plugin capability strings.

### AI Chat Pane (`OpenAiPane`)
Full conversational AI pane with streaming responses. Reads scrollback, CWD, and foreground process from sibling panes as context. Conversation history capped at 20 exchanges. Destructive command detection with red warning banners.

### AI Command Overlay (`ToggleCommandOverlay`)
Lightweight modal: type a question in natural language, get a shell command back. Press Enter to paste it into your terminal, Escape to dismiss. Markdown and backtick stripping built in.

### Multi-Backend LLM Support
Ollama (local, default: `qwen2.5-coder:7b`) and Anthropic Claude API. Backend abstraction in `arcterm-ai/` with streaming NDJSON parsing for both.

### Destructive Command Detection
37 regex patterns covering `rm -rf`, `DROP TABLE`, `git push --force`, `mkfs`, `dd`, `chmod 777`, fork bombs, and more. Applied in both the chat pane and command overlay before displaying results.

### macOS App Bundle
Proper `ArcTerm.app` with icon, Info.plist, and Launch Services registration. Install via `scripts/install-macos-app.sh`.

---

## Phase 1 — AI Hardening (In Progress)

Security and configuration cleanup before adding new features.

- **Escape Injection Protection** — `sanitize_llm_output()` strips ANSI/VT sequences from LLM responses before they reach the terminal surface. Prevents prompt injection via crafted model output.
- **Lua Config for AI** — Move from hardcoded `AiConfig::default()` to `ai_backend`, `ai_model`, `ai_endpoint`, `ai_api_key`, `ai_allow_remote` fields in `arcterm.lua`.
- **Consent Gate** — Claude backend requires `ai_allow_remote = true`. Without it, falls back to Ollama with a warning. Prevents silent exfiltration of scrollback to external APIs.
- **Lua AI API** — `wezterm.ai.is_available()`, `wezterm.ai.query(prompt)`, `wezterm.ai.get_config()` callable from config scripts.

---

## Phase 2 — Inline Ghost-Text Suggestions

Autocomplete-style command suggestions that appear as dimmed "ghost text" after your cursor, powered by a local Ollama model. No data leaves your machine.

### How It Works
1. You start typing at a shell prompt (detected via OSC 133 semantic zones, with a heuristic fallback for shells without integration).
2. After 300ms of idle time (debounced), ArcTerm sends the partial command plus ~10 lines of scrollback context to your local Ollama instance.
3. The LLM returns a completion. ArcTerm strips markdown artifacts and duplicate prefixes, then renders the result as dimmed ghost text after your cursor.
4. **Tab** accepts the suggestion and inserts it. **Escape** or any navigation key dismisses it.

### What's Already Built
- Prompt detection logic: OSC 133 zones + heuristic fallback (`arcterm-ai/src/suggestions.rs`)
- Query builder with context injection
- Response cleaner (strips backticks, markdown, repeated prefixes, multi-line → single-line)
- `SuggestionConfig` (enabled, debounce_ms, accept_key, context_lines)
- `SuggestionState` state machine with debounce cookie tracking (`wezterm-gui/src/suggestion_overlay.rs`)
- 15+ unit tests passing

### What Needs to Be Built
- **Debounce timer** — Spawn async task on each keystroke, cancel previous, fire LLM query after 300ms idle.
- **Overlay rendering** — Inject ghost text into the terminal render pipeline. The suggestion appears as dimmed text (e.g., `CellAttributes` with reduced alpha or a distinct color) appended after the cursor position. Must not affect the actual terminal buffer.
- **Key interception** — Push an `"ai_suggestion"` key table when a suggestion is visible. Tab inserts the text via `pane.send_paste()`. Escape/arrow keys pop the table and clear the ghost text.
- **Async LLM dispatch** — Wire `SuggestionState` to Ollama backend. Query fires in background, response matched to current cookie (stale responses discarded).
- **Backend constraint** — Ghost text always uses Ollama (local). Never sends keystrokes to a remote API regardless of `ai_backend` config.

---

## Phase 3 — Agent Mode

A multi-step task executor that turns a natural-language request into a reviewed, step-by-step shell plan — then runs it with your approval at each step.

### What It Does

You open an Agent pane and describe a task:

> "Set up a Python virtualenv, install requirements.txt, run the test suite, and show me the coverage report"

The LLM breaks this into discrete steps:

```
 PLAN: 4 steps

 Step 1  python3 -m venv .venv
         Create a virtual environment

 Step 2  source .venv/bin/activate && pip install -r requirements.txt
         Install project dependencies

 Step 3  pytest --cov=src --cov-report=term-missing
         Run tests with coverage

 Step 4  open htmlcov/index.html
         Open the coverage report in browser
```

You review the full plan before anything executes. Then for each step:
- **Enter** — execute it
- **S** — skip it
- **A** — abort the entire plan
- **?** — ask the LLM to explain why this step is needed

Each command runs in your actual terminal. You see real output. If a step fails (non-zero exit), Agent mode pauses and offers:
- **R** — retry the step
- **S** — skip and continue
- **A** — abort

Destructive commands (`rm -rf`, `DROP TABLE`, `git push --force`, etc.) are flagged with a warning before you can execute them.

### What's Already Built
The full state machine in `arcterm-ai/src/agent.rs`:
- `AgentSession` manages task, steps, current position, overall state
- `AgentStep` tracks command, explanation, status (Pending/Running/Completed/Failed/Skipped), and captured output
- `AgentState` enum: Planning → Reviewing → Executing → StepFailed → Completed/Aborted
- `build_agent_query()` formats the task + pane context for the LLM
- `parse_steps()` extracts the JSON step array from LLM response (handles markdown wrapping)
- State transitions: execute, complete, skip, retry, abort — all covered by tests
- 8 unit tests passing (execute/complete, skip, abort, retry, failure, summary, empty plan, markdown extraction)

### What Needs to Be Built
- **Agent pane UI** — A new `TermWizTerminal`-based pane (similar to `ai_pane.rs`) that:
  - Accepts a task description from the user
  - Shows "Planning..." while the LLM generates the step plan
  - Renders the plan as a numbered list with commands and explanations
  - Highlights the current step, shows status icons (pending/running/done/failed/skipped)
  - Displays real command output inline after each step executes
  - Shows the summary on completion
- **Command execution bridge** — Agent needs to run commands in the user's actual shell environment (not a sandboxed subprocess). This means either:
  - Writing to the parent pane via `pane.send_paste()` and capturing output via scrollback polling, or
  - Spawning a new pane per step and reading its exit code
- **Output capture** — Read command output from the pane's scrollback between the command start and the next prompt (using OSC 133 zones or heuristic detection).
- **KeyAssignment integration** — Add `OpenAgentPane` action, wire it into the command palette and an optional keybinding.
- **Destructive step warnings** — Apply `is_destructive()` check to each step's command before allowing execution. Flagged steps require explicit confirmation.

### Design Principles
- **You are always in control.** Nothing executes without your explicit approval. Every step is reviewed before it runs.
- **Local by default.** Agent mode uses Ollama. Your task descriptions and terminal context stay on your machine.
- **Transparent.** Commands run in your real shell. You see exactly what happens. No hidden side effects.
- **Recoverable.** Skip steps, retry failures, or abort at any point. The agent adapts to where you are.

---

## Phase 4 — Blocks (Command Output Grouping)

Warp's signature UX feature: each command and its output form a discrete, selectable "block."

### What It Would Look Like
Instead of a continuous scroll of text, each prompt→command→output cycle is visually grouped:
- A subtle separator or background tint marks the boundary between blocks
- Click a block to select its entire output (for copying)
- Collapse/expand long output
- Per-block toolbar: copy output, re-run command, share as snippet
- Search scoped to a single block's output

### Technical Approach
- **OSC 133 shell integration** already marks prompt (A), command start (B), command end (C), and output end (D) zones in the terminal buffer. WezTerm (and therefore ArcTerm) already parses these.
- The rendering layer needs to draw block boundaries at zone transitions.
- Selection model needs a "select block" mode alongside character/line selection.
- Block metadata (command text, exit code, timestamp) can be derived from the semantic zones.

### Effort: Large
This touches the core rendering pipeline and selection model. It's the most impactful feature for general UX but also the most invasive change.

---

## Phase 5 — Workflows & Command Palette

### Workflows
Parameterized, saveable command templates:
```lua
-- In arcterm.lua
arcterm.workflows = {
  {
    name = "Deploy to staging",
    steps = {
      "git push origin {{branch}}:staging",
      "ssh staging 'cd /app && git pull && systemctl restart app'",
    },
  },
  {
    name = "New feature branch",
    steps = { "git checkout -b feature/{{name}} main" },
  },
}
```
Invoke from the command palette. `{{placeholders}}` prompt for input before execution.

### Enhanced Command Palette
- Fuzzy search across workflows, recent commands, keybindings, and actions
- AI-powered: type a question and get a suggested command without opening the full overlay

---

## Phase 6 — Modern Input Editor

Replace the standard readline-style input with a real editor widget at the prompt:
- Multi-line editing with proper cursor movement
- Syntax highlighting for shell commands
- Bracket matching and auto-close
- Inline error squiggles from shellcheck (if installed)

### Effort: Very Large
This is the most architecturally invasive change. The terminal emulator currently passes all keystrokes to the child process. A modern input editor means intercepting input before the shell sees it, which changes the fundamental data flow.

---

## Not Planned

These are Warp features that don't fit ArcTerm's direction:

- **Team/cloud features** — ArcTerm is a local tool. No accounts, no telemetry, no shared state.
- **Proprietary AI** — Local-first via Ollama. Claude API is opt-in and requires explicit consent.
- **Warp Drive (cloud sync)** — Workflows and config stay in `arcterm.lua` under version control.
