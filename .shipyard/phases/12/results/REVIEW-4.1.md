# REVIEW-4.1: Reconnect AI Features, Delete Old Crates, Integration Tests

**Reviewer:** Claude (review agent)
**Branch:** phase-12-engine-swap
**Date:** 2026-03-16
**SUMMARY reviewed:** `.worktrees/phase-12-engine-swap/.shipyard/phases/12/results/SUMMARY-4.1.md`

---

## Stage 1: Spec Compliance

**Verdict:** PASS

### Task 1: Verify and reconnect AI features

- **Status:** PASS
- **Evidence:**
  - `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/ai_detect.rs`: No imports from `arcterm_core`, `arcterm_vt`, or `arcterm_pty`. `AiAgentState::check(pid: Option<u32>)` accepts `Option<u32>` and calls `detect_ai_agent` with the raw PID — matches the spec's requirement for `terminal.child_pid()` (which returns `Option<u32>`) as the argument source.
  - `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:655–664`: `child_pid()` returns `Some(self.child_pid)` (always `Some` given the current field type); `cwd()` delegates to `cwd_for_pid(self.child_pid)` which reads `/proc/{pid}/cwd`.
  - `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/context.rs:163–182`: `collect_sibling_contexts` iterates `&HashMap<PaneId, Terminal>`, calls `t.cwd()` per entry — matches the spec exactly.
  - `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/main.rs:2835`: `ai_detect::AiAgentState::check(child_pid)` called with the result of `terminal.child_pid()` (line 2747–2750). `take_exit_codes()` draining into `PaneContext` confirmed at line 1542.
  - `cargo test --workspace` result: 415 passed, 0 failed (21 lib + 322 bin + 3 integration + 22 plugin + 47 render).
- **Notes:** Task 1 was verification-only; the AI features were already using new types from prior phases. The SUMMARY correctly documents this as no-op. The acceptance criteria (compile, `detect_ai_agent` logic, `collect_sibling_contexts`, exit code drain) are all met.

### Task 2: Delete old crates and clean up workspace

- **Status:** PASS
- **Evidence:**
  - `arcterm-core/`, `arcterm-vt/`, `arcterm-pty/` directories confirmed absent (bash probe returned `DELETED` for all three).
  - `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/Cargo.toml`: `[workspace] members` contains exactly `["arcterm-render", "arcterm-app", "arcterm-plugin"]` — 3 members as required. `[workspace.dependencies]` contains no `arcterm-core`, `arcterm-vt`, `arcterm-pty`, or `portable-pty` entries. `vte` is also absent (correctly removed per the plan's conditional).
  - `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/Cargo.toml`: No old crate path dependencies.
  - Grep for `arcterm_core|arcterm_vt|arcterm_pty|portable_pty` across all `.rs` and `.toml` files: only one match, a doc comment in `arcterm-app/src/osc7770.rs:6` reading `"from arcterm_render rather than arcterm_vt, eliminating the need for"` — this is a documentation reference only, not a `use` statement or functional reference. Does not violate the acceptance criterion (which targets `use`/`extern` references, not doc comment prose).
  - `cargo clippy --workspace -- -D warnings`: clean (`Finished dev profile, 0 errors`).
- **Notes:** The SUMMARY accurately documents all changes, including the tuple refactor of `grid_size_for_rect` and the clippy fixes applied. No functional regressions introduced.

### Task 3: Add integration tests

- **Status:** PASS
- **Evidence:**
  - `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/tests/engine_migration.rs` exists with 6 tests (spec required at least terminal creation, PreFilter round-trip, and write-input).
  - Test 1 `terminal_creates_pty_and_reports_pid`: `#[ignore]`, creates `Terminal::new(80, 24, 9, 18, None, None)`, asserts `child_pid()` is `Some(>0)`. Matches spec exactly.
  - Test 2 `prefilter_round_trip_separates_intercepted_and_passthrough`: passes without `#[ignore]`, feeds OSC 7770 + APC + plain text, verifies `passthrough == b"helloworld"`, `osc7770_params.len() == 1`, `apc_payloads.len() == 1`.
  - Test 3 `prefilter_handles_split_sequences`: passes without `#[ignore]`, verifies the state machine survives split across two `advance()` calls — this is the spec's implicit PTY read boundary requirement.
  - Test 4 `write_input_echo_hello_appears_in_grid`: `#[ignore]`, polls `has_wakeup()`, snapshots via `snapshot_from_term`, asserts "hello" in grid rows.
  - Test 5 `resize_updates_terminal_dimensions`: `#[ignore]`, asserts `cols()/rows()` update after `resize(120, 40, ...)`.
  - Test 6 `prefilter_osc7770_start_content_end_sequence`: passes without `#[ignore]`, verifies 2 OSC 7770 params extracted from a start/content/end block and content passes through.
  - `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/lib.rs`: `[lib]` target added, re-exports `Terminal` and `PreFilter`. `arcterm-app/Cargo.toml` confirms `[lib]` entry. Integration tests import `arcterm_app::Terminal` and `arcterm_app::PreFilter` correctly.
  - `cargo test --workspace`: 3 integration tests pass, 3 ignored (exactly the PTY-dependent ones).
  - The `arcterm-render` dependency on `arcterm-app` (confirmed in `Cargo.toml:24`) satisfies the `use arcterm_render::snapshot_from_term` import in the write-input test.

---

## Stage 2: Code Quality

### Critical

None.

### Important

- **`child_pid()` returns infallible `Some(u32)` but is typed as `Option<u32>`** at `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:655–656`.
  - The method body is `Some(self.child_pid)` — it is always `Some` because `child_pid` is a `u32` field set unconditionally at construction time. The `Option<u32>` return type was designed for the old architecture where the PID might not be known. All callers (`main.rs:2747–2750`, integration test) pattern-match or call `.is_some()` unnecessarily. This will mislead future maintainers into thinking the PID might be absent at runtime.
  - Remediation: Either change the return type to `u32` and update all call sites (simplest and most accurate), or document prominently on the method that it currently always returns `Some` with the rationale for retaining `Option` (e.g., anticipated future scenarios where PID capture fails). If changed to `u32`, the `collect_sibling_contexts` call at `context.rs:172` (`and_then(|t| t.cwd())`) is unaffected since `cwd()` is called separately.

- **`#[allow(dead_code)]` on `proc::process_comm` and `proc::process_args`** at `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/proc.rs:11` and `:51`.
  - The SUMMARY explains these functions are used only by the binary target's `ai_detect` module, not the lib target. This is correct — the lib target (`src/lib.rs`) does not include `ai_detect.rs`, so the compiler warns when building the lib. However, `process_comm` and `process_args` are legitimately used functions, not truly dead code. Suppressing the warning with `#[allow(dead_code)]` hides real module wiring gaps in `lib.rs`.
  - Remediation: Add `pub(crate) mod ai_detect;` and `pub(crate) mod proc;` re-exports to `lib.rs` (or make them `pub(crate)` modules in lib), which both solves the warning correctly and makes the function availability consistent across compilation targets. Alternatively, if `lib.rs` intentionally exposes only `Terminal` and `PreFilter`, document this scoping decision with a comment and keep the suppressions, but replace `#[allow(dead_code)]` on the functions themselves with a file-level `#![cfg_attr(not(test), allow(dead_code))]` to be more precise.

- **`push_output_line` and `set_command` in `context.rs` are marked `#[allow(dead_code)]`** at `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/context.rs:77–78` and `:87–88`.
  - `push_output_line` has no caller in `main.rs` (grep confirms zero matches). `set_command` also has no caller in `main.rs`. These two methods are core to the `PaneContext` design — output ring population and OSC 133 B command capture — yet are completely unwired. The output ring and `last_command` field will always be empty at runtime, making `error_context`'s `output_tail` and `ErrorContext::command` always empty/default.
  - Remediation: Wire `set_command` to the OSC 133 B handler and `push_output_line` to the PTY output processing loop in `main.rs`. If this wiring is deferred to a future phase, add a tracked issue rather than using `#[allow(dead_code)]` which makes the gap invisible.

- **Integration test `_wakeup_rx` naming discrepancy** at `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/tests/engine_migration.rs:31,134,174`.
  - The plan spec (Task 3, Test 1) shows `Terminal::new` returning `(terminal, _wakeup_rx, _image_rx)` — a 3-tuple. The actual `Terminal::new` signature returns `Result<(Terminal, mpsc::Receiver<PendingImage>), std::io::Error>` — a 2-tuple with the wakeup channel internal. The integration test correctly uses `(terminal, _image_rx)` (2-tuple). This is a spec artifact (the plan was written before `Terminal::new` finalized its API); the implementation chose a better design. No code defect, but the plan's example code no longer matches reality. Worth noting for future spec authors.
  - Remediation: No code change needed. This finding is informational.

### Suggestions

- **`lib.rs` does not expose `context.rs`, `ai_detect.rs`, or `layout.rs`** at `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/lib.rs`.
  - The lib target currently re-exports only `Terminal` and `PreFilter`. Future integration tests or tool consumers will need `PaneContext`, `collect_sibling_contexts`, `AiAgentState`, and `PaneId` to write end-to-end tests for the AI features verified in Task 1. Adding these to `lib.rs` now is low-cost and removes friction later.
  - Remediation: Add `pub use context::{PaneContext, SiblingContext, collect_sibling_contexts};`, `pub use ai_detect::{AiAgentState, AiAgentKind};`, `pub use layout::PaneId;` to `src/lib.rs`, along with the necessary `pub(crate) mod` declarations.

- **`format_context_osc7770` does not escape `cwd` paths in JSON output** at `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/context.rs:196–199`.
  - `format!("\"{}\"", p.display())` does not escape backslashes or double-quotes in the path. On macOS/Linux this is uncommon but possible (paths with `"` or `\` in directory names). The output is consumed by MCP tool callers that parse JSON strictly.
  - Remediation: Use `serde_json::to_string(p.to_str().unwrap_or(""))` for the CWD value, or apply the same escaping already done for `cmd_json` (`replace('\\', "\\\\").replace('"', "\\\"")`) to the path string.

- **Integration test `prefilter_osc7770_start_content_end_sequence` interprets content as passthrough** at `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/tests/engine_migration.rs:220–224`.
  - The test asserts `out.passthrough == b"fn main() {}"`. This means the content between the OSC 7770 `start` and `end` sequences is passed through to the terminal engine unchanged — the PreFilter does not accumulate it. This is the correct design for the PreFilter (it intercepts OSC delimiters, not the content between them), but the test assertion is architecturally surprising and not commented. A future developer might change the PreFilter to intercept the content body, breaking this test without understanding why it was written this way.
  - Remediation: Add a comment above the assertion explaining that content between OSC 7770 delimiters passes through to `alacritty_terminal` for rendering, while only the delimiters are intercepted as side-channel events.

---

## Issues Appended

The following findings have been appended to `/Users/lgbarn/Personal/arcterm/.shipyard/ISSUES.md`:

- ISSUE-024: `child_pid()` return type `Option<u32>` is always `Some` — misleads callers (Important)
- ISSUE-025: `push_output_line` and `set_command` unwired in `main.rs` — output ring and command field always empty at runtime (Important)

---

## Summary

**Verdict:** APPROVE

All three tasks are correctly implemented. The old crates are gone, the workspace has exactly 3 members, clippy is clean with `-D warnings`, all 415 tests pass, and the integration test file covers all five scenarios specified in the plan (with 3 PTY-dependent tests appropriately marked `#[ignore]`). The migration from `arcterm-core/vt/pty` to `alacritty_terminal` is functionally complete.

Two Important findings are flagged for follow-up before phase close: the infallible-but-`Option`-typed `child_pid()` return and the unwired `push_output_line`/`set_command` methods that leave the context output ring permanently empty at runtime.

**Critical:** 0 | **Important:** 3 (1 informational) | **Suggestions:** 3
