# Research: Inline AI Command Suggestions

**Date**: 2026-03-19
**Feature**: 005-inline-ai-suggestions

## Decision 1: Shell Prompt Detection

**Decision**: Use OSC 133 semantic zones as primary detection. When the
cursor is inside a `SemanticType::Input` zone (between `133;B` and `133;C`),
the user is at a shell prompt typing a command. Fall back to heuristic
(cursor on last row + foreground process is a shell) when OSC 133 is absent.

**Rationale**: OSC 133 is already fully implemented in WezTerm's terminal
state machine. Modern shells (Fish, Zsh with integration, Bash with
PROMPT_COMMAND) emit these zones. The heuristic fallback handles shells
without integration.

**Key finding**: `133;D;N` (exit code) is currently a no-op in the handler —
exit code is not stored. This could be enhanced to provide richer context.

## Decision 2: Ghost Text Rendering

**Decision**: Implement as a pane-level overlay (via `assign_overlay_for_pane`)
that wraps the real pane and overrides `with_lines_mut` to inject dimmed
text at the cursor position. Uses `ColorAttribute::PaletteIndex(8)` (bright
black / dim gray) for the ghost text styling.

**Rationale**: This follows the exact pattern used by `CopyOverlay` for
rendering search matches on top of terminal content. The ghost text is a
rendering overlay — it's never in the terminal's text buffer, doesn't
affect scrollback, and doesn't interfere with copy-to-clipboard.

**Alternatives considered**:
- Modify `Line` directly — would pollute scrollback and clipboard
- Custom GPU quad — too much effort, doesn't reuse rendering pipeline
- Floating overlay pane — would steal focus from shell input

## Decision 3: Debounce Strategy

**Decision**: Use `smol::Timer::after(Duration::from_millis(300))` with a
monotone `typing_cookie` counter. Each keystroke increments the cookie.
The async task waits for the timer, then checks if the cookie still matches.
If it does, the input hasn't changed — fire the LLM query.

**Rationale**: This is the exact pattern used by WezTerm's search overlay
(`copy.rs` lines 275-297) for debouncing search queries. Proven, tested,
no new infrastructure.

**Performance**: The blocking `ureq` HTTP call is wrapped in
`smol::unblock(|| ...)` to avoid blocking the async executor. Results are
delivered to the GUI via `window.notify(TermWindowNotif::Apply(...))`.

## Decision 4: Input Interception (Tab/Escape)

**Decision**: The ghost text overlay registers an `"ai_suggestion"` key
table with `Tab -> AcceptAiSuggestion` and `Escape -> DismissAiSuggestion`.
When the overlay is active (suggestion visible), Tab is intercepted at
priority 2 (overlay key table). When no suggestion is visible, the overlay
is removed and Tab falls through to the shell at priority 6 (normal).

**Rationale**: WezTerm's key dispatch has a clear priority chain. Overlay
key tables sit at priority 2, above global bindings but below modals.
This means Tab-accept only fires when a suggestion is showing — shell
Tab-completion works normally otherwise. Same pattern as CopyOverlay's
key table.

## Decision 5: LLM Query Format

**Decision**: System prompt: "Given the partial shell command and terminal
context, return ONLY the completion text that should follow the cursor.
No explanation, no backticks, no newlines. Just the remaining characters."

Context includes: partial command, CWD, last 10 lines of output, last
exit code. Reuses `arcterm-ai::context::PaneContext`.

**Rationale**: The LLM should return just the completion suffix, not the
full command. This avoids the need to diff the response against the input.

## Decision 6: Crate Structure

**Decision**: Add the suggestion logic to the existing `arcterm-ai` crate
as a new `suggestions` module. The GUI-side overlay pane goes in
`wezterm-gui/src/` as a new file.

- `arcterm-ai/src/suggestions.rs` — debounce logic, query formatting,
  response cleaning
- `wezterm-gui/src/suggestion_overlay.rs` — ghost text pane overlay,
  key table registration, rendering integration

**Rationale**: The LLM interaction (query building, response parsing) is
crate logic. The rendering (ghost text injection, key interception) is
GUI logic. Same split as the AI pane feature.
