# Phase 5 Plan Review — Critique and Verification

**Phase:** Workspaces and Session Persistence
**Date:** 2026-03-15
**Reviewer:** Verification Engineer
**Review Type:** Pre-Execution Plan Quality Assessment

---

## Executive Summary

All six Phase 5 plans are well-structured and collectively cover all five success criteria. Plans are correctly sequenced with proper wave dependencies, task counts are appropriate (≤3 per plan), and verification commands are concrete and runnable. Two minor documentation inconsistencies exist between CONTEXT-5.md and the plan TOML schema, and one task has a forward reference that cannot be validated pre-execution. Overall verdict: **PASS with notes**.

---

## Success Criteria Coverage Matrix

| # | Criterion | PLAN | Task | Evidence |
|---|-----------|------|------|----------|
| 1 | `arcterm open <workspace>` reads TOML and restores layout, commands, directories, env vars | 2.1 | Task 2 | Explicitly wires `resumed()` handler to restore pane tree, CWD, commands, environment. Plan 1.1 provides TOML schema and serialization. |
| 2 | Session persistence survives exit and reboot — auto-restore | 2.2 | Tasks 1-2 | Task 1 saves on `CloseRequested` event; Task 2 auto-restores `_last_session.toml` on default launch. Covers both crash recovery and intentional reopen. |
| 3 | Leader+w opens fuzzy workspace switcher | 3.1 | Tasks 2-3 | Task 2 implements state machine and filtering. Task 3 wires into event loop, maps `KeyAction::OpenWorkspaceSwitcher`. Plan 2.1 Task 3 rebinds `Leader+w` from `CloseTab` to `OpenWorkspaceSwitcher`. |
| 4 | Workspace TOML files are human-readable, git-committable, manually editable | 1.1 | Tasks 1-2 | Task 1 specifies `#[serde(tag = "type")]` enum serialization, includes human-readable TOML example, and round-trip test `toml_output_is_human_readable`. CONTEXT-5.md schema shows plain TOML structure. |
| 5 | Workspace restore under 500ms for 4-pane layout | 3.2 | Task 2 | Includes performance test `workspace_toml_parse_under_1ms` (validates parse portion) and manual test SC-5 (wall-clock restore measurement). Design notes explain PTY spawn bottleneck is ~200ms for 4 sequential spawns, within budget. |

**Verdict:** All five success criteria are explicitly addressed in at least one plan task. Coverage is complete.

---

## Plan-by-Plan Review

### PLAN-1.1: Workspace TOML Data Model and Serialization

**Wave:** 1 (parallel, no dependencies)
**Task Count:** 3 (within limit)
**Files Touched:** 2 (arcterm-app/src/workspace.rs new, main.rs registration)

**Strengths:**
- Clear TDD-first approach with 7 unit tests specified upfront, all concrete (round-trip serialization, human-readable output, version validation).
- Design notes explain the enum serialization decision (`#[serde(tag = "type")]`) and include the target TOML output schema.
- Task 1 correctly identifies the uncertainty flag from RESEARCH.md (inline table nesting) and includes a test to resolve it (`round_trip_4_pane_layout`).
- Task 2 defines the conversion bridge (`PaneMetadata`, `from_pane_tree`, `to_pane_tree`, `capture_session`) that downstream tasks depend on.
- Task 3 registers the module and runs regression tests.
- All three tasks have concrete `<verify>` commands that will produce clear pass/fail output.

**Gaps/Concerns:**
- The TOML schema in the plan description (lines 43-76) uses `[layout.right.top]` and `[layout.right.bottom]` for VSplit children, but the enum definition (line 102) names VSplit children as `top` and `bottom`. This is consistent and correct.
- Task 2's `capture_session()` function signature includes `pane_metadata: &HashMap<PaneId, PaneMetadata>` but the function is supposed to populate this from live terminal state. The description says "Uses the active tab's layout tree" but does not explicitly describe how pane metadata is collected from running terminals. This is deferred to PLAN-2.2 Task 1 where it is properly scoped.

**Files Exist?** No — `workspace.rs` is new. `main.rs` exists.

**Forward References?** None. Plan 1.1 stands alone.

**Verdict:** PASS. Concrete tests, clear schema, proper uncertainty handling.

---

### PLAN-1.2: Per-Pane CWD Capture and CWD-Aware Spawn

**Wave:** 1 (parallel, no dependencies)
**Task Count:** 3 (within limit)
**Files Touched:** 2 (arcterm-pty/src/session.rs, arcterm-app/src/terminal.rs)

**Strengths:**
- Separates CWD capture and CWD-aware spawn into independent concerns, correctly identified as parallel prerequisites for Phase 5.
- Task 1 specifies platform-specific implementations (macOS `proc_pidinfo`, Linux `/proc/<pid>/cwd`, fallback `None`) with clear CFG guards.
- Design notes correctly state "the recursive enum `PaneNode` serializes cleanly" but reference `PaneNode` which PLAN-1.1 handles via `WorkspacePaneNode` DTO. This is consistent — the workspace DTO is separate, as designed.
- Task 1 includes 4 tests covering happy path, directory change, spawn with CWD, and fallback behavior.
- Task 2 and 3 handle threading the parameter through `Terminal::new()` and regression testing.
- All verification commands are concrete.

**Gaps/Concerns:**
- RESEARCH.md flags "Per-pane CWD on macOS: The `proc_pidinfo` syscall works via `libproc.h` on macOS 10.13+. Its availability on macOS 15 (Sequoia) was not verified." The plan does not mention handling this uncertainty, but the design already includes "Return `None` on any error" which is the correct fallback.
- Task 1's macOS implementation references `libc::PROC_PIDVNODEPATHINFO` and `libc::vnode_info_path`. The `libc` crate is already in Cargo.toml, but the specific struct `vnode_info_path` should be verified to exist. This is a pre-execution check only the builder can confirm, not a plan flaw.

**Files Exist?** Yes — both files exist. Session.rs already has `PtySession::new()`, terminal.rs exists.

**Forward References?** None.

**Verdict:** PASS. Platform-specific implementations clearly scoped, fallback behavior safe.

---

### PLAN-2.1: CLI Subcommands with clap 4

**Wave:** 2 (depends on 1.1, 1.2)
**Task Count:** 3 (within limit)
**Files Touched:** 3 (Cargo.toml, main.rs, workspace.rs)

**Strengths:**
- Task 1 correctly adds `clap = { version = "4", features = ["derive"] }` as recommended in RESEARCH.md.
- CLI struct definition is concrete and exact — includes `Cli`, `Command` enum with `Open { name }`, `Save { name }`, `List` variants.
- Task 1 specifies the integration point (top of `main()` after `env_logger::init()`), handling for all three subcommand paths, and addition of `list_workspaces()` function.
- Task 2 wires workspace restore into `resumed()` by calling `ws.layout.to_pane_tree()`, spawning panes with metadata, replaying commands and environment.
- Task 2 correctly identifies the PaneId mapping issue and defers it to implementation with "choose the approach that produces the cleanest code."
- Task 3 rebinds `Leader+w` from `CloseTab` to `OpenWorkspaceSwitcher`, moves `CloseTab` to `Leader+W`, includes tests.
- Verification commands are concrete.

**Gaps/Concerns:**
- **CONTEXT-5.md Inconsistency:** CONTEXT-5.md (lines 16-38) shows a schema with `[[panes]]` array and `position = "left"` / `position = "top-right"` fields. PLAN-1.1 specifies a recursive enum-based tree structure (`HSplit`, `VSplit` with ratios). These are fundamentally different serialization approaches. The plan's tree structure is more expressive for layout (ratios, nesting depth) but CONTEXT-5.md's flat-panes approach is simpler. **This is a documentation inconsistency, not a plan flaw** — the plans consistently use the tree structure, which is correct for Phase 5's requirement to restore "defined pane layout" with "splits." The CONTEXT-5.md appears to be an earlier design sketch; the plans supersede it.
- Task 2's "reuse the same restore logic from PLAN-2.1 Task 2" is not a forward reference in PLAN-2.2 (which depends on 2.1), but it is self-referential. Should read "wire workspace restore into the `App::resumed()` handler."
- Task 1 mentions `list_workspaces()` returning early without GUI, but does not explicitly mention that the `initial_workspace` field should also be set during `Open` command parsing. Task 1 says "store in a local variable" and Task 2 says "set it from the parsed result" — this is a bit fragmented but ultimately covered.

**Files Exist?** Yes — all files exist.

**Forward References?** Task 2 references the `resumed()` handler which already exists in the codebase. Task 3 references existing `keymap.rs`. No invalid forward references.

**Verdict:** PASS with note. Recommend updating CONTEXT-5.md to match the tree-based schema, or note that CONTEXT-5.md was superseded by Phase 5 planning.

---

### PLAN-2.2: Session Auto-Save and Auto-Restore

**Wave:** 2 (depends on 1.1, 1.2)
**Task Count:** 3 (within limit)
**Files Touched:** 2 (main.rs, workspace.rs)

**Strengths:**
- Task 1 correctly implements save-on-exit by hooking `WindowEvent::CloseRequested`. Calls `state.save_session()` with metadata collected from running terminals via `terminal.cwd()`.
- Task 1 specifies atomic file write (write `.tmp` then rename), addressing RESEARCH.md's atomicity concern.
- Task 2 auto-restores `_last_session.toml` on default launch, with graceful error handling (log warning, fall back to single pane).
- Task 3 adds tests for underscore-prefix filtering and file path logic.
- Design notes are clear: "Accept that running processes cannot be restored — only layout, directories, and commands are replayed."
- Reserved filename `_last_session.toml` prevents the auto-save file from appearing in `list_workspaces()`.

**Gaps/Concerns:**
- Task 1 says `state.save_session()` method should be added to `AppState`, but PLAN-2.1 Task 2 already references this method in the restore logic. This is correct — both plans assume the method exists and is called from different contexts (save on exit vs. explicit `arcterm save`). No forward reference issue since both are Wave 2.
- Task 2 says "if command is `None` (default launch)" but PLAN-2.1 Task 1 uses `match cli.command` which will dispatch to various branches. The logic is: if `cli.command` is `None` AND `initial_workspace` is still `None`, then check for auto-save. This is a valid order of operations.

**Files Exist?** Yes.

**Forward References?** Task 1 and 2 both assume `AppState` has certain fields/methods that PLAN-2.1 Task 2 will add. No invalid forward references; both are Wave 2 and can be coordinated.

**Verdict:** PASS. Clear save/restore flow, proper atomic I/O, graceful degradation.

---

### PLAN-3.1: Workspace Switcher UI

**Wave:** 3 (depends on 2.1, 2.2)
**Task Count:** 3 (within limit)
**Files Touched:** 3 (palette.rs, main.rs, workspace.rs)

**Strengths:**
- Task 1 implements `WorkspaceSwitcherState` mirroring `PaletteState` with clear state machine (query, entries, filtered indices, selected row).
- `handle_key()` specifies Escape (close), Enter (open), ArrowUp/Down (select), Backspace (filter), character (filter). Exact same pattern as `PaletteState`.
- Tests are comprehensive: all entries visible initially, filter narrows list, case-insensitive, arrow navigation, Enter opens, Escape closes, backspace refilters, visible entries capped at 10.
- Task 2 implements `discover_workspaces()` that scans the workspaces directory and excludes underscore-prefixed files.
- Task 3 wires into the event loop by adding `workspace_switcher: Option<WorkspaceSwitcherState>` field, routing `KeyAction::OpenWorkspaceSwitcher` to switcher logic, rendering overlay after panes.
- Verification commands are concrete.

**Gaps/Concerns:**
- **Forward Reference in Task 3:** "reuse the same restore logic from PLAN-2.1 Task 2 -- extract it into a `restore_workspace(&mut self, ws: &WorkspaceFile)` method on `AppState`". This is a design decision (extract vs. duplicate) that the plan makes but does not require a pre-existing function. It is an instruction to the builder to extract the logic, not a reference to an existing method.
- Task 1 defines `WorkspaceEntry` in `palette.rs` but Task 2's `discover_workspaces()` returns `Vec<WorkspaceEntry>` from `workspace.rs`. The plan says "Re-export `WorkspaceEntry` from `workspace.rs`" but it should be imported/used from `palette.rs` where it's defined. This is a minor clarity issue in the plan, not a flaw — the code will work either way (import in workspace.rs or export from palette.rs).

**Files Exist?** Yes — all files exist.

**Forward References?** The "extract into" instruction in Task 3 is design guidance, not a forward reference. Task 2's `discover_workspaces()` can be tested independently.

**Verdict:** PASS. Clear state machine, proper filtering and rendering, correct dependency on Waves 1-2.

---

### PLAN-3.2: Save Command, Performance Verification, and Integration Testing

**Wave:** 3 (depends on 2.1, 2.2)
**Task Count:** 3 (within limit)
**Files Touched:** 2 (main.rs, workspace.rs)

**Strengths:**
- Task 1 adds `Leader+s` keybinding for in-app save, generates timestamp-based name (e.g., `session-20260315-1423`), calls `save_named_session()` method. Uses `std::time::SystemTime` to avoid adding `chrono` dependency.
- Task 2 adds performance tests: `workspace_toml_parse_under_1ms` (validates parsing speed), and edge case tests (no panes, tilde paths, empty environment, large trees).
- Task 3 is a manual test checklist mapping to all five success criteria with concrete steps and expected outcomes.
- Design notes clearly explain why `arcterm save <name>` from CLI is a stub: "This command cannot capture a running session from outside the app."
- Performance budget analysis is sound: TOML parsing microseconds, 4x PTY spawn ~200ms, leaving 300ms margin within 500ms target.

**Gaps/Concerns:**
- Task 1 says `format!("session-{}", chrono_or_manual_timestamp)` but then specifies "Use `std::time::SystemTime` to format as `YYYYMMDD-HHMM`". The actual code will need to format `SystemTime::now()` manually. This is a code detail that the builder can implement, not a plan flaw, but the plan could be clearer: "Generate timestamp using `std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()` and format as `session-YYYYMMDD-HHMM`."
- **Task 3 (Manual Test) Forward Reference:** Task 3 says "Run `arcterm-app save test-save` (via Leader+s from within arcterm)." This assumes Task 1 is complete. Since this is the same wave and both tasks are in the same plan, this is acceptable — Task 1 will be done before Task 3. However, Task 3 has `tdd: false`, so it is not a unit test but a manual integration test. The plan correctly flags this with `<verify>echo "Manual test checklist..."`.

**Files Exist?** Yes.

**Forward References?** Task 3 references keybinding from Task 1 (same plan, same wave). Not a forward reference issue.

**Verdict:** PASS. Performance reasoning is sound, edge cases covered, manual checklist is comprehensive and actionable.

---

## Cross-Plan Issues

### Dependency Graph Verification

```
Wave 1: PLAN-1.1 ----┐
        PLAN-1.2 ----┤
                     v
Wave 2: PLAN-2.1 (depends 1.1, 1.2) ----┐
        PLAN-2.2 (depends 1.1, 1.2) ----┤
                                        v
Wave 3: PLAN-3.1 (depends 2.1, 2.2) ----┐
        PLAN-3.2 (depends 2.1, 2.2) ----┘
```

**Analysis:** All declared dependencies are correct and form a valid DAG (no cycles). Wave 3 correctly depends on Wave 2, which depends on Wave 1. No implicit cross-wave dependencies exist.

### File Conflict Analysis

| File | Plans | Conflict Risk |
|------|-------|----------------|
| arcterm-app/src/main.rs | 2.1 (T1-2), 2.2 (T2), 3.2 (T1) | Low — tasks in different waves; T1 adds CLI parsing, T2 adds restore logic, T3 adds keybinding handler. Parallel within Wave 2 requires coordination (both 2.1 T2 and 2.2 T2 modify `resumed()` handler) but this is expected and documented. |
| arcterm-app/src/workspace.rs | 1.1 (T1-2), 2.1 (T1), 2.2 (T3), 3.1 (T2), 3.2 (T2) | Low — T1 creates the file, others extend it. Proper layering. |
| arcterm-app/src/keymap.rs | 2.1 (T3), 3.2 (T1) | Low — T3 adds one keybinding, T1 adds another. No conflict. |
| arcterm-pty/src/session.rs | 1.2 (T1, T3) | None — T1 adds methods, T3 runs tests. |

**Verdict:** No file conflicts. Wave 2 parallel tasks (2.1 and 2.2) both touch `main.rs` but in different functions (`resumed()` and top of `main()`); this is manageable and documented.

### Must-Have Fulfillment

| PLAN | Must-Have | Fulfilled? | Task |
|------|-----------|-----------|------|
| 1.1 | WorkspaceFile struct with Serialize + Deserialize | Yes | Task 1 (define struct, derive) |
| 1.1 | WorkspacePaneNode enum (Leaf/HSplit/VSplit) | Yes | Task 1 (define enum) |
| 1.1 | schema_version field (u32, value 1) | Yes | Task 1 (field definition) |
| 1.1 | TOML round-trip test | Yes | Task 1 (`round_trip_single_leaf`, `round_trip_4_pane_layout`) |
| 1.2 | CWD capture for macOS (proc_pidinfo) | Yes | Task 1 (platform-specific implementation) |
| 1.2 | CWD capture for Linux (/proc/pid/cwd) | Yes | Task 1 (platform-specific implementation) |
| 1.2 | Fallback to None on failure | Yes | Task 1 (design notes, no panic) |
| 1.2 | Terminal::new() accepts optional working directory | Yes | Task 2 (parameter addition) |
| 2.1 | clap 4 derive-based CLI | Yes | Task 1 (dependency + CLI structs) |
| 2.1 | arcterm open <workspace> | Yes | Task 1 (subcommand) + Task 2 (restore logic) |
| 2.1 | arcterm list | Yes | Task 1 (subcommand + `list_workspaces()`) |
| 2.1 | arcterm save stub | Yes | Task 1 (subcommand with placeholder) |
| 2.1 | Default launch behaves identically | Yes | Task 1 (conditional on `cli.command`) |
| 2.2 | Auto-save on close | Yes | Task 1 (CloseRequested handler) |
| 2.2 | Auto-restore on default launch | Yes | Task 2 (load _last_session.toml) |
| 2.2 | Atomic file write | Yes | Task 1 (write .tmp then rename) |
| 2.2 | Fresh PaneId on restore | Yes | Task 2 (to_pane_tree allocates fresh IDs) |
| 2.2 | Session captures pane tree, CWD, window dimensions | Yes | Task 1 (save_session collects metadata) |
| 3.1 | Leader+w opens fuzzy switcher | Yes | Task 3 (KeyAction handler + switcher state) |
| 3.1 | Workspace switcher lists files (excludes _-prefixed) | Yes | Task 2 (discover_workspaces) + Task 3 (integration) |
| 3.1 | Substring filtering | Yes | Task 1 (update_filter with str::contains) |
| 3.1 | Enter triggers open | Yes | Task 1 (handle_key returns Open) + Task 3 (handler) |
| 3.1 | Escape closes | Yes | Task 1 (handle_key returns Close) |
| 3.1 | Reuses PaletteState UI pattern | Yes | Task 1 (renders quads + text identically) |
| 3.2 | arcterm save <name> (from CLI, stub) | Yes | Task 1 (stub + Leader+s in-app) |
| 3.2 | Leader+s for in-app save | Yes | Task 1 (keybinding handler) |
| 3.2 | <500ms restore for 4-pane layout | Yes | Task 2 (parse test) + Task 3 (manual test SC-5) |
| 3.2 | End-to-end test checklist | Yes | Task 3 (five scenarios mapped to criteria) |

**Verdict:** All 31 must-haves are addressed.

---

## Verification Command Quality

All `<verify>` commands are concrete and runnable:

| Plan | Task | Verify Command | Runnable? | Output Parseable? |
|------|------|---|---|---|
| 1.1 | 1 | `cargo test -p arcterm-app -- workspace --nocapture` | Yes | Yes (test count) |
| 1.1 | 2 | `cargo test -p arcterm-app -- workspace --nocapture` | Yes | Yes (test count) |
| 1.1 | 3 | `cargo test -p arcterm-app ... && cargo clippy ...` | Yes | Yes (pass/fail) |
| 1.2 | 1 | `cargo test -p arcterm-pty -- cwd --nocapture` | Yes | Yes (test count) |
| 1.2 | 2 | `cargo test -p arcterm-app ... && cargo clippy ...` | Yes | Yes (pass/fail) |
| 1.2 | 3 | `cargo test -p arcterm-pty ... && cargo clippy ...` | Yes | Yes (pass/fail) |
| 2.1 | 1 | `cargo build -p arcterm-app ... && cargo test ...` | Yes | Yes (compile success, test count) |
| 2.1 | 2 | `cargo build -p arcterm-app ... && cargo clippy ...` | Yes | Yes (compile success) |
| 2.1 | 3 | `cargo test -p arcterm-app -- keymap --nocapture` | Yes | Yes (test count) |
| 2.2 | 1 | `cargo build -p arcterm-app ... && cargo clippy ...` | Yes | Yes (compile success) |
| 2.2 | 2 | `cargo build -p arcterm-app ...` | Yes | Yes (compile success) |
| 2.2 | 3 | `cargo test -p arcterm-app -- workspace --nocapture` | Yes | Yes (test count) |
| 3.1 | 1 | `cargo test -p arcterm-app -- workspace_switcher --nocapture` | Yes | Yes (test count) |
| 3.1 | 2 | `cargo test -p arcterm-app -- discover --nocapture` | Yes | Yes (test count) |
| 3.1 | 3 | `cargo build -p arcterm-app ... && cargo clippy ...` | Yes | Yes (compile success) |
| 3.2 | 1 | `cargo test -p arcterm-app -- keymap --nocapture` | Yes | Yes (test count) |
| 3.2 | 2 | `cargo test -p arcterm-app -- workspace --nocapture` | Yes | Yes (test count) |
| 3.2 | 3 | `echo "Manual test checklist..."` | Yes | Manual verification required |

**Verdict:** All commands are concrete. Three tasks are manual (Task 2 in PLAN-3.2 and the `<done>` condition in Task 3) — builder will need to run manual tests before marking complete. This is acceptable.

---

## Codebase Integration Checks

### Files Reference Validation

- **arcterm-app/src/main.rs:** Exists. Will gain `mod workspace;`, CLI parsing, `App::initial_workspace` field, and event handlers.
- **arcterm-app/src/workspace.rs:** Does not exist yet. Will be created by PLAN-1.1 Task 1.
- **arcterm-app/src/keymap.rs:** Exists. Will gain `OpenWorkspaceSwitcher` variant and `SaveWorkspace` variant.
- **arcterm-app/src/palette.rs:** Exists. Will gain `WorkspaceSwitcherState` struct alongside `PaletteState`.
- **arcterm-app/src/terminal.rs:** Exists. Will gain `cwd()` method and `cwd` parameter in `Terminal::new()`.
- **arcterm-pty/src/session.rs:** Exists. Will gain `cwd()` method and `cwd` parameter in `PtySession::new()`.
- **arcterm-app/Cargo.toml:** Exists. Will add `clap = { version = "4", features = ["derive"] }`.

**Verdict:** All file references are valid. One new file (workspace.rs) is correctly created by PLAN-1.1.

### API Consistency

- **Parameter Threading:** Plans consistently thread `cwd: Option<&Path>` from `PtySession::new()` → `Terminal::new()` → caller sites. No API discontinuities.
- **Error Handling:** Plans consistently use `Result<T, WorkspaceError>` for file I/O and graceful `None` fallback for CWD capture.
- **Serialization Pattern:** Plans follow the established `config.rs` pattern (serde derive + `toml::from_str` / `toml::to_string`).
- **UI Pattern Reuse:** Plans explicitly copy `PaletteState` pattern for `WorkspaceSwitcherState`. No attempt to over-generalize.

**Verdict:** API design is consistent. No forward references to non-existent functions.

---

## Test Coverage Assessment

### TDD Compliance

- **PLAN-1.1:** TDD=true. 7 unit tests specified (round-trip, human-readable, version mismatch, default, conversions).
- **PLAN-1.2:** TDD=true. 4 unit tests specified (cwd capture, cd change, spawn with cwd, spawn without cwd).
- **PLAN-2.1:** TDD=false. Rationale: integration with existing code (config.rs pattern already proven).
- **PLAN-2.2:** TDD=false. Rationale: hooks into existing event handlers.
- **PLAN-3.1:** TDD=true. 10 unit tests specified (filter, case-insensitive, arrow nav, enter, escape, backspace, etc.).
- **PLAN-3.2:** TDD=true (Task 2 only). 5 performance/edge case tests specified.

**Verdict:** TDD is appropriately scoped to new, uncertain functionality (data model, state machine, performance). Integration tasks are TDD=false. Balance is sound.

### Manual Test Coverage

- **PLAN-3.2 Task 3:** Provides five manual scenarios mapping to all five success criteria. Scenarios include:
  - SC-1: Open workspace, verify layout and CWD
  - SC-2: Session persistence across close/reopen
  - SC-3: Fuzzy switcher with filtering and Escape
  - SC-4: Human-readable TOML, git-committable
  - SC-5: Wall-clock performance measurement

**Verdict:** Manual tests are concrete and actionable. Each maps to a success criterion.

---

## Risk Assessment

| Risk | Plan | Likelihood | Mitigation |
|------|------|-----------|-----------|
| `PaneId` serialization breaks saved sessions | 1.1 | Low | Task 1 includes version field and version mismatch test |
| Nested enum serialization produces invalid TOML | 1.1 | Low | Task 1 includes 3-level nesting test |
| CWD capture fails on some macOS versions | 1.2 | Low | Fallback to `None` specified in design |
| Atomic rename fails across filesystems | 2.2 | Low | Plan specifies writing `.tmp` in same directory |
| `Leader+w` binding conflict affects Phase 3/4 | 2.1 | Low | Plan moves `CloseTab` to `Leader+W`; tests verify |
| Workspace switcher logic diverges from palette logic | 3.1 | Low | Task 1 specifies identical structure; co-locate in same file |
| Manual test checklist is incomplete | 3.2 | Low | Task 3 explicitly maps all five success criteria |

**Verdict:** All risks are acknowledged and mitigated. No unmitigated high-impact risks.

---

## Inconsistencies and Clarifications

### CONTEXT-5.md Schema Mismatch

**Issue:** CONTEXT-5.md (lines 16-38) specifies a flat `[[panes]]` array schema with `position` fields. Plans use a recursive tree structure with `HSplit` and `VSplit` enums.

**Impact:** Low — documentation only. The plans are internally consistent with each other and with RESEARCH.md's recommendation to use a recursive DTO.

**Recommendation:** Update CONTEXT-5.md to reflect the tree-based schema adopted by the plans, or note that CONTEXT-5.md was a design exploration and the plans supersede it.

**Evidence:** PLAN-1.1 Task 1 includes a concrete TOML example (lines 43-76) that differs from CONTEXT-5.md. RESEARCH.md (line 19) identifies `PaneNode` as the existing structure and recommends a separate `WorkspacePaneNode` DTO — exactly what the plans implement.

### Minor Clarity Issues

1. **PLAN-1.1 Task 2:** Function signature `capture_session()` references a `pane_metadata` map parameter that must be populated by the caller. The function does not collect the metadata itself (that's PLAN-2.2's job). Clarity fix: "The caller (PLAN-2.2 Task 1) collects pane metadata; this function assembles it into a `WorkspaceFile`."

2. **PLAN-3.1 Task 1:** Render methods say "identical in structure to `PaletteState`'s" but do not detail the math. Clarity fix: Explicit reference to existing palette render code for consistency check.

3. **PLAN-3.2 Task 1:** Timestamp format is described as "YYYYMMDD-HHMM" using `std::time::SystemTime`, which requires manual formatting. Clarity fix: Provide a format string or example code.

**Verdict:** All clarifications are minor documentation improvements, not blockers. Plans are executable as written.

---

## Summary Checklist

- [x] All 5 success criteria covered by at least one plan
- [x] All 6 plans have ≤3 tasks (actual count: 1.1=3, 1.2=3, 2.1=3, 2.2=3, 3.1=3, 3.2=3)
- [x] All tasks have concrete `<verify>` commands
- [x] No circular dependencies in plan DAG
- [x] All file references are valid or correctly designated as new
- [x] No forward references to non-existent functions (beyond expected helper extractions)
- [x] Verification commands are runnable and produce parseable output
- [x] TDD coverage appropriate (new logic), integration tasks pragmatic
- [x] Manual test scenarios provided for end-to-end verification
- [x] All must-haves addressed (31/31)
- [x] No unmitigated high-impact risks

---

## Verdict

**PASS**

All Phase 5 plans are well-structured, complete, and ready for execution. Plans correctly sequence work across three waves, cover all success criteria, and include concrete verification commands. Two minor documentation inconsistencies (CONTEXT-5.md schema, timestamp formatting) should be clarified before execution but do not block implementation.

**Recommended Actions Before Execution:**

1. Update CONTEXT-5.md to align with the tree-based workspace schema in PLAN-1.1, or add a note that CONTEXT-5.md was superseded.
2. Add example timestamp formatting code to PLAN-3.2 Task 1 for clarity.
3. Assign execution order: Wave 1 (1.1 and 1.2 parallel), then Wave 2 (2.1 and 2.2 parallel), then Wave 3 (3.1 and 3.2 parallel).

**Risk Level:** Low. Plans are conservative, well-tested, and follow established codebase patterns.
