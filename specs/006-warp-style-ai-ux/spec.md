# Feature Specification: Warp-Style AI UX

**Feature Branch**: `006-warp-style-ai-ux`
**Created**: 2026-03-19
**Status**: Draft
**Input**: User description: "Modify ArcTerm to function like Warp — compact bottom command panel, agent mode with multi-step execution, polished integrated AI UX"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Compact Bottom Command Panel (Priority: P1)

A developer presses `Ctrl+`` and a compact panel slides up from the bottom of the terminal — 3-4 lines tall, not a full overlay. They type "find files larger than 100MB" and the panel shows the generated command with a brief explanation below it. They see two buttons: **Run** (executes immediately) and **Copy** (copies to clipboard). The terminal content above remains fully visible and interactive. Pressing Escape dismisses the panel.

**Why this priority**: The current command overlay takes over the entire tab, hiding the terminal. A compact bottom panel is the single biggest UX improvement — it makes AI feel integrated rather than intrusive.

**Independent Test**: Press `Ctrl+``, type a question, verify the terminal is still visible above, verify Run pastes and executes, verify Copy puts the command on the clipboard.

**Acceptance Scenarios**:

1. **Given** the user presses `Ctrl+``, **When** the panel opens, **Then** it occupies only the bottom 3-4 lines of the terminal and the rest of the terminal content remains visible and scrollable
2. **Given** the user types a question and presses Enter, **When** the AI responds, **Then** the generated command is displayed with a one-line explanation
3. **Given** a command is displayed, **When** the user presses Enter or clicks Run, **Then** the command is pasted into the active terminal and executed, and the panel closes
4. **Given** a command is displayed, **When** the user presses `Ctrl+C` or clicks Copy, **Then** the command is copied to the system clipboard and the panel remains open
5. **Given** the panel is open, **When** the user presses Escape, **Then** the panel closes with no side effects
6. **Given** a destructive command is returned, **When** displayed, **Then** it shows a warning badge and requires a confirmation step before Run

---

### User Story 2 - AI Agent Mode (Priority: P2)

A developer types `# deploy the staging environment` at their shell prompt. ArcTerm detects the `#` prefix, intercepts the command, and sends it to the AI as a task. The AI breaks the task into steps: (1) `git pull origin main`, (2) `docker build -t app:staging .`, (3) `docker push registry/app:staging`, (4) `kubectl apply -f k8s/staging.yaml`. Each step is displayed with an explanation. The developer reviews and presses Enter to execute each step, seeing the output before proceeding. They can press `s` to skip a step or `q` to abort the whole plan.

**Why this priority**: Agent mode is the most powerful AI feature — it turns the terminal into an intelligent execution environment. Lower priority than the bottom panel because it requires the panel UX to work first.

**Independent Test**: Type `# list all running docker containers and their resource usage`, verify ArcTerm intercepts the `#` prefix, generates a multi-step plan, and allows step-by-step execution.

**Acceptance Scenarios**:

1. **Given** the user types `# <task description>` at the shell prompt, **When** they press Enter, **Then** ArcTerm intercepts the input (does not send `#...` to the shell) and opens the agent mode UI
2. **Given** the AI generates a multi-step plan, **When** displayed, **Then** each step shows a numbered command with a brief explanation
3. **Given** a step is displayed, **When** the user presses Enter, **Then** the command executes in the terminal and the output is visible
4. **Given** a step has executed, **When** the output is complete, **Then** the next step is presented with its explanation, incorporating the output of the previous step if relevant
5. **Given** a step is displayed, **When** the user presses `s`, **Then** the step is skipped and the next step is presented
6. **Given** any step is displayed, **When** the user presses `q`, **Then** the entire plan is aborted and the user returns to the normal prompt
7. **Given** a step fails (non-zero exit code), **When** the failure is detected, **Then** the agent pauses and asks the user whether to retry, skip, or abort

---

### User Story 3 - Polished AI Integration UX (Priority: P3)

A developer uses ArcTerm's AI features daily. The experience feels native and polished — the AI pane has a proper title showing the model name, responses render with basic Markdown formatting (bold, code blocks, lists), the command panel has a loading spinner while waiting, and all AI UI elements match the terminal's color scheme.

**Why this priority**: Polish makes the difference between "functional prototype" and "product I want to use daily." Lower priority because the features work without polish.

**Independent Test**: Open the AI pane, ask a question that returns a code block, verify it renders with syntax highlighting. Open the command panel, verify the spinner appears while loading. Verify all AI UI uses the terminal's configured color scheme.

**Acceptance Scenarios**:

1. **Given** the AI pane receives a response with markdown code blocks, **When** rendered, **Then** code blocks appear with syntax highlighting matching the terminal theme
2. **Given** the command panel is waiting for a response, **When** loading, **Then** a spinner or "Thinking..." animation is visible
3. **Given** the user changes their terminal color scheme, **When** AI UI is displayed, **Then** it matches the new color scheme
4. **Given** the AI pane title bar, **When** displayed, **Then** it shows "ArcTerm AI" and the model name

---

### Edge Cases

- What happens when the compact panel opens but the terminal only has 5 lines visible? The panel takes at most 40% of the terminal height, scaling down for small windows.
- What happens when agent mode encounters a command that requires interactive input (e.g., `sudo`)? The agent pauses and lets the user interact directly with the running command; when it completes, the agent resumes.
- What happens when `#` is typed inside vim or another program? The `#` prefix interception only activates when the cursor is at a shell prompt (same detection as inline suggestions — OSC 133 or heuristic).
- What happens when the user wants to type a literal `#` at the prompt? `##` (double hash) is passed through as a single `#` to the shell.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: ArcTerm MUST provide a compact command panel that occupies the bottom 3-4 lines of the terminal when opened
- **FR-002**: The command panel MUST NOT obscure or disable the terminal content above — the user can still see their terminal output
- **FR-003**: The command panel MUST show the generated command with a one-line explanation
- **FR-004**: The command panel MUST provide Run (paste + execute) and Copy (clipboard) actions
- **FR-005**: Destructive commands in the panel MUST show a warning and require confirmation before Run
- **FR-006**: ArcTerm MUST detect `# <text>` at a shell prompt as an agent mode trigger
- **FR-007**: Agent mode MUST break tasks into numbered steps with explanations
- **FR-008**: Agent mode MUST execute steps one at a time, showing output before proceeding
- **FR-009**: Agent mode MUST allow skipping individual steps (`s`) or aborting the plan (`q`)
- **FR-010**: Agent mode MUST pause on step failure and offer retry, skip, or abort options
- **FR-011**: The `#` prefix interception MUST only activate at shell prompts (not inside programs)
- **FR-012**: `##` at a shell prompt MUST pass through as a literal `#` to the shell
- **FR-013**: AI pane responses MUST render Markdown formatting (bold, code blocks, inline code, lists)
- **FR-014**: All AI UI elements MUST respect the terminal's configured color scheme
- **FR-015**: The command panel MUST show a loading indicator while waiting for the LLM response

### Key Entities

- **Command Panel**: A compact UI element at the bottom of the terminal for one-shot command generation. Shows input field, generated command, explanation, and Run/Copy actions.
- **Agent Session**: A multi-step execution plan generated by the AI. Contains numbered steps, each with a command and explanation. Tracks execution state (pending, running, completed, failed, skipped).
- **Agent Step**: A single command in an agent plan. Has a number, command string, explanation, execution status, and output.

## Assumptions

- The compact command panel is rendered as a fixed-height region at the bottom of the terminal pane, above the scrollback. It does not create a new pane or overlay — it's a UI element within the existing pane's rendering.
- Agent mode uses the same LLM backend as the AI pane and command panel (Ollama by default, Claude optional).
- The agent's system prompt instructs the LLM to return a JSON array of steps: `[{"command": "...", "explanation": "..."}, ...]`. The agent parses this and presents steps sequentially.
- Step execution uses the existing `pane.send_paste()` mechanism to inject commands into the shell.
- Agent mode detects step completion by monitoring the terminal for a new shell prompt (same OSC 133 detection used by inline suggestions).
- Markdown rendering in the AI pane uses the existing `syntect` highlighting (already in the codebase) for code blocks, plus basic ANSI formatting for bold/italic/lists.
- The `#` prefix detection happens in the key handling layer — when a shell prompt is detected and the user types `#` as the first character, ArcTerm starts buffering until Enter.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The compact command panel opens and displays a result within 3 seconds of the user pressing Enter on their question
- **SC-002**: Terminal content above the panel remains visible and scrollable while the panel is open — zero content is hidden or displaced
- **SC-003**: Agent mode completes a 3-step task (each step under 10 seconds) with the user only pressing Enter between steps — total interaction time under 45 seconds
- **SC-004**: The `#` prefix is correctly intercepted at shell prompts 100% of the time and never intercepted inside running programs
- **SC-005**: Markdown code blocks in AI pane responses render with syntax highlighting for at least 10 programming languages
- **SC-006**: All existing ArcTerm tests pass (`cargo test --all` green) with the new features integrated
- **SC-007**: The full open → query → Run cycle for the command panel completes in under 5 seconds for simple queries
