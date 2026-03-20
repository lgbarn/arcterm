---
description: "Task list for Warp-Style AI UX"
---

# Tasks: Warp-Style AI UX

**Input**: Design documents from `/specs/006-warp-style-ai-ux/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, quickstart.md

**Organization**: Tasks grouped by user story.

## Format: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup

**Purpose**: Create new files and add helper infrastructure.

- [x] T001 Create `wezterm-gui/src/termwindow/render/ai_panel.rs` with module declaration — add `pub mod ai_panel;` to `wezterm-gui/src/termwindow/render/mod.rs`
- [x] T002 Create `arcterm-ai/src/agent.rs` with module declaration — add `pub mod agent;` to `arcterm-ai/src/lib.rs`
- [x] T003 Add `total_fixed_chrome_height(&self) -> usize` helper method to `TermWindow` in `wezterm-gui/src/termwindow/mod.rs` — returns `tab_bar_height + ai_panel_height` to centralize geometry calculations
- [x] T004 Add `ai_panel_height: usize` and `ai_panel_visible: bool` fields to `TermWindow` struct in `wezterm-gui/src/termwindow/mod.rs`
- [x] T005 Verify `cargo check --package wezterm-gui --package arcterm-ai` compiles

**Checkpoint**: New files exist, helper method added, crates compile.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: OSC 133;D command completion handler and agent step planning logic.

- [x] T006 Add `CommandComplete { status: i64 }` variant to `Alert` enum in `term/src/terminal.rs`
- [x] T007 Add `last_command_exit_status: Option<i64>` field to `TerminalState` in `term/src/terminalstate/mod.rs`
- [x] T008 Fill the OSC 133;D handler stub in `term/src/terminalstate/performer.rs` (line ~913) — store exit status on `TerminalState` and call `self.alert(Alert::CommandComplete { status })`
- [x] T009 Implement `AgentStep` and `AgentSession` structs in `arcterm-ai/src/agent.rs` — step with command/explanation/status/output, session with task/steps/current_step/state
- [x] T010 Implement `plan_steps(task: &str, context: &PaneContext) -> Vec<AgentStep>` in `arcterm-ai/src/agent.rs` — sends task to LLM with system prompt requesting JSON array of `{"command": "...", "explanation": "..."}` steps; parses response
- [x] T011 Write unit tests for agent step planning (mock LLM response parsing) and AgentSession state transitions
- [x] T012 Verify `cargo test --package arcterm-ai --package wezterm-term` passes

**Checkpoint**: OSC 133;D fires alerts. Agent planning logic works with tests.

---

## Phase 3: User Story 1 — Compact Bottom Command Panel (Priority: P1)

**Goal**: 3-4 line panel at bottom of terminal for command generation.

**Independent Test**: Press Ctrl+`, type question, see panel with Run/Copy, terminal visible above.

- [x] T013 [US1] Implement `paint_ai_panel()` in `wezterm-gui/src/termwindow/render/ai_panel.rs` — render 3-4 synthetic `Line` objects at the bottom of the window using `render_screen_line` (follow `paint_tab_bar` pattern exactly); lines contain: input prompt, generated command, explanation, and action hints
- [x] T014 [US1] Modify `apply_dimensions()` in `wezterm-gui/src/termwindow/resize.rs` — subtract `ai_panel_height` from `avail_height` alongside `tab_bar_height` so the PTY gets fewer rows when panel is visible
- [x] T015 [US1] Add `paint_ai_panel` call in `paint_pass()` in `wezterm-gui/src/termwindow/render/paint.rs` — after `paint_tab_bar`, paint the AI panel at the computed Y offset
- [x] T016 [US1] Implement panel show/hide toggle — when `ToggleCommandOverlay` KeyAssignment fires, set `ai_panel_visible = true` and `ai_panel_height = 4 * cell_height`; trigger resize to update PTY rows
- [x] T017 [US1] Implement panel input handling — when panel is visible, capture keystrokes: printable chars accumulate in panel input buffer, Enter submits query, Escape closes panel
- [x] T018 [US1] Implement panel LLM query — on Enter: send input + context to `backend.generate()` with command overlay prompt; display result in panel lines (command + explanation)
- [x] T019 [US1] Implement Run action — on Enter while result is displayed: call `pane.send_paste(&command)` then `pane.send_paste("\r")` to execute; close panel
- [x] T020 [US1] Implement Copy action — on Ctrl+C while result is displayed: copy command to system clipboard; panel stays open
- [x] T021 [US1] Implement destructive warning — check `is_destructive()` on result; if true, show warning badge in panel and require confirmation Enter before Run
- [x] T022 [US1] Update all geometry call sites that reference `tab_bar_height` to use `total_fixed_chrome_height()` instead (approximately 8 locations across mod.rs, resize.rs, pane.rs, mouseevent.rs, split.rs)
- [x] T023 [US1] Verify panel renders correctly and terminal content is fully visible above it — manual test

**Checkpoint**: Compact panel works end-to-end. Run/Copy/destructive warning functional. Terminal visible above.

---

## Phase 4: User Story 2 — AI Agent Mode (Priority: P2)

**Goal**: `# <task>` at prompt triggers multi-step AI execution.

**Independent Test**: Type `# list docker containers`, verify step-by-step plan with execute/skip/abort controls.

- [x] T024 [US2] Extend the AI overlay shim in `wezterm-gui/src/suggestion_overlay.rs` to intercept Enter key — when Enter is pressed: read input zone text via `pane.get_lines()` + `get_semantic_zones()`; if text starts with `#` and cursor is in OSC 133 `Input` zone, route to agent mode
- [x] T025 [US2] Implement `##` escape — if input zone text starts with `##`, write `#` + rest of text to shell via `pane.writer()` (strip one `#`), do not intercept
- [x] T026 [US2] Implement agent mode UI in the compact bottom panel — display step plan: numbered steps with commands and explanations, highlight current step, show controls (Enter=run, s=skip, q=abort)
- [x] T027 [US2] Implement step execution — on Enter: paste current step's command into terminal via `pane.send_paste()`, transition step to Running state, wait for `Alert::CommandComplete` (or zone transition fallback) to detect completion
- [x] T028 [US2] Implement step completion handling — on `CommandComplete`: capture exit code, transition to next step's Reviewing state; if last step, show "Plan complete" summary
- [x] T029 [US2] Implement skip (`s` key) — skip current step (mark as Skipped), advance to next
- [x] T030 [US2] Implement abort (`q` key) — abort entire plan, close agent UI, return to normal prompt
- [x] T031 [US2] Implement failure handling — on non-zero exit code: pause and show "Step failed (exit N). [r]etry / [s]kip / [q]uit?" prompt
- [x] T032 [US2] Implement fallback completion detection — for shells without 133;D, track `Output → Input` semantic zone transition via `MuxNotification::PaneOutput` as command completion signal
- [x] T033 [US2] Verify agent mode end-to-end — manual test per quickstart.md

**Checkpoint**: Agent mode works: plan, execute, skip, abort, failure handling all functional.

---

## Phase 5: User Story 3 — Polish (Priority: P3)

**Goal**: Markdown rendering in AI pane, loading indicators, theme matching.

- [x] T034 [P] [US3] Implement Markdown rendering in AI pane responses in `wezterm-gui/src/ai_pane.rs` — detect code blocks (triple backtick) and render with syntax highlighting; render bold (`**text**`) as SGR bold, inline code as dimmed, lists as indented bullets
- [x] T035 [P] [US3] Add loading spinner in compact panel in `wezterm-gui/src/termwindow/render/ai_panel.rs` — while LLM query is in flight, show animated dots or spinner character in the panel
- [x] T036 [US3] Ensure all AI UI elements use the terminal's configured color scheme — panel background matches terminal background, text uses terminal foreground, commands use the bright green from the palette

**Checkpoint**: AI pane has rich Markdown rendering. Panel shows loading state. Colors match theme.

---

## Phase 6: Polish & Cross-Cutting

- [x] T037 Run `cargo test --all` to verify no regressions
- [x] T038 Run `cargo fmt --all`
- [x] T039 Run `cargo clippy --package arcterm-ai --package wezterm-gui`
- [x] T040 Test full workflow per quickstart.md (panel + agent + polish)
- [x] T041 Update spec status to Complete

---

## Dependencies & Execution Order

- **Setup (Phase 1)**: No dependencies
- **Foundational (Phase 2)**: Depends on Setup — OSC 133;D + agent logic
- **US1 (Phase 3)**: Depends on Foundational — compact panel rendering
- **US2 (Phase 4)**: Depends on US1 (uses panel for agent UI) + Foundational (uses 133;D for step completion)
- **US3 (Phase 5)**: Depends on US1 (panel) and existing AI pane
- **Polish (Phase 6)**: Depends on all

### Parallel Opportunities
- T006-T008 (133;D handler) can parallel with T009-T011 (agent logic)
- T034, T035 (polish) are parallelizable
- US3 can start after US1 panel is working

---

## Implementation Strategy

### MVP (US1 Only)
1. Setup + Foundational (133;D handler + agent structs)
2. US1: Compact bottom panel with Run/Copy
3. **STOP**: Command panel replaces the full overlay — biggest UX win

### Full Feature
4. US2: Agent mode (`#` prefix + step execution)
5. US3: Polish (Markdown, spinners, theming)
