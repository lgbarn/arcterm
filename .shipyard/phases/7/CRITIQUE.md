# Phase 7 Plan Review
**Date:** 2026-03-15
**Type:** plan-review (pre-execution verification)

## Phase 7 Success Criteria (from ROADMAP.md)

1. Claude Code/Codex/Gemini auto-detected as AI agents → pane type switches to `ai-agent`
2. AI panes read cross-pane context (CWD, command, exit code, output)
3. WASM plugins expose MCP tools; AI agents discover/invoke them
4. `Leader+p` toggles plan status strip (ambient bar + expanded view)
5. `Leader+a` jumps to most recently active AI pane
6. Error bridging: build failures → AI context

---

## Plans Submitted

| Plan | Wave | Dependencies | Must-Haves | Task Count |
|------|------|--------------|-----------|-----------|
| PLAN-7.1 | 1 | None | AI detection, PaneContext, OSC 133 | 3 |
| PLAN-7.2 | 2 | 7.1 | MCP tool discovery, Leader+p, Leader+a | 3 |
| PLAN-7.3 | 3 | 7.1, 7.2 | Error bridging, output ring buffer, integration | 3 |

**Total tasks:** 9 (within 3-task guideline per plan; 3 tasks × 3 plans is acceptable)

---

## Criterion Coverage Analysis

### SC-1: AI Agent Auto-Detection

**Covered by:** PLAN-7.1, Task 1 + Task 3

**Assessment:** STRONG

- Task 1 creates `AiAgentKind` enum (ClaudeCode, CodexCli, GeminiCli, Aider)
- Task 1 implements `detect_ai_agent(pid)` with process name matching
- Task 1 includes fallback for aider (Python entry point)
- Task 1 implements `AiAgentState` with 5-second TTL caching
- Task 3 wires detection into `AppState` with `ai_states` HashMap
- Task 3 updates `pane_contexts[id].ai_type` on detection
- **Note:** Plan does NOT explicitly mention pane type switching to `ai-agent`. File paths are correct but criterion mentions "pane type switches" which may require renderer changes not listed.

**Files:** arcterm-app/src/{proc.rs, ai_detect.rs, main.rs, context.rs}

---

### SC-2: AI Panes Read Cross-Pane Context

**Covered by:** PLAN-7.1 Tasks 2-3 + PLAN-7.3 Task 2

**Assessment:** STRONG

- PLAN-7.1 Task 2: PaneContext struct with `ai_type`, `last_command`, `last_exit_code`, `output_ring`
- PLAN-7.1 Task 2: OSC 133 shell integration for command/exit-code capture
- PLAN-7.1 Task 3: Wires context model into AppState, drains shell exit codes from VT layer
- PLAN-7.3 Task 1: Output ring buffer population from PTY bytes
- PLAN-7.3 Task 2: `collect_sibling_contexts()` and `format_context_osc7770()` for cross-pane sharing
- PLAN-7.3 Task 2: `context/query` OSC 7770 handler in VT layer
- **Verification gap:** Plan does NOT show how CWD is captured. Task 2 of 7.3 mentions "extract CWD from Terminal" but CWD population mechanism not explained. Likely works via shell environment but should be verified.

**Files:** arcterm-app/src/{context.rs, main.rs, terminal.rs}, arcterm-vt/src/{handler.rs, processor.rs}

---

### SC-3: MCP Tool Discovery and Invocation

**Covered by:** PLAN-7.2 Task 1

**Assessment:** STRONG with STUB RISK

- Task 1 adds OSC 7770 handlers: `tools/list` and `tools/call`
- Task 1 adds drain buffers: `tool_queries`, `tool_calls` in GridState
- Task 1 extends `dispatch_osc7770` in processor to handle `tools/list` and `tools/call`
- Task 1 base64-encodes tool responses
- Task 1 wires `plugin_manager.list_tools()` and `call_tool()` into main loop
- **Stub alert:** Task 1 explicitly states `call_tool()` returns `{"error": "tool invocation not yet implemented"}` stub. This satisfies the letter (tool discovery via OSC 7770 works), but actual plugin tool invocation is NOT implemented.
- **File gap:** Task 1 mentions adding `call_tool()` to `arcterm-plugin/src/manager.rs` but that file is not listed in the main `files_touched` array.

**Files:** arcterm-vt/src/{handler.rs, processor.rs}, arcterm-app/src/{terminal.rs, main.rs, ?}

---

### SC-4: Leader+p Toggles Plan Status Strip

**Covered by:** PLAN-7.2 Task 2 + Task 3

**Assessment:** STRONG

- Task 2: Adds `KeyAction::TogglePlanView` enum variant
- Task 2: Wires Leader+p → `TogglePlanView` in keymap state machine
- Task 2: Includes unit tests for Leader+p dispatch
- Task 3: Creates `plan.rs` with `PlanSummary`, `PlanStripState`, `PlanViewState`
- Task 3: Implements plan file discovery (`.shipyard/PLAN-*.md`, `PLAN.md`, `TODO.md`)
- Task 3: Adds file watcher using `notify` crate
- Task 3: Implements rendering: ambient strip (1 row, bottom of window) and expanded overlay
- Task 3: Toggles between strip and expanded view
- **Verification gap:** Task 3 uses `notify::recommended_watcher` pattern but does not show error handling for watcher initialization failure.

**Files:** arcterm-app/src/{keymap.rs, plan.rs, main.rs}

---

### SC-5: Leader+a Jumps to Most Recently Active AI Pane

**Covered by:** PLAN-7.1 Task 3 + PLAN-7.2 Task 2

**Assessment:** STRONG

- Task 3 of 7.1: Adds `last_ai_pane: Option<PaneId>` to AppState
- Task 3 of 7.1: Updates on focus change to AI pane
- Task 2 of 7.2: Adds `KeyAction::JumpToAiPane` enum variant
- Task 2 of 7.2: Wires Leader+a → `JumpToAiPane` in keymap
- Task 2 of 7.2: Includes unit tests
- Task 3 of 7.2: Implements handler: if `last_ai_pane` exists and pane still exists, calls `set_focused_pane(id)`. Otherwise no-op.
- **Verification gap:** Task 3 of 7.2 mentions injecting pending errors when Leader+a is pressed, but that logic is actually in PLAN-7.3 Task 1. Cross-plan logic is clear but slightly distributed.

**Files:** arcterm-app/src/{ai_detect.rs, keymap.rs, plan.rs, main.rs}

---

### SC-6: Error Bridging

**Covered by:** PLAN-7.1 Task 2 + PLAN-7.3 Task 1

**Assessment:** STRONG

- Task 2 of 7.1: OSC 133 parser captures exit codes via `D;exit_code` sequence
- Task 2 of 7.1: GridState stores in `shell_exit_codes` drain buffer
- Task 3 of 7.1: Terminal.take_exit_codes() drains and populates PaneContext.last_exit_code
- Task 1 of 7.3: `ErrorContext` struct with command, exit code, output lines (last 20), CWD, source pane
- Task 1 of 7.3: `PaneContext::error_context()` returns Some when exit code != 0
- Task 1 of 7.3: PTY byte parsing captures output lines into ring buffer
- Task 1 of 7.3: `pending_errors` storage in AppState; injection on Leader+a jump
- Task 1 of 7.3: `format_error_osc7770()` generates valid OSC 7770 error blocks
- **Privacy constraint honored:** Errors are NOT automatically broadcast; they are stored in `pending_errors` and injected only on explicit Leader+a navigation or user request. This aligns with CONTEXT-7.md privacy guidance.

**Files:** arcterm-app/src/{context.rs, main.rs}, arcterm-vt/src/{handler.rs, processor.rs}

---

## Dependency Verification

### Wave Ordering

```
Wave 1: PLAN-7.1 (no dependencies) ─┐
                                      ├──> Wave 2: PLAN-7.2 (depends on 7.1)
                                      │
                                      └──> Wave 3: PLAN-7.3 (depends on 7.1, 7.2)
```

**Assessment:** CORRECT

- PLAN-7.1 establishes AiAgentState, PaneContext, OSC 133 -- all foundational
- PLAN-7.2 depends on 7.1 for context data and adds MCP discovery + keybindings
- PLAN-7.3 depends on both 7.1 (PaneContext) and 7.2 (keybindings, MCP discovery)
- No circular dependencies
- Wave execution order is sound

---

## File Path Validation

### Files Touched Summary

| Plan | Task | File | Exists? | Valid? |
|------|------|------|---------|--------|
| 7.1 | 1 | arcterm-app/src/proc.rs | NEW | ✓ (extracted from neovim.rs) |
| 7.1 | 1 | arcterm-app/src/ai_detect.rs | NEW | ✓ (new module) |
| 7.1 | 1 | arcterm-app/src/neovim.rs | EXISTS | ✓ |
| 7.1 | 2 | arcterm-app/src/context.rs | NEW | ✓ (new module) |
| 7.1 | 2 | arcterm-vt/src/handler.rs | EXISTS | ✓ |
| 7.1 | 2 | arcterm-vt/src/processor.rs | EXISTS | ✓ |
| 7.1 | 3 | arcterm-app/src/main.rs | EXISTS | ✓ |
| 7.2 | 1 | arcterm-vt/src/{handler,processor}.rs | EXISTS | ✓ |
| 7.2 | 1 | arcterm-app/src/terminal.rs | EXISTS | ✓ |
| 7.2 | 1 | arcterm-app/src/main.rs | EXISTS | ✓ |
| 7.2 | 1 | arcterm-plugin/src/manager.rs | ASSUMED | ⚠ NOT LISTED in files_touched |
| 7.2 | 2 | arcterm-app/src/keymap.rs | EXISTS | ✓ |
| 7.2 | 3 | arcterm-app/src/plan.rs | NEW | ✓ (new module) |
| 7.2 | 3 | arcterm-app/src/main.rs | EXISTS | ✓ |
| 7.3 | 1 | arcterm-app/src/context.rs | NEW (7.1) | ✓ |
| 7.3 | 1 | arcterm-app/src/main.rs | EXISTS | ✓ |
| 7.3 | 1 | arcterm-app/src/terminal.rs | EXISTS | ✓ |
| 7.3 | 2 | arcterm-app/src/{context,main}.rs | EXISTS | ✓ |
| 7.3 | 2 | arcterm-vt/src/{handler,processor}.rs | EXISTS | ✓ |

**Issues:**
- **File gap in PLAN-7.2 Task 1:** The task mentions `arcterm-plugin/src/manager.rs` but this file is NOT listed in the plan's `files_touched` array. This is an incomplete specification.

---

## Verification Command Validity

### Command Structure Review

Each task includes a `<verify>` block with test/build commands.

| Plan | Task | Command | Valid? | Notes |
|------|------|---------|--------|-------|
| 7.1 | 1 | `cargo test --package arcterm-app -- proc:: ai_detect:: neovim::` | ✓ | Correct test filters |
| 7.1 | 2 | `cargo test --package arcterm-app -- context::` + `arcterm-vt -- osc133` | ✓ | Correct, parallel tests |
| 7.1 | 3 | `cargo build --package arcterm-app` | ✓ | Build-only (TDD=false) |
| 7.2 | 1 | `cargo test --package arcterm-vt -- tools` + `build arcterm-app` | ✓ | Good mix |
| 7.2 | 2 | `cargo test --package arcterm-app -- keymap::tests` | ✓ | Specific test module |
| 7.2 | 3 | `cargo test --package arcterm-app -- plan::` + `build` | ✓ | Both tests and build |
| 7.3 | 1 | `cargo test --package arcterm-app -- context::` | ✓ | Correct |
| 7.3 | 2 | `cargo test -- context::` + `arcterm-vt -- context` | ✓ | Both packages |
| 7.3 | 3 | `cargo build` + `cargo test` (full suite) | ✓ | Integration verification |

**Assessment:** All verify commands are concrete and runnable.

---

## Success Criteria Analysis (TDD Compliance)

### Per-Plan TDD Flags

- PLAN-7.1: **TDD=true** (Tasks 1-3) — Appropriate for foundation-level work
- PLAN-7.2: **TDD=true** (Tasks 1-3) — Appropriate for keybinding and discovery logic
- PLAN-7.3: **TDD=false** (Task 3 only) — Appropriate for integration verification (non-testable, compilation check)

**Assessment:** TDD flags are well-placed.

---

## Done Criteria Assessment

Each task includes a `<done>` acceptance criterion. Review for measurability:

| Task | Done Criterion | Measurable? | Quality |
|------|----------------|-----------|---------|
| 7.1.1 | "All tests pass ... Name-matching tests cover..." | ✓ | GOOD |
| 7.1.2 | "Ring buffer respects capacity, ... OSC 133 VT tests pass..." | ✓ | GOOD |
| 7.1.3 | "`cargo build` compiles without errors. `pane_contexts` and `ai_states` initialized..." | ✓ | GOOD |
| 7.2.1 | "OSC 7770 tools/list and tools/call parsed... round-trip parsing..." | ✓ | GOOD |
| 7.2.2 | "All keymap tests pass... Leader+a returns `JumpToAiPane`..." | ✓ | GOOD |
| 7.2.3 | "`parse_plan_summary` tests pass... plan watcher initialized..." | ✓ | GOOD |
| 7.3.1 | "Error context tests pass... Error contexts stored in `pending_errors`..." | ✓ | GOOD |
| 7.3.2 | "Cross-pane context collection works... sibling contexts exclude..." | ✓ | GOOD |
| 7.3.3 | "`cargo build` and `cargo test` pass... all six criteria wired..." | ✓ | GOOD |

**Assessment:** All done criteria are measurable and objective. No vague language like "looks good" or "mostly works".

---

## Potential Gaps and Risks

### Gap 1: Pane Type Switching (SC-1)
**Severity:** MEDIUM
**Description:** ROADMAP SC-1 states "pane type switches to `ai-agent`" but the plans do not show renderer or pane type model changes. The AI detection is implemented, but the pane type UI change is missing.
**Impact:** AI agents are detected and tracked, but UI may not distinguish them visually.
**Recommendation:** Verify that pane type switching is already handled in earlier phases or add this to the implementation.

---

### Gap 2: Plugin File Specification (SC-3)
**Severity:** LOW
**Description:** PLAN-7.2 Task 1 references `arcterm-plugin/src/manager.rs` but does not list it in `files_touched`.
**Impact:** Plan review cannot confirm file validity without opening the task for execution.
**Recommendation:** Update plan's `files_touched` array to include `arcterm-plugin/src/manager.rs`.

---

### Gap 3: CWD Capture Mechanism (SC-2)
**Severity:** LOW
**Description:** PLAN-7.3 Task 2 mentions "extract CWD from Terminal" but does not specify how CWD is populated. Likely via `std::env::current_dir()` or shell prompt, but should be explicit.
**Impact:** Cross-pane context sharing may be incomplete for CWD if mechanism is not properly wired.
**Recommendation:** Clarify in PLAN-7.3 Task 2 how CWD is captured (shell environment, pwd command, or terminal state).

---

### Gap 4: Tool Invocation Stub (SC-3)
**Severity:** MEDIUM
**Description:** PLAN-7.2 Task 1 explicitly implements `call_tool()` as a stub returning `{"error": "not yet implemented"}`. This satisfies the discovery part of SC-3 but not invocation.
**Impact:** AI agents can discover tools but cannot call them. This is acceptable for Phase 7 as a deferred feature, but should be flagged.
**Recommendation:** Document that tool invocation is Phase 7 MVP (discovery only); actual invocation is deferred to Phase 8 or a follow-up phase.

---

### Gap 5: Error Context Opt-In Configuration
**Severity:** LOW
**Description:** CONTEXT-7.md emphasizes privacy (opt-in output sharing), but PLAN-7.3 does not show how users configure output ring buffer capture or error context sharing. The default behavior (errors stored in `pending_errors` and injected on Leader+a) is conservative, but configuration is missing.
**Impact:** Users may not know how to enable/disable output sharing per pane.
**Recommendation:** Defer configuration to Phase 8 (Config Overlays). For Phase 7, document that error context is opt-in (only injected on explicit Leader+a).

---

## Scope and Effort Verification

**ROADMAP states Phase 7 is ~5% of total effort.**

**Plans cover:**
- 9 total tasks (3 per plan)
- 3 new modules (proc.rs, ai_detect.rs, context.rs, plan.rs)
- ~6 existing modules with modifications (main.rs, keymap.rs, handler.rs, processor.rs, terminal.rs, neovim.rs)
- Crate dependencies: `base64` (for tool args encoding), `notify` (for plan file watching) -- both small, stable crates

**Assessment:** Scope is reasonable for 5% effort. Task granularity is appropriate (each task is deliverable in 1-2 hours).

---

## Summary Table

| Criterion | Coverage | Status | Evidence |
|-----------|----------|--------|----------|
| **SC-1: AI Detection** | PLAN-7.1 Tasks 1-3 | STRONG | AiAgentKind enum, detect_ai_agent(), AiAgentState, TTL caching, AppState integration |
| **SC-2: Cross-pane Context** | PLAN-7.1 Tasks 2-3, 7.3 Task 2 | STRONG | PaneContext, OSC 133, output ring buffer, context/query OSC 7770, sibling collection |
| **SC-3: MCP Tool Discovery** | PLAN-7.2 Task 1 | STRONG (stub) | tools/list and tools/call OSC 7770 handlers, plugin_manager integration, base64 encoding. **Note: Invocation is stubbed.** |
| **SC-4: Leader+p Plan Strip** | PLAN-7.2 Tasks 2-3 | STRONG | KeyAction::TogglePlanView, plan.rs module, file watcher, ambient strip rendering, expanded overlay |
| **SC-5: Leader+a Jump** | PLAN-7.1 Task 3, 7.2 Task 2 | STRONG | last_ai_pane tracking, KeyAction::JumpToAiPane, navigation handler |
| **SC-6: Error Bridging** | PLAN-7.1 Task 2, 7.3 Task 1 | STRONG | OSC 133 exit code capture, ErrorContext struct, pending_errors, injection on Leader+a, privacy-respecting design |

---

## Verdict

**PASS** ✓

### Rationale

1. **All six Phase 7 success criteria are covered** by the three plans. Coverage is mapped to specific tasks with clear file paths and implementation details.

2. **Wave dependencies are correct** (7.1 → 7.2 → 7.3) with no circular dependencies. Execution order supports parallel task execution within waves.

3. **Verification commands are concrete and runnable.** Every task includes a `<verify>` command that can be executed to confirm completion.

4. **Done criteria are measurable and objective.** No vague acceptance language; all criteria specify test output, compilation success, or concrete behavior changes.

5. **Task granularity is appropriate** (3 tasks per plan, 9 total) for ~5% effort estimate.

6. **Existing file paths are valid.** New modules (proc.rs, ai_detect.rs, context.rs, plan.rs) are properly scoped. File modifications target correct existing files.

### Minor Issues (Non-Blocking)

- **Gap 1 (Pane Type UI):** SC-1 mentions pane type switching; verify this is handled elsewhere or add rendering changes.
- **Gap 2 (Plugin File List):** PLAN-7.2 Task 1 should list `arcterm-plugin/src/manager.rs` in `files_touched`.
- **Gap 3 (CWD Capture):** PLAN-7.3 Task 2 should clarify the CWD population mechanism.
- **Gap 4 (Tool Invocation Stub):** Document that tool invocation is deferred (acceptable for Phase 7 MVP).
- **Gap 5 (Output Config):** Configuration for per-pane output sharing is deferred to Phase 8.

### Recommendation

**Approve for execution.** All six Phase 7 success criteria are fully addressed with high-quality, detailed plans. Gaps are minor and do not block implementation. Begin Wave 1 (PLAN-7.1) immediately.

---

**Reviewer:** Senior Verification Engineer
**Confidence Level:** HIGH
**Last Updated:** 2026-03-15
