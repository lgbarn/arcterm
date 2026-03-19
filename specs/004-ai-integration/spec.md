# Feature Specification: AI Integration Layer

**Feature Branch**: `004-ai-integration`
**Created**: 2026-03-19
**Status**: Complete
**Input**: User description: "AI integration layer with AI pane, command overlay, Ollama and Claude support, cross-pane context"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - AI Pane for Conversational Help (Priority: P1)

A developer encounters a build error in their terminal. They open an AI pane alongside their working pane using a keyboard shortcut. The AI pane automatically reads context from the sibling pane — the last 30 lines of output, the current working directory, and the last command with its exit code. The developer asks "why did that fail?" and the AI provides a terse, actionable explanation with a suggested fix command. The conversation persists — the developer can ask follow-up questions without re-explaining the context.

**Why this priority**: This is the headline differentiating feature of ArcTerm. No other terminal emulator provides an AI assistant that can read sibling pane context. It transforms the terminal from a passive output display into an interactive problem-solving environment.

**Independent Test**: Open ArcTerm, run a command that fails (e.g., `cargo build` with a syntax error), then open the AI pane with the configured shortcut. Verify the AI pane shows the build error context and responds to a question about it.

**Acceptance Scenarios**:

1. **Given** a user with Ollama running locally, **When** they press the AI pane shortcut, **Then** a new pane opens beside the active pane with an AI chat interface
2. **Given** an open AI pane, **When** the sibling pane has recent output, **Then** the AI pane automatically includes the last 30 lines, CWD, and last command as context
3. **Given** a conversation in the AI pane, **When** the user asks a follow-up question, **Then** the AI responds with awareness of the full conversation history
4. **Given** the AI pane receives a response, **When** tokens arrive from the LLM, **Then** they stream into the pane in real-time (not waiting for the full response)
5. **Given** Ollama is not running, **When** the user opens an AI pane and sends a message, **Then** the pane displays "LLM unavailable — is Ollama running?" without crashing

---

### User Story 2 - Command Overlay for Quick Lookups (Priority: P1)

A developer needs a shell command but can't remember the exact syntax. They press `Ctrl+Space` and a minimal floating prompt appears at the top of the terminal. They type their question ("find all .rs files modified in the last week") and the AI returns a single shell command. The developer presses Enter to paste the command into their active pane, or Escape to dismiss. No conversation history, no follow-ups — optimized for speed.

**Why this priority**: The command overlay is the "quick access" complement to the AI pane. Many AI interactions are one-shot command lookups that don't need a full conversation. This serves the most common use case with the least friction.

**Independent Test**: Press `Ctrl+Space`, type "list files larger than 100MB", verify a single shell command appears, press Enter, verify the command is pasted into the active terminal pane.

**Acceptance Scenarios**:

1. **Given** a user presses `Ctrl+Space`, **When** the overlay appears, **Then** it shows a minimal text input at the top of the screen
2. **Given** the user types a question and presses Enter, **When** the AI responds, **Then** exactly one shell command is displayed (no explanation, no markdown)
3. **Given** the command is displayed, **When** the user presses Enter, **Then** the command is pasted into the active pane's input and the overlay closes
4. **Given** the command is displayed, **When** the user presses Escape, **Then** the overlay closes without pasting anything
5. **Given** the overlay is open, **When** the user types a question, **Then** the active pane's context (last 30 lines, CWD) is included in the query automatically
6. **Given** Ollama is not running, **When** the user submits a query, **Then** the overlay shows "LLM unavailable" and does not crash

---

### User Story 3 - Multi-Model Support (Priority: P2)

A developer configures ArcTerm to use Claude API for AI features instead of (or in addition to) Ollama. They add their API key to the config and select Claude as the model. The AI pane and command overlay work identically regardless of which backend is active. The developer can switch between local (Ollama) and remote (Claude) models via configuration.

**Why this priority**: While Ollama is the default for privacy and offline use, some users will prefer Claude's capabilities for complex tasks. Multi-model support makes ArcTerm flexible without changing the user experience.

**Independent Test**: Configure ArcTerm with a Claude API key. Open the AI pane, ask a question, and verify the response comes from Claude. Switch config to Ollama and verify the same workflow uses the local model.

**Acceptance Scenarios**:

1. **Given** a user configures `model = "claude"` with an API key, **When** they use the AI pane, **Then** queries are sent to the Claude API instead of Ollama
2. **Given** a user configures `model = "ollama:qwen2.5-coder:7b"`, **When** they use the AI pane, **Then** queries are sent to Ollama with that specific model
3. **Given** no AI configuration is specified, **When** ArcTerm starts, **Then** it defaults to Ollama on `localhost:11434` with no API key required
4. **Given** a Claude API key is configured but the API is unreachable, **When** the user sends a message, **Then** the AI pane shows a connection error without crashing

---

### User Story 4 - Context Refresh and Awareness (Priority: P2)

A developer has an AI pane open and runs a new command in their working pane. They want the AI to see the updated output. They press a refresh shortcut to pull fresh context from the sibling pane. The AI's next response incorporates the new terminal state — the updated scrollback, new CWD, and latest command result.

**Why this priority**: Terminal state changes constantly. The AI needs to stay current with what the user is seeing. Without refresh, the AI's context becomes stale after the first command.

**Independent Test**: Open an AI pane, run a command in the sibling pane, press the context refresh shortcut, ask a question, and verify the AI references the new command output.

**Acceptance Scenarios**:

1. **Given** an AI pane is open, **When** the user presses the context refresh shortcut, **Then** the AI pane re-reads the sibling pane's last 30 lines, CWD, and last command
2. **Given** refreshed context, **When** the user asks a question, **Then** the AI's response reflects the updated terminal state
3. **Given** the sibling pane has no output, **When** context is refreshed, **Then** the AI acknowledges the empty context gracefully

---

### User Story 5 - Destructive Operation Warning (Priority: P3)

A developer asks the AI for a command, and the AI suggests something destructive (e.g., `rm -rf`, `DROP TABLE`, `git push --force`). Before suggesting it, the AI flags the command with a clear warning label so the user can make an informed decision before executing it.

**Why this priority**: Safety in a terminal is critical — a pasted destructive command executes immediately. The AI must actively protect users from accidental damage. Lower priority because this is a behavior quality feature built on top of the core AI functionality.

**Independent Test**: Ask the AI pane "delete all files in the current directory". Verify the response includes a warning label before the `rm` command.

**Acceptance Scenarios**:

1. **Given** the AI suggests a command containing `rm -rf`, **When** the response is displayed, **Then** a visible warning label (e.g., "DESTRUCTIVE") appears before the command
2. **Given** the command overlay returns a destructive command, **When** displayed, **Then** the command is highlighted in a warning color and requires an additional confirmation step before pasting

---

### Edge Cases

- What happens when the sibling pane has no scrollback (just opened)? The AI receives minimal context (empty scrollback, CWD only) and responds accordingly.
- What happens when the user has multiple panes? The AI pane reads context from the most recently focused non-AI pane.
- What happens when the LLM response is extremely long? Responses stream token-by-token into the AI pane, which scrolls like a normal terminal pane.
- What happens when the user closes the sibling pane while the AI pane is open? The AI pane continues functioning but notes it has no sibling context available.
- What happens when the network drops during a streaming response? The partial response is displayed, followed by a "[Connection lost]" message.
- What happens when the user sends a new message while the AI is still streaming a response? The current response is interrupted and the new query is sent.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: ArcTerm MUST provide an AI pane that opens as a split alongside the active pane via a configurable keyboard shortcut (default: `leader + i`)
- **FR-002**: The AI pane MUST automatically read context from the most recently focused sibling pane: last 30 lines of scrollback, current working directory, and last command with exit code
- **FR-003**: The AI pane MUST maintain conversation history within a session — follow-up questions retain prior context
- **FR-004**: LLM responses MUST stream token-by-token into the AI pane in real-time
- **FR-005**: ArcTerm MUST provide a command overlay triggered by a configurable keyboard shortcut (default: `Ctrl+Space`)
- **FR-006**: The command overlay MUST send the user's question plus active pane context to the LLM and display exactly one shell command in response
- **FR-007**: The command overlay MUST paste the returned command into the active pane when the user presses Enter, or dismiss on Escape
- **FR-008**: ArcTerm MUST support Ollama as the default LLM backend with zero configuration required (defaults to `localhost:11434`)
- **FR-009**: ArcTerm MUST support Claude API as an alternative backend, configurable via API key in the terminal config
- **FR-010**: AI features MUST degrade gracefully when no LLM is available — display a clear "unavailable" message, never crash
- **FR-011**: The AI pane MUST support a context refresh shortcut (default: `leader + c`) that re-reads sibling pane state
- **FR-012**: The AI MUST flag destructive commands (rm -rf, DROP TABLE, force push, etc.) with a visible warning before suggesting them
- **FR-013**: AI configuration (endpoint, model, API key) MUST be specified in the terminal's config file under an `ai` section
- **FR-014**: Terminal content MUST NOT be sent to remote APIs unless the user has explicitly configured a remote model — local inference is the default
- **FR-015**: The AI pane MUST render Markdown responses with basic formatting (bold, code blocks, lists)

### Key Entities

- **AI Pane**: A special pane type that connects to an LLM instead of a PTY. Maintains conversation history, reads sibling pane context, and renders streamed responses.
- **Command Overlay**: A floating prompt UI that appears over the terminal, sends one-shot queries, and pastes the result into the active pane.
- **LLM Backend**: An abstraction over model providers (Ollama, Claude API). Handles connection, authentication, request formatting, and response streaming.
- **Pane Context**: A snapshot of a terminal pane's state — last 30 lines of scrollback, current working directory, last command, and exit code — passed to the LLM with each query.
- **AI Configuration**: User settings for the AI subsystem — endpoint URL, model name, API key, and keyboard shortcuts.

## Assumptions

- Ollama is the default backend; it runs on `localhost:11434` with no authentication. Users install and run Ollama independently — ArcTerm does not manage the Ollama process.
- Claude API support uses Anthropic's Messages API with an API key. The key is stored in the config file (not environment variable) per convention.
- Default model for Ollama is `qwen2.5-coder:7b` — a capable coding model that runs on consumer hardware. Users can change this to any Ollama-hosted model.
- The system prompt for the AI pane emphasizes terseness, shell commands, and destructive operation warnings. The system prompt for the command overlay emphasizes returning a single command with no explanation.
- Context reading (scrollback, CWD, exit code) uses the existing `Pane` trait API from the `mux` crate — no new terminal protocol extensions needed.
- Conversation history is per-session (lost when the AI pane is closed). Persistent history across sessions is out of scope.
- The AI pane renders as a normal terminal pane with Markdown formatting — it does not require custom GPU rendering.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can open the AI pane, ask a question about a build error visible in their terminal, and receive an actionable response — all within 10 seconds of the AI pane opening
- **SC-002**: First token of a streaming response appears within 2 seconds of sending a query (with Ollama running locally)
- **SC-003**: The command overlay completes the full cycle (open → type → receive command → paste) in under 5 seconds for simple queries
- **SC-004**: Terminal remains responsive (60fps, sub-100ms input latency) while the AI pane is streaming a response
- **SC-005**: AI features work identically across Ollama and Claude backends — the same question produces a response through either provider with no user-visible difference in the interface
- **SC-006**: 100% of destructive command suggestions include a visible warning label
- **SC-007**: All existing ArcTerm tests pass (`cargo test --all` green) with the AI integration system
- **SC-008**: A developer with Ollama already running can use AI features immediately after ArcTerm install — zero configuration required
