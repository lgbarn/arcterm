# Data Model: Inline AI Command Suggestions

**Date**: 2026-03-19
**Feature**: 005-inline-ai-suggestions

## Suggestion

The current AI suggestion displayed as ghost text.

**Attributes**:
- `text: String` — the completion text to display after the cursor
- `generation_id: u64` — monotone counter tying this suggestion to the input state that triggered it
- `input_prefix: String` — the partial command that generated this suggestion
- `state: SuggestionState` — current display state

**State transitions**:
```
Idle → Debouncing → Querying → Displaying → Idle
         ↓              ↓           ↓
        Idle           Idle        Idle
    (new keystroke)  (new keystroke) (accept/dismiss)
```

- `Idle`: No suggestion visible, no query pending
- `Debouncing`: User paused typing, timer running (300ms default)
- `Querying`: LLM request in flight
- `Displaying`: Ghost text visible, waiting for Tab/Escape/new input
- Any state → `Idle`: keystroke resets to idle (new debounce starts)

## PromptDetection

Determines whether suggestions should be active.

**Attributes**:
- `at_prompt: bool` — true when cursor is in a shell prompt input zone
- `detection_method: DetectionMethod` — how prompt was detected

**Detection methods**:
- `Osc133`: Cursor in `SemanticType::Input` zone (reliable)
- `Heuristic`: Cursor on last row + foreground is a shell (fallback)
- `Disabled`: Feature turned off in config

## SuggestionConfig

User-configurable settings.

**Attributes**:
- `enabled: bool` — master toggle (default: true)
- `debounce_ms: u32` — delay before querying (default: 300)
- `accept_key: String` — key to accept (default: "Tab")
- `context_lines: u32` — scrollback lines sent to LLM (default: 10)
- `model: Option<String>` — override model for suggestions (default: use AiConfig model)

## GhostTextOverlay

The rendering overlay that displays suggestion text.

**Attributes**:
- `suggestion: Option<Suggestion>` — current suggestion to render
- `cursor_col: usize` — column where ghost text starts
- `cursor_row: usize` — row where ghost text starts
- `style: CellAttributes` — dimmed/gray styling for ghost text

## Relationships

```
User keystroke → PromptDetection (is user at shell prompt?)
  → if yes: reset debounce timer
  → Debounce timer fires → build query (partial command + PaneContext)
  → LlmBackend::generate() → Suggestion
  → GhostTextOverlay renders Suggestion at cursor position
  → Tab: accept (inject text into shell) → Idle
  → Escape/keystroke: dismiss → Idle
```
