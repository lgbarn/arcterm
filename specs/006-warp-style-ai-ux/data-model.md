# Data Model: Warp-Style AI UX

**Date**: 2026-03-19
**Feature**: 006-warp-style-ai-ux

## CommandPanel

The compact bottom panel for one-shot command generation.

**Attributes**:
- `visible: bool` — whether the panel is shown
- `input: String` — the user's current query text
- `result: Option<CommandResult>` — the generated command + explanation
- `is_loading: bool` — whether waiting for LLM response
- `height_rows: u8` — panel height (default: 4, max 40% of terminal)

**State transitions**:
```
Hidden → InputActive → Loading → ShowingResult → Hidden
                                      ↓
                                  Executing → Hidden
```

## CommandResult

A generated command with metadata.

**Attributes**:
- `command: String` — the shell command to execute
- `explanation: String` — one-line description of what it does
- `is_destructive: bool` — whether the command matches destructive patterns
- `confirmed: bool` — whether the user has confirmed a destructive command

## AgentSession

A multi-step AI execution plan.

**Attributes**:
- `task: String` — the original user task description
- `steps: Vec<AgentStep>` — the ordered plan
- `current_step: usize` — index of the current step
- `state: AgentState` — overall session state

**State transitions**:
```
Planning → Reviewing → Executing → Reviewing → ... → Completed
                ↓           ↓
             Aborted     StepFailed → (Retry|Skip|Abort)
```

## AgentStep

A single step in an agent plan.

**Attributes**:
- `number: u32` — step number (1-indexed)
- `command: String` — shell command to execute
- `explanation: String` — what this step does and why
- `status: StepStatus` — Pending, Running, Completed, Failed, Skipped
- `output: Option<String>` — captured output from execution
- `exit_code: Option<i32>` — exit code after execution

## AgentState

```
Planning    — AI is generating the step plan
Reviewing   — user is viewing the next step, deciding to run/skip/abort
Executing   — a step's command is running in the terminal
StepFailed  — a step exited non-zero, waiting for user decision
Completed   — all steps done
Aborted     — user pressed q to abort
```

## Relationships

```
User presses Ctrl+` → CommandPanel opens → query → CommandResult
User presses Run → command pasted to terminal → panel closes

User types # at prompt → AgentSession created → AI generates steps
AgentSession → AgentStep[] → executed one at a time
Each step → monitors terminal for prompt return → next step
```
