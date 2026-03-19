# Feature Specification: Inline AI Command Suggestions

**Feature Branch**: `005-inline-ai-suggestions`
**Created**: 2026-03-19
**Status**: Draft
**Input**: User description: "Inline AI command suggestions — as the user types at the shell prompt, ArcTerm sends the partial command plus terminal context to the local LLM and renders a dimmed ghost suggestion inline after the cursor. Tab accepts the suggestion, Escape dismisses it, continuing to type refines the query. Works with Ollama by default, debounced to avoid flooding the LLM with requests on every keystroke."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Ghost Text Suggestion While Typing (Priority: P1)

A developer is at the shell prompt and starts typing a command. After a brief pause (debounce period), ArcTerm sends the partial command plus recent terminal context to the local LLM. The LLM returns a completion suggestion, which appears as dimmed gray text after the cursor — a "ghost" suggestion. The developer can see what the AI thinks they're about to type without any interruption to their workflow.

**Why this priority**: This is the core experience — the "wow" moment. Without ghost text rendering, there is no feature. Everything else (accept, dismiss, refine) depends on seeing the suggestion first.

**Independent Test**: Type `git` at a shell prompt, pause for 500ms, and verify that dimmed text appears after the cursor suggesting a plausible git subcommand (e.g., `status`, `commit -m "`, `push origin main`).

**Acceptance Scenarios**:

1. **Given** the user is at a shell prompt and types `git co`, **When** they pause typing for the debounce period, **Then** a dimmed suggestion appears inline after the cursor (e.g., `mmit -m "`)
2. **Given** the user is at a shell prompt, **When** they type rapidly without pausing, **Then** no suggestion appears until they stop typing for the debounce period
3. **Given** a suggestion is displayed, **When** the user continues typing characters that match the suggestion, **Then** the suggestion updates to show only the remaining untyped portion
4. **Given** a suggestion is displayed, **When** the user types a character that diverges from the suggestion, **Then** the old suggestion disappears and a new query is debounced
5. **Given** the LLM is unavailable, **When** the user types at the prompt, **Then** no suggestion appears and typing works exactly as normal — no errors, no delays

---

### User Story 2 - Accept Suggestion with Tab (Priority: P1)

A developer sees a ghost suggestion they want. They press Tab and the suggestion text is inserted into the command line as if they had typed it. The cursor moves to the end of the inserted text. The developer can then press Enter to execute or continue editing.

**Why this priority**: Accept is the primary interaction — without it, seeing a suggestion has no value. Tab is the standard accept key (matching Warp, Fish shell autosuggestions, and GitHub Copilot CLI conventions).

**Independent Test**: Type `docker`, wait for suggestion `run -it ubuntu bash`, press Tab, verify the full command `docker run -it ubuntu bash` appears at the prompt and is editable.

**Acceptance Scenarios**:

1. **Given** a ghost suggestion is visible, **When** the user presses Tab, **Then** the suggestion text is inserted at the cursor position and the ghost text disappears
2. **Given** no suggestion is visible, **When** the user presses Tab, **Then** normal shell Tab-completion behavior occurs (not intercepted)
3. **Given** a suggestion is visible, **When** the user presses Tab, **Then** the inserted text is editable — the user can backspace, add to it, or press Enter to execute

---

### User Story 3 - Dismiss Suggestion (Priority: P2)

A developer sees a suggestion they don't want. They press Escape to dismiss it, or they simply continue typing something different. The ghost text disappears cleanly with no artifacts left on screen.

**Why this priority**: Dismiss must work reliably — stale ghost text that doesn't clear is confusing and breaks the typing experience. This is lower than Tab-accept because the natural flow (keep typing to dismiss) handles most cases.

**Independent Test**: Type `ls`, see a suggestion, press Escape, verify the ghost text is gone and the prompt shows only `ls`.

**Acceptance Scenarios**:

1. **Given** a ghost suggestion is visible, **When** the user presses Escape, **Then** the ghost text disappears immediately
2. **Given** a ghost suggestion is visible, **When** the user types any character, **Then** the old suggestion disappears (a new query may be debounced based on the updated input)
3. **Given** a ghost suggestion is visible, **When** the user presses Enter (to execute what they've typed so far), **Then** the suggestion is dismissed and only the typed command executes
4. **Given** a ghost suggestion is visible, **When** the user moves the cursor (arrow keys, Home, End), **Then** the suggestion is dismissed

---

### User Story 4 - Context-Aware Suggestions (Priority: P2)

A developer runs `cargo build` and gets an error. They start typing at the next prompt. The AI suggestion takes into account the recent terminal output (the build error) and suggests a relevant command — like `cargo build --message-format=short` or the specific fix command for the error.

**Why this priority**: Context awareness is what makes this more than a dumb autocomplete. Using scrollback + CWD + last exit code transforms suggestions from "generic command completion" to "intelligent next-step prediction."

**Independent Test**: Run `git status` (showing modified files), then type `git add`, verify the suggestion includes the actual modified file names from the `git status` output.

**Acceptance Scenarios**:

1. **Given** the previous command output shows a compilation error, **When** the user starts typing a command, **Then** the suggestion is informed by the error output
2. **Given** the current directory is a Python project, **When** the user types `pip`, **Then** the suggestion relates to Python package management (not unrelated `pip` commands)
3. **Given** the last command exited with a non-zero exit code, **When** the user starts typing, **Then** the suggestion may offer a corrective action

---

### User Story 5 - Configurable Behavior (Priority: P3)

A user wants to customize the suggestion behavior — change the debounce delay, disable suggestions entirely, change the accept key, or adjust how much context is sent to the LLM. They modify their terminal config to adjust these settings.

**Why this priority**: Customization is important for power users but not for the initial experience. Sensible defaults should work for most users without configuration.

**Independent Test**: Set `ai_suggestions.enabled = false` in config, verify no suggestions appear. Set `ai_suggestions.debounce_ms = 1000`, verify suggestions wait 1 second.

**Acceptance Scenarios**:

1. **Given** the user sets `ai_suggestions.enabled = false`, **When** they type at the prompt, **Then** no suggestions appear
2. **Given** the user sets `ai_suggestions.debounce_ms = 1000`, **When** they pause typing, **Then** suggestions appear after 1 second instead of the default
3. **Given** the user sets `ai_suggestions.accept_key = "Right"`, **When** a suggestion is visible and they press Right arrow, **Then** the suggestion is accepted (instead of Tab)

---

### Edge Cases

- What happens when the terminal is in a non-shell application (vim, htop, ssh session)? Suggestions are suppressed when the terminal detects a full-screen application is running (no shell prompt visible).
- What happens when the user types very fast and the LLM response arrives for a stale query? The response is discarded if the input has changed since the query was sent.
- What happens when multiple suggestions arrive out of order? Only the most recent response matching the current input is displayed.
- What happens when the suggestion text wraps to the next line? The ghost text wraps naturally with the same dimming style, respecting the terminal width.
- What happens when the user resizes the terminal while a suggestion is visible? The suggestion is dismissed and re-queried on the next debounce cycle.
- What happens at the start of a session with no terminal history? Suggestions work with minimal context (just the partial command and CWD).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: ArcTerm MUST detect when the user is at a shell prompt (vs. inside a running program) before attempting to generate suggestions
- **FR-002**: ArcTerm MUST send the partial command text to the LLM after a configurable debounce period (default 300ms) of no typing activity
- **FR-003**: The LLM query MUST include context: the partial command, current working directory, last 10 lines of terminal output, and the last command's exit code
- **FR-004**: Suggestions MUST render as dimmed/gray text immediately following the cursor, visually distinct from user-typed text
- **FR-005**: Pressing Tab MUST accept the suggestion — inserting the text at the cursor and clearing the ghost text
- **FR-006**: Pressing Escape MUST dismiss the suggestion without inserting any text
- **FR-007**: Typing any character MUST dismiss the current suggestion and debounce a new query with the updated input
- **FR-008**: Pressing Enter MUST execute only the user-typed text, ignoring any visible suggestion
- **FR-009**: When no LLM is available, the suggestion feature MUST be completely invisible — no errors, no delays, no UI changes
- **FR-010**: Suggestions MUST NOT interfere with normal shell Tab-completion when no AI suggestion is visible
- **FR-011**: The debounce timer MUST reset on each keystroke — only the final pause triggers a query
- **FR-012**: Stale LLM responses (for input that has since changed) MUST be silently discarded
- **FR-013**: Terminal content MUST NOT be sent to remote APIs unless the user has explicitly configured a remote model
- **FR-014**: The suggestion system MUST be configurable: enable/disable, debounce delay, accept key, context line count
- **FR-015**: Suggestions MUST be suppressed when a full-screen application is detected in the terminal

### Key Entities

- **Suggestion**: A completion string returned by the LLM, displayed as ghost text after the cursor. Has a generation ID tied to the input state that triggered it.
- **Prompt Detection**: The mechanism that determines whether the user is at a shell prompt (eligible for suggestions) or inside a running program (suggestions suppressed).
- **Debounce Timer**: A timer that resets on each keystroke and fires after the configured delay, triggering an LLM query with the current partial command and context.
- **Ghost Text**: Dimmed text rendered after the cursor that shows the suggestion. Visually distinct from typed text. Not part of the terminal's text buffer — it's a rendering overlay.

## Assumptions

- Shell prompt detection uses OSC 133 semantic zones (emitted by modern shells like Fish, Zsh with prompt integration, Bash with `PROMPT_COMMAND` setup). If OSC 133 is not available, the system falls back to heuristic detection (cursor on last line, line starts with common prompt characters like `$`, `#`, `%`, `>`).
- The ghost text is a visual overlay rendered by the GUI — it is not injected into the terminal's text buffer. This means it doesn't appear in scrollback, isn't sent to the shell, and doesn't affect copy-to-clipboard.
- The default debounce delay is 300ms — short enough to feel responsive, long enough to avoid querying on every keystroke during fast typing.
- Tab is the default accept key. When no suggestion is visible, Tab passes through to the shell for normal completion. This means suggestions and shell completion coexist — Tab does AI accept when a suggestion is showing, shell completion when it's not.
- The LLM system prompt instructs the model to complete the command (not explain it): "Given the partial shell command and terminal context, return ONLY the completion text that should follow the cursor. No explanation, no backticks, no newlines. Just the remaining characters."
- Context sent to the LLM is the same as the AI pane context: CWD, last 10 lines of output, exit code. This reuses `arcterm-ai::context::PaneContext`.
- The feature is disabled by default when no LLM endpoint is reachable — it activates automatically when Ollama is detected on localhost.
- A small, fast model (1.5B-7B parameters) is recommended for suggestions to minimize latency. The default model from AiConfig is used.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Ghost text suggestion appears within 1 second of the user stopping typing (300ms debounce + LLM response time)
- **SC-002**: Tab-accept inserts the suggestion text in under 50ms — instantaneous from the user's perspective
- **SC-003**: Terminal remains at 60fps while suggestions are being queried and displayed — no visible lag or jank during typing
- **SC-004**: Typing latency is unaffected by the suggestion system — input appears on screen within the same frame as without suggestions enabled
- **SC-005**: At least 70% of displayed suggestions are relevant to the user's intent (measured by accept rate — users Tab-accept vs dismiss)
- **SC-006**: The feature works immediately with Ollama running on the default port — zero configuration required
- **SC-007**: Normal shell Tab-completion continues to work correctly when no AI suggestion is visible
- **SC-008**: All existing ArcTerm tests pass (`cargo test --all` green) with the suggestion system integrated
