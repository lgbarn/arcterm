---
plan: "7.1"
phase: ai-integration
status: complete
commits:
  - 6252015 shipyard(phase-7): add AI detection engine and shared proc module
  - 54a7129 shipyard(phase-7): add PaneContext struct and OSC 133 shell integration
  - 88c2d8b shipyard(phase-7): wire AI detection and PaneContext into AppState
---

# Summary — Plan 7.1: AI Detection Engine + Cross-Pane Context Model

**Wave 1 | No dependencies | Foundation for all Phase 7 features**

All three tasks completed sequentially with TDD where required. 41 tests pass
across the three affected packages (26 in arcterm-app for Task 1, 7 + 8 = 15
new tests in Task 2, build-only for Task 3). No deviations from the plan.

---

## Task 1: AI Detection Engine

**Files:** `arcterm-app/src/proc.rs` (new), `arcterm-app/src/ai_detect.rs` (new),
`arcterm-app/src/neovim.rs` (updated), `arcterm-app/src/main.rs` (updated)

### What was done

- Extracted `process_comm()` and `process_args()` from `neovim.rs` into a new
  `proc.rs` module with `pub` visibility. Both platform-specific variants
  (macOS `libc::proc_name` / `sysctl`, Linux `/proc/<pid>/comm`) were moved
  verbatim with no behaviour changes.
- Updated `neovim.rs` to `use crate::proc::{process_args, process_comm}` —
  the function bodies are gone from that file.
- Created `ai_detect.rs` with:
  - `AiAgentKind` enum: `ClaudeCode`, `CodexCli`, `GeminiCli`, `Aider`, `Unknown(String)`
  - `match_ai_name(name: &str) -> Option<AiAgentKind>` — pure string-based
    matching used by tests and `detect_ai_agent`
  - `detect_ai_agent(pid: u32) -> Option<AiAgentKind>` — reads `process_comm`;
    for python3 processes falls back to `process_args[0].ends_with("aider")`
  - `AiAgentState { kind, last_check }` with `check(pid: Option<u32>)` and
    `is_fresh()` (5-second TTL)
- Registered `mod proc;`, `mod ai_detect;`, and `mod context;` in `main.rs`.

### Deviations

None. A stub `context.rs` was created alongside to satisfy the `mod context;`
declaration needed for Task 1 compilation; Task 2 replaced it with the real
implementation.

### Tests (26 total, all pass)

- `proc::tests::proc_comm_does_not_panic`
- `proc::tests::proc_args_does_not_panic`
- `ai_detect::tests::detect_ai_returns_none_for_pid_1`
- `ai_detect::tests::ai_agent_state_check_none_returns_none_kind`
- `ai_detect::tests::ai_agent_state_is_fresh_after_creation`
- `ai_detect::tests::name_matching_claude` (also covers `claude-code` prefix)
- `ai_detect::tests::name_matching_codex`
- `ai_detect::tests::name_matching_gemini`
- `ai_detect::tests::name_matching_aider`
- `ai_detect::tests::name_matching_cursor`
- `ai_detect::tests::name_matching_copilot`
- `ai_detect::tests::name_matching_unknown_returns_none`
- All pre-existing `neovim::tests::*` (14 tests) continue to pass.

---

## Task 2: PaneContext + OSC 133

**Files:** `arcterm-app/src/context.rs` (new), `arcterm-vt/src/handler.rs` (updated),
`arcterm-vt/src/processor.rs` (updated)

### What was done

- Created `context.rs` with:
  - `ErrorContext { command, exit_code, output_tail }` — snapshot for non-zero exits
  - `PaneContext { ai_type, last_command, last_exit_code, output_ring, ring_capacity }`
  - `PaneContext::new(capacity)`, `push_output_line`, `set_command`, `set_exit_code`
  - `error_context() -> Option<ErrorContext>` — returns `Some` only when
    `last_exit_code` is non-zero; `output_tail` is capped at 20 lines
- Added three methods to the `Handler` trait in `handler.rs` with default
  no-op implementations: `shell_prompt_start()`, `shell_command_start()`,
  `shell_command_end(exit_code: i32)`
- Added to `GridState`:
  - `shell_exit_codes: Vec<i32>` — drain buffer for OSC 133 D events
  - `pending_command_start: bool` — set by B, cleared by D
  - `take_exit_codes() -> Vec<i32>` — drains the buffer
- Implemented the three trait methods on `GridState`
- Added `dispatch_osc133` to `processor.rs` dispatching `A`/`B`/`C`/`D`
  sub-commands with optional exit code parsing for `D`
- Added `b"133"` arm to `osc_dispatch` in `processor.rs`

### Deviations

None.

### Tests (15 total, all pass)

**context::tests (7):**
- `context_new_empty`
- `context_ring_buffer_fills`
- `context_ring_buffer_evicts_oldest`
- `context_error_context_none_on_success`
- `context_error_context_none_when_no_exit_code`
- `context_error_context_some_on_failure`
- `context_error_context_tail_capped_at_20`

**osc133_tests (8, in arcterm-vt):**
- `osc133_a_is_noop_on_grid`
- `osc133_b_sets_pending_command_start`
- `osc133_c_is_noop`
- `osc133_d_with_code_sets_exit_code`
- `osc133_d_without_code_defaults_to_zero`
- `osc133_d_multiple_codes_accumulate`
- `osc133_take_exit_codes_drains_buffer`
- `osc133_full_sequence_a_b_d`

---

## Task 3: Wire into AppState

**Files:** `arcterm-app/src/main.rs` (updated), `arcterm-app/src/terminal.rs` (updated)

### What was done

- Added to `AppState`:
  - `ai_states: HashMap<PaneId, AiAgentState>` — initialized empty; populated
    on pane creation and refreshed lazily on `NavigatePane` events
  - `pane_contexts: HashMap<PaneId, PaneContext>` — initialized with capacity
    200; populated on pane creation
  - `last_ai_pane: Option<PaneId>` — set to the pane ID whenever an
    AI-detected pane receives PTY data or when AI detection refreshes on focus
- Added `Terminal::take_exit_codes() -> Vec<i32>` delegating to
  `GridState::take_exit_codes()`
- In `resumed()`: pre-built `ai_states` and `pane_contexts` from the initial
  `panes` HashMap keys before constructing `AppState`
- In `spawn_pane_with_cwd()`: inserts into `ai_states` and `pane_contexts`
  alongside existing `auto_detectors` and `structured_blocks`
- In `restore_workspace()`: clears all new maps alongside `nvim_states` and
  resets `last_ai_pane = None`
- In the pane restore loop inside `restore_workspace()`: inserts defaults for
  each restored pane ID
- PTY drain loop: after `process_pty_output`, drains `take_exit_codes()` into
  `pane_contexts[id].last_exit_code`; updates `last_ai_pane` when an
  AI-detected pane receives data
- `NavigatePane` handler: lazy AI detection refresh with 5-second TTL,
  replicating the Neovim detection pattern exactly
- All three `nvim_states.remove` sites (PTY channel closed, tab close, pane
  close) extended to also remove `ai_states`, `pane_contexts`, and clear
  `last_ai_pane` if it matches the removed pane

### Deviations

None. No new keybindings or rendering changes were introduced.

### Verification

```
cargo build --package arcterm-app 2>&1 | tail -10
```

Build succeeds with 3 dead_code warnings (expected — the data layer is wired
but not yet consumed by rendering; those warnings will be resolved by Phase 7.2
when the UI reads `last_ai_pane` and `pane_contexts`).

---

## Final State

| Component | Status |
|---|---|
| `arcterm-app/src/proc.rs` | New — shared process introspection |
| `arcterm-app/src/ai_detect.rs` | New — AI agent detection engine |
| `arcterm-app/src/context.rs` | New — per-pane context model |
| `arcterm-app/src/neovim.rs` | Updated — imports from proc.rs |
| `arcterm-app/src/terminal.rs` | Updated — take_exit_codes() |
| `arcterm-vt/src/handler.rs` | Updated — OSC 133 trait methods + GridState fields |
| `arcterm-vt/src/processor.rs` | Updated — OSC 133 dispatch |
| `arcterm-app/src/main.rs` | Updated — AppState wiring |

All 41 tests pass. `cargo build --package arcterm-app` is clean.
