---
phase: ai-integration
plan: "7.2"
wave: 2
dependencies: ["7.1"]
must_haves:
  - MCP tool discovery via OSC 7770 (tools/list query triggers JSON response)
  - Leader+p toggles plan status strip (ambient bar + expanded overlay)
  - Leader+a jumps to most recently active AI pane
files_touched:
  - arcterm-vt/src/handler.rs
  - arcterm-vt/src/processor.rs
  - arcterm-app/src/keymap.rs
  - arcterm-app/src/plan.rs
  - arcterm-app/src/main.rs
  - arcterm-app/src/terminal.rs
tdd: true
---

# Plan 7.2 -- MCP Tool Discovery + Plan Status Layer + AI Keybindings

**Wave 2** | Depends on 7.1 (PaneContext, AiAgentState, OSC 133 in VT layer) | Three parallel features

## Goal

Deliver three independent features that all depend on the Wave 1 data model:
(1) MCP tool discovery so AI agents can query available plugin tools via OSC 7770,
(2) a plan status strip with Leader+p toggle for the expanded view, and
(3) Leader+a to jump to the most recently active AI pane.

Features 2 and 3 share no file dependencies with feature 1 and could be built in
parallel, but they are grouped into one plan because they all depend on 7.1 and are
each small enough to fit within three tasks.

---

<task id="1" files="arcterm-vt/src/handler.rs, arcterm-vt/src/processor.rs, arcterm-app/src/terminal.rs, arcterm-app/src/main.rs" tdd="true">
  <action>
    Implement MCP tool discovery via OSC 7770 query/response:

    1. In `arcterm-vt/src/handler.rs`, add two methods to the `Handler` trait with
       default no-op implementations:
       - `tool_list_query(&mut self)` -- called when the VT processor receives
         `ESC ] 7770 ; tools/list ST`.
       - `tool_call(&mut self, name: String, args_json: String)` -- called when the VT
         processor receives `ESC ] 7770 ; tools/call ; name=<n> ; args=<base64> ST`.

    2. In `GridState`, implement `tool_list_query` by pushing a sentinel value onto a new
       `pub tool_queries: Vec<()>` drain buffer (same pattern as `completed_blocks` and
       `shell_exit_codes`). Implement `tool_call` by pushing `(name, args_json)` onto a
       new `pub tool_calls: Vec<(String, String)>` drain buffer.

    3. In `arcterm-vt/src/processor.rs` `dispatch_osc7770`, extend the action matching to
       handle `b"tools/list"` (call `handler.tool_list_query()`) and `b"tools/call"`
       (parse `name=` and `args=` from remaining params, base64-decode args, call
       `handler.tool_call(name, decoded_args)`). Use the `base64` crate (add to
       `arcterm-vt/Cargo.toml` if not present; check workspace deps first).

    4. In `arcterm-app/src/terminal.rs`, add `take_tool_queries(&mut self) -> Vec<()>`
       and `take_tool_calls(&mut self) -> Vec<(String, String)>` methods that drain the
       GridState buffers.

    5. In `arcterm-app/src/main.rs`, in the PTY output processing loop (where
       `take_pending_replies` and `take_completed_blocks` are already called), add:
       - For each drained tool query: call `plugin_manager.list_tools()`, serialize the
         result as JSON, base64-encode it, and write
         `ESC ] 7770 ; tools/response ; <base64_json> ST` back to the pane's PTY input
         via `terminal.write_input()`.
       - For each drained tool call: call `plugin_manager.call_tool(name, args)` (to be
         added -- see below), serialize the result, and write
         `ESC ] 7770 ; tools/result ; result=<base64_json> ST` back to the PTY.

    6. In `arcterm-plugin/src/manager.rs`, add a `call_tool(&self, name: &str, args_json: &str) -> Result<String>`
       method that finds the plugin owning the named tool and invokes it. For Phase 7,
       this can return a JSON string `{"error": "tool invocation not yet implemented"}`
       as a stub -- the full WASM tool invocation pipeline is deferred. The important
       thing is the OSC 7770 round-trip works.

    7. Unit tests in `arcterm-vt`:
       - Feed `ESC ] 7770 ; tools/list ST` and verify `tool_queries` has one entry.
       - Feed `ESC ] 7770 ; tools/call ; name=get_pods ; args=<base64("{}") ST` and verify
         `tool_calls` has `("get_pods", "{}")`.
       - Malformed tool queries (missing params) are silently ignored.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-vt -- tools --no-fail-fast 2>&1 | tail -20 && cargo build --package arcterm-app 2>&1 | tail -10</verify>
  <done>OSC 7770 tools/list and tools/call are parsed by the VT processor. Tool queries and calls are drained in AppState and produce responses written back to PTY input. Unit tests verify the round-trip parsing. `cargo build --package arcterm-app` succeeds.</done>
</task>

<task id="2" files="arcterm-app/src/keymap.rs" tdd="true">
  <action>
    Add Leader+a and Leader+p keybindings to the keymap state machine:

    1. Add two new variants to the `KeyAction` enum:
       - `JumpToAiPane` -- triggered by Leader then `a`.
       - `TogglePlanView` -- triggered by Leader then `p`.

    2. In `handle_logical_key_with_time`, in the `LeaderPending` branch's
       `Key::Character(s)` match, add:
       - `"a" => Some(KeyAction::JumpToAiPane)`
       - `"p" => Some(KeyAction::TogglePlanView)`

    3. Add unit tests following the existing pattern in `keymap.rs::tests`:
       - `leader_then_a_jumps_to_ai_pane`: press Ctrl+a then `a`, assert
         `KeyAction::JumpToAiPane`.
       - `leader_then_p_toggles_plan_view`: press Ctrl+a then `p`, assert
         `KeyAction::TogglePlanView`.
       - Verify both reset `is_leader_pending()` to `false` after dispatch.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-app -- keymap::tests --no-fail-fast 2>&1 | tail -20</verify>
  <done>All keymap tests pass including the two new tests. Leader+a returns `JumpToAiPane`, Leader+p returns `TogglePlanView`. No existing keybindings are affected.</done>
</task>

<task id="3" files="arcterm-app/src/plan.rs, arcterm-app/src/main.rs" tdd="true">
  <action>
    Implement the plan status layer and wire Leader+a / Leader+p into AppState:

    1. Create `arcterm-app/src/plan.rs` with:
       - `PlanSummary` struct: `phase: Option<String>`, `completed: usize`, `total: usize`,
         `file_path: PathBuf`.
       - `parse_plan_summary(path: &Path) -> Option<PlanSummary>` function that reads a
         file at `path` and counts `[x]` (completed) and `[ ]` (incomplete) checkbox
         patterns via simple string matching (no markdown parser needed). If the file
         contains a YAML frontmatter `phase:` field, extract it. Return `None` if the
         file does not exist or has no checkboxes.
       - `discover_plan_files(workspace_root: &Path) -> Vec<PathBuf>` function that checks
         for `.shipyard/` directory with `PLAN-*.md` files, then `PLAN.md`, then `TODO.md`.
         Returns all found paths.
       - `PlanStripState` struct: `summaries: Vec<PlanSummary>`, `last_updated: Instant`.
       - `PlanViewState` struct (expanded overlay): `entries: Vec<PlanSummary>`,
         `selected: usize`. Follow the `PaletteState` / `WorkspaceSwitcherState` pattern.
       - Unit tests: `parse_plan_summary` with fixture strings containing `[x]` and `[ ]`
         checkboxes returns correct counts. Empty file returns `None`. File with no
         checkboxes returns `None`.

    2. In `arcterm-app/src/main.rs`:
       - Add `plan_strip: Option<PlanStripState>` and `plan_view: Option<PlanViewState>`
         fields to `AppState`.
       - Add `plan_watcher: Option<notify::RecommendedWatcher>` to `AppState`. Initialize
         it following the exact pattern in `config.rs` (use `notify::recommended_watcher`
         with `std::sync::mpsc::channel`). Watch the workspace root's `.shipyard/` directory,
         `PLAN.md`, and `TODO.md`. On file change, re-parse summaries and update
         `plan_strip`.
       - Handle `KeyAction::TogglePlanView`: toggle `plan_view` between `None` and
         `Some(PlanViewState::new(plan_strip.summaries.clone()))`.
       - Handle `KeyAction::JumpToAiPane`: if `last_ai_pane` is `Some(id)` and that pane
         still exists in `panes`, call `set_focused_pane(id)`. Otherwise no-op.
       - In the render path, when `plan_strip` is `Some`, subtract one row of height from
         pane rects (same technique as the tab bar) and add one `OverlayQuad` at the
         bottom of the window with text showing `"Phase {phase} | {completed}/{total}"`.
       - When `plan_view` is `Some`, render the expanded overlay following the command
         palette pattern (dim background overlay + centered list of plan entries).

    Register `mod plan;` in main.rs.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-app -- plan:: --no-fail-fast 2>&1 | tail -20 && cargo build --package arcterm-app 2>&1 | tail -10</verify>
  <done>`parse_plan_summary` tests pass with correct checkbox counting. `cargo build` succeeds with plan strip, plan view overlay, Leader+a jump, and Leader+p toggle all wired into AppState. The plan watcher is initialized using the same `notify` pattern as config hot-reload.</done>
</task>
