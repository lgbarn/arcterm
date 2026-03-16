---
phase: ai-integration
plan: "7.1"
wave: 1
dependencies: []
must_haves:
  - AI agent detection via process name matching (claude, codex, gemini, aider)
  - PaneContext struct with ai_type, last_command, last_exit_code, output ring buffer
  - OSC 133 shell integration (prompt start, command start, command end with exit code)
  - Detection cached per-pane with TTL, following NeovimState pattern
files_touched:
  - arcterm-app/src/proc.rs
  - arcterm-app/src/neovim.rs
  - arcterm-app/src/ai_detect.rs
  - arcterm-app/src/context.rs
  - arcterm-app/src/main.rs
  - arcterm-vt/src/handler.rs
  - arcterm-vt/src/processor.rs
tdd: true
---

# Plan 7.1 -- AI Detection Engine + Cross-Pane Context Model

**Wave 1** | No dependencies | Foundation for all Phase 7 features

## Goal

Establish the two foundational data structures that every other Phase 7 feature depends on:
(1) an AI agent detection engine that identifies Claude Code, Codex CLI, Gemini CLI, and
Aider running in terminal panes, and (2) a per-pane context model that captures CWD, last
command, exit code, and recent output -- populated via OSC 133 shell integration sequences.

No rendering changes in this plan. No keybinding changes. Pure data-layer work.

---

<task id="1" files="arcterm-app/src/proc.rs, arcterm-app/src/neovim.rs, arcterm-app/src/ai_detect.rs" tdd="true">
  <action>
    Extract `process_comm()` and `process_args()` from `neovim.rs` into a new shared
    module `arcterm-app/src/proc.rs`. Update `neovim.rs` to import from `proc.rs` instead
    of defining these functions locally. Then create `arcterm-app/src/ai_detect.rs` with:

    1. An `AiAgentKind` enum: `ClaudeCode`, `CodexCli`, `GeminiCli`, `Aider`, `Unknown(String)`.
    2. A `detect_ai_agent(pid: u32) -> Option<AiAgentKind>` function that calls
       `process_comm(pid)` and matches against known binary names (`claude`, `codex`,
       `gemini`). For `aider` (Python entry point), fall back to
       `process_args(pid)[0].ends_with("aider")` since `process_comm()` returns `python3`.
    3. An `AiAgentState` struct mirroring `NeovimState`: fields `kind: Option<AiAgentKind>`,
       `last_check: Instant`, method `check(pid: Option<u32>) -> Self`, method
       `is_fresh() -> bool` with a 5-second TTL (per CONTEXT-7.md).
    4. Unit tests following the `neovim.rs` pattern: `detect_ai_agent(1)` returns `None`
       (PID 1 is not an AI tool); `AiAgentState::check(None)` returns `kind: None`;
       `AiAgentState` is fresh immediately after creation; known binary name matching
       covers all enum variants (test the matching logic with a helper that takes a
       process name string directly, not a PID).

    Register `mod proc;` and `mod ai_detect;` in `arcterm-app/src/main.rs`.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-app -- proc:: ai_detect:: neovim:: --no-fail-fast 2>&1 | tail -20</verify>
  <done>All tests in proc, ai_detect, and neovim modules pass. `process_comm()` and `process_args()` are defined once in `proc.rs` and imported by both `neovim.rs` and `ai_detect.rs`. `detect_ai_agent(1)` returns `None`. Name-matching tests cover claude, codex, gemini, aider.</done>
</task>

<task id="2" files="arcterm-app/src/context.rs, arcterm-vt/src/handler.rs, arcterm-vt/src/processor.rs" tdd="true">
  <action>
    Create `arcterm-app/src/context.rs` defining the `PaneContext` struct:

    ```
    pub struct PaneContext {
        pub ai_type: Option<AiAgentKind>,
        pub last_command: Option<String>,
        pub last_exit_code: Option<i32>,
        pub output_ring: VecDeque<String>,  // capped at `ring_capacity`
        pub ring_capacity: usize,           // default 200
    }
    ```

    Add methods: `new(capacity: usize) -> Self`, `push_output_line(&mut self, line: String)`,
    `set_command(&mut self, cmd: String)`, `set_exit_code(&mut self, code: i32)`,
    `error_context(&self) -> Option<ErrorContext>` (returns last command + exit code +
    last 20 lines if exit_code is non-zero).

    Then add OSC 133 support to the VT layer:

    1. In `arcterm-vt/src/handler.rs`, add three methods to the `Handler` trait with
       default no-op implementations: `shell_prompt_start()`, `shell_command_start()`,
       `shell_command_end(exit_code: i32)`.
    2. In `GridState`, implement these methods. `shell_command_start()` sets a
       `pending_command_start: bool` flag. `shell_command_end(code)` stores `(last_exit_code, code)`
       in a new `pub shell_exit_codes: Vec<i32>` drain buffer (same pattern as
       `completed_blocks`).
    3. In `arcterm-vt/src/processor.rs` `osc_dispatch`, add an arm for `b"133"` that
       parses: `A` -> prompt start, `B` -> command start, `C` -> command executed (no-op),
       `D` with optional `;exit_code` -> command end. Dispatch to the handler methods.
    4. Unit tests: feed OSC 133 sequences through the processor and verify `GridState`
       fields are set correctly. Test: `OSC 133;A ST` is no-op on grid, `OSC 133;D;1 ST`
       sets exit code 1, `OSC 133;D ST` without code defaults to 0.

    Register `mod context;` in `arcterm-app/src/main.rs`.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-app -- context:: --no-fail-fast 2>&1 | tail -20 && cargo test --package arcterm-vt -- osc133 --no-fail-fast 2>&1 | tail -20</verify>
  <done>PaneContext unit tests pass: ring buffer respects capacity, `push_output_line` evicts oldest when full, `error_context()` returns `Some` when exit code is non-zero and `None` when zero. OSC 133 VT tests pass: D;1 sets exit code, D without code defaults to 0, A/B/C are accepted without error.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs" tdd="false">
  <action>
    Wire the detection engine and context model into AppState:

    1. Add `pane_contexts: HashMap<PaneId, PaneContext>` to `AppState`. Initialize a
       `PaneContext` for each pane on creation (in the same locations where `nvim_states`
       entries are created/removed -- search for `nvim_states.insert` and `nvim_states.remove`
       to find all sites).
    2. Add `ai_states: HashMap<PaneId, ai_detect::AiAgentState>` to `AppState`, following
       the exact pattern of `nvim_states`. Update AI detection lazily on `NavigatePane`
       events using the same TTL-refresh pattern as Neovim detection (search for
       `needs_refresh` / `NeovimState::check` in main.rs and replicate for `AiAgentState`).
    3. Add `last_ai_pane: Option<PaneId>` to `AppState`. Update it whenever focus moves
       to a pane where `ai_states[id].kind.is_some()`.
    4. In the `process_pty_output` call site (where PTY bytes are fed to `Terminal`),
       after calling `terminal.process_pty_output(bytes)`, drain `GridState.shell_exit_codes`
       (via a new `Terminal::take_exit_codes() -> Vec<i32>` method) and update the
       corresponding `PaneContext.last_exit_code`.
    5. Clean up `pane_contexts`, `ai_states`, and `last_ai_pane` entries in every
       location where `nvim_states.remove(&id)` is called.

    This task does NOT add any new keybindings or rendering. It wires data flow only.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app 2>&1 | tail -10</verify>
  <done>`cargo build --package arcterm-app` compiles without errors. `pane_contexts` and `ai_states` are initialized, updated, and cleaned up at all the same sites as `nvim_states`. `last_ai_pane` is set on focus change to AI panes.</done>
</task>
