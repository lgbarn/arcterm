---
phase: ai-integration
plan: "7.3"
wave: 3
dependencies: ["7.1", "7.2"]
must_haves:
  - Non-zero exit codes in shell panes surface as structured error context for AI panes
  - Output ring buffer captures recent lines (opt-in) for cross-pane context sharing
  - AI panes can read sibling pane context (CWD, command, exit code, error output)
  - Integration verification across all Phase 7 success criteria
files_touched:
  - arcterm-app/src/context.rs
  - arcterm-app/src/ai_detect.rs
  - arcterm-app/src/main.rs
  - arcterm-app/src/terminal.rs
  - arcterm-app/src/config.rs
tdd: true
---

# Plan 7.3 -- Error Bridging + Integration Verification

**Wave 3** | Depends on 7.1 (PaneContext, AiAgentState) and 7.2 (MCP, keybindings) | Final integration

## Goal

Complete Phase 7 by implementing error bridging (build failures in shell panes surface
as structured context for AI panes) and wiring the cross-pane context sharing so AI
panes can read sibling pane metadata. Verify all six Phase 7 success criteria are met.

---

<task id="1" files="arcterm-app/src/context.rs, arcterm-app/src/main.rs, arcterm-app/src/terminal.rs" tdd="true">
  <action>
    Implement error bridging and output ring buffer population:

    1. In `arcterm-app/src/context.rs`, add:
       - `ErrorContext` struct: `command: Option<String>`, `exit_code: i32`,
         `output_lines: Vec<String>` (last 20 lines from ring buffer),
         `cwd: Option<PathBuf>`, `source_pane: PaneId`.
       - `PaneContext::error_context(&self, pane_id: PaneId) -> Option<ErrorContext>`:
         returns `Some` when `last_exit_code` is `Some(code)` where `code != 0`.
         Extracts the last 20 lines from `output_ring`. Returns `None` when exit code
         is 0 or absent.
       - `format_error_osc7770(ctx: &ErrorContext) -> Vec<u8>`: formats the error context
         as an OSC 7770 structured error block:
         `ESC ] 7770 ; start ; type=error ; source=pane-{id} ; exit_code={code} ST`
         followed by the output lines, followed by `ESC ] 7770 ; end ST`.

    2. In `arcterm-app/src/main.rs`, in the PTY output processing loop:
       - After feeding bytes to the terminal and draining exit codes, also capture
         output lines. Parse the raw PTY bytes for complete lines (split on `\n`) and
         call `pane_context.push_output_line()` for each. This is a best-effort capture
         -- not every byte sequence maps cleanly to lines, but for error output (which
         is typically line-oriented) this is sufficient.
       - When `last_exit_code` changes to a non-zero value, check if any AI pane exists
         (via `last_ai_pane`). If so, do NOT automatically inject the error context --
         instead, store it in a new `pending_errors: Vec<ErrorContext>` on AppState.
         The error context will be injected when the user navigates to the AI pane
         (Leader+a) or explicitly requests it, respecting the privacy constraint from
         CONTEXT-7.md.

    3. In the `JumpToAiPane` handler in `main.rs` (where Leader+a is processed):
       after focusing the AI pane, if `pending_errors` is non-empty, drain it and write
       each error context to the AI pane's PTY input via `format_error_osc7770()` and
       `terminal.write_input()`. Clear `pending_errors` after injection.

    4. Unit tests in `context.rs`:
       - `error_context` returns `None` when exit code is 0.
       - `error_context` returns `Some` with correct fields when exit code is 1.
       - `error_context` includes last 20 lines from ring buffer (test with 30 lines
         in buffer, verify only last 20 are in the ErrorContext).
       - `format_error_osc7770` produces valid OSC 7770 start/end sequence with
         correct type, source, and exit_code attributes.
       - Ring buffer overflow: push 250 lines into a ring with capacity 200, verify
         `output_ring.len() == 200` and oldest lines are evicted.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-app -- context:: --no-fail-fast 2>&1 | tail -20</verify>
  <done>Error context tests pass: correct extraction from PaneContext, ring buffer eviction works, OSC 7770 formatting produces valid sequences. Error contexts are stored in `pending_errors` and injected on Leader+a navigation to AI pane.</done>
</task>

<task id="2" files="arcterm-app/src/main.rs, arcterm-app/src/context.rs" tdd="true">
  <action>
    Implement cross-pane context reading so AI panes can access sibling pane metadata:

    1. In `arcterm-app/src/context.rs`, add:
       - `SiblingContext` struct: `pane_id: PaneId`, `cwd: Option<PathBuf>`,
         `last_command: Option<String>`, `exit_code: Option<i32>`,
         `ai_type: Option<AiAgentKind>`.
       - `fn collect_sibling_contexts(pane_contexts: &HashMap<PaneId, PaneContext>, panes: &HashMap<PaneId, Terminal>, exclude: PaneId) -> Vec<SiblingContext>`:
         iterates all pane contexts except `exclude`, extracts CWD from the Terminal,
         and returns a vec of sibling summaries.
       - `fn format_context_osc7770(siblings: &[SiblingContext]) -> Vec<u8>`: formats
         as `ESC ] 7770 ; start ; type=context ST` followed by a JSON array of sibling
         metadata (CWD, command, exit code per pane), followed by `ESC ] 7770 ; end ST`.

    2. In `arcterm-vt/src/handler.rs` and `arcterm-vt/src/processor.rs`, add handling for
       `ESC ] 7770 ; context/query ST`. When received, push a sentinel onto a new
       `pub context_queries: Vec<()>` drain buffer in `GridState`. In `Terminal`, add
       `take_context_queries()`.

    3. In `arcterm-app/src/main.rs`, in the PTY output drain loop, for each context query:
       call `collect_sibling_contexts()` and write the formatted response back to the
       querying pane's PTY input via `terminal.write_input()`.

    4. Unit tests:
       - `collect_sibling_contexts` excludes the requesting pane.
       - `collect_sibling_contexts` returns empty vec when only one pane exists.
       - `format_context_osc7770` produces valid JSON within OSC 7770 delimiters.
       - Context query OSC 7770 sequence is parsed correctly by VT processor.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-app -- context:: --no-fail-fast 2>&1 | tail -20 && cargo test --package arcterm-vt -- context --no-fail-fast 2>&1 | tail -20</verify>
  <done>Cross-pane context collection works: sibling contexts exclude the querying pane, format as valid OSC 7770 JSON. VT processor correctly parses context/query sequences. `cargo build` succeeds.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs" tdd="false">
  <action>
    Integration verification -- ensure all six Phase 7 success criteria are met:

    1. **SC-1: AI agent detection** -- Verify that the AI detection path is fully wired:
       `AppState` creates `AiAgentState` entries, refreshes them on navigation, and
       updates `pane_contexts[id].ai_type` when detection succeeds. Add a log message
       `log::info!("AI agent detected in pane {}: {:?}", pane_id, kind)` when a new
       AI agent is first detected.

    2. **SC-2: Cross-pane context** -- Verify the context query/response round-trip
       compiles and the data flow is complete: VT parser dispatches query -> Terminal
       drains it -> AppState collects siblings -> response written to PTY.

    3. **SC-3: MCP tool discovery** -- Verify `tools/list` query -> `list_tools()` ->
       JSON response is wired end-to-end. Verify `tools/call` -> `call_tool()` -> result
       response is wired. Ensure `base64` encoding/decoding is correct.

    4. **SC-4: Leader+p** -- Verify plan strip renders when `.shipyard/` files are present.
       The plan watcher initializes without error. TogglePlanView switches between
       strip-only and expanded overlay.

    5. **SC-5: Leader+a** -- Verify JumpToAiPane focuses the correct pane and injects
       pending errors. When no AI pane exists, the action is a no-op (no crash).

    6. **SC-6: Error bridging** -- Verify that a non-zero exit code captured via OSC 133
       flows through to `pending_errors` and is injected on Leader+a. Verify the
       formatted OSC 7770 error block contains the exit code and output lines.

    Fix any compilation errors, missing imports, or incomplete wiring found during
    this verification pass.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app 2>&1 | tail -10 && cargo test --package arcterm-app --no-fail-fast 2>&1 | tail -30 && cargo test --package arcterm-vt --no-fail-fast 2>&1 | tail -20</verify>
  <done>`cargo build` and `cargo test` for both `arcterm-app` and `arcterm-vt` pass with zero errors. All six Phase 7 success criteria have corresponding data paths wired in AppState. AI detection logs when agents are found. Error bridging flows from OSC 133 exit code through PaneContext to AI pane injection on Leader+a.</done>
</task>
