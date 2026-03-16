# Review: Plan 1.2

**Reviewer:** Claude (code review agent)
**Date:** 2026-03-16
**Branch:** phase-12-engine-swap
**File reviewed:** `arcterm-app/src/prefilter.rs` (516 lines, commit `7b7ef26`)

---

## Stage 1: Spec Compliance

**Verdict:** PASS

### Task 1: Implement PreFilter state machine

- Status: PASS
- Evidence: `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/prefilter.rs` contains `pub struct PreFilter` with `pub fn advance(&mut self, input: &[u8]) -> PreFilterOutput`. The six required states (`Normal`, `PendingEsc`, `InApc`, `InApcPendingEsc`, `InOsc`, `InOscPendingEsc`) are defined at lines 69–89.
- Notes:
  - `PreFilterOutput` (lines 41–50) has exactly the four fields specified: `passthrough: Vec<u8>`, `apc_payloads: Vec<Vec<u8>>`, `osc7770_params: Vec<String>`, `osc133_events: Vec<Osc133Event>`.
  - `Osc133Event` (lines 22–34) has the four required variants: `PromptStart`, `CommandStart`, `CommandExecuted`, `CommandFinished(Option<i32>)`.
  - APC interception (lines 136–177): `ESC _` enters `InApc`; `ESC \` within APC completes the payload; ESC followed by any non-`\` inside APC includes both bytes as payload and stays in `InApc` — satisfying the "nested ESC" edge case from the spec.
  - OSC 7770 and OSC 133 interception (lines 181–263): both BEL (0x07) and ST (`ESC \`) are handled as terminators.
  - Non-intercepted OSC sequences are reconstructed and passed to `passthrough` via `reconstruct_osc_passthrough`.
  - Partial sequences across read boundaries: the state machine is stateful (`&mut self`); `self.buf` accumulates across calls.

### Task 2: Write comprehensive tests

- Status: PASS
- Evidence: `#[cfg(test)] mod tests` at lines 322–516 contains 14 tests. All 11 required plan scenarios are covered:
  1. `test_plain_ascii_passthrough` — scenario 1
  2. `test_apc_complete` — scenario 2
  3. `test_apc_split` — scenario 3
  4. `test_osc7770_bel` — scenario 4
  5. `test_osc7770_st` — scenario 5
  6. `test_osc7770_split` — scenario 6
  7. `test_osc133_d_with_exit_code` — scenario 7
  8. `test_osc133_a`, `test_osc133_b`, `test_osc133_c`, `test_osc133_d_no_exit_code` — scenario 8
  9. `test_mixed_sequence` — scenario 9
  10. `test_non_intercepted_osc_passthrough` — scenario 10
  11. `test_csi_passthrough` — scenario 11
- Notes: The plan's acceptance criteria for Task 2 explicitly states "Edge case: empty input produces empty output." No dedicated `test_empty_input` test exists. This is a minor gap (the behavior is correct since the `for` loop over `input` is a no-op on an empty slice) but the spec required it. Logged as a minor finding below.

### Task 3: Register PreFilter module in arcterm-app

- Status: PASS
- Evidence: `mod prefilter;` is present at line 190 of `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/main.rs`. The SUMMARY documents that PLAN-1.1 commit `33cf57a` pre-added this declaration, which is an acceptable no-op for PLAN-1.2 Task 3.

---

## Stage 2: Code Quality

### Critical

None.

### Minor

**1. ST-terminated non-intercepted OSC sequences are reconstructed with BEL, not ST**

- File: `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/prefilter.rs`, lines 269–276 (`reconstruct_osc_passthrough`) and lines 265–268 (doc comment).
- `reconstruct_osc_passthrough` always appends `0x07` (BEL) as the terminator regardless of whether the original sequence was BEL-terminated or ST-terminated. The doc comment acknowledges this with "The terminal engine accepts both." That assumption holds for most OSC sequences (OSC 0, OSC 1, OSC 2) but is not universally true. OSC sequences embedded inside tmux's DCS passthrough, or sequences handled by applications watching raw terminal output, may reject BEL as a substitute for ST. More concretely: if a future non-intercepted OSC sequence is ST-only (e.g., an OSC with a payload containing BEL-like bytes), silently converting the terminator may cause the downstream engine to misparse it.
- Remediation: Track which terminator ended the sequence. Add a `last_osc_terminator: u8` field to `PreFilter` (default `0x07`), set it when BEL (`0x07`) or ST bytes arrive, and emit it in `reconstruct_osc_passthrough` instead of the hardcoded BEL. This is a one-field, three-line change.

**2. Missing `test_empty_input` test (plan acceptance criteria gap)**

- File: `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/prefilter.rs`, tests module (line 322).
- The Task 2 acceptance criteria explicitly requires a test that empty input produces empty output. No such test exists among the 14 present.
- Remediation: Add:
  ```rust
  #[test]
  fn test_empty_input() {
      let out = run(b"");
      assert!(out.passthrough.is_empty());
      assert!(out.apc_payloads.is_empty());
      assert!(out.osc7770_params.is_empty());
      assert!(out.osc133_events.is_empty());
  }
  ```

**3. `PreFilterOutput::new()` is private; Wave 2 callers cannot construct a default value**

- File: `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/prefilter.rs`, line 53.
- `PreFilterOutput::new()` is `fn new()` (private). The `Default` trait is not derived or implemented on `PreFilterOutput`. Wave 2 code in `terminal.rs` that needs to accumulate outputs across multiple `advance` calls (e.g. merging two reads before dispatching) cannot call `PreFilterOutput::default()` or `PreFilterOutput::new()` without adding its own construction. This is a minor API friction issue. `PreFilter` itself correctly derives `Default` (lines 279–283), but its output type does not.
- Remediation: Either derive `Default` on `PreFilterOutput` (all four fields are `Vec` types, which implement `Default`) or make `new()` pub. The simplest fix is `#[derive(Debug, Clone, PartialEq, Eq, Default)]` on line 40.

**4. No test covering ESC at the end of a buffer (partial PendingEsc across call boundary)**

- File: `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/prefilter.rs`, tests module.
- The plan explicitly mentions "ESC at end of buffer" as an edge case to verify. The `run_split` helper exists but no test calls it with a split exactly at the ESC byte (e.g., first call is `b"hello\x1b"`, second is `b"[0m"`). The code path through `PendingEsc` with the ESC as the last byte of a call works correctly (the state is preserved), but there is no test proving it.
- Remediation: Add:
  ```rust
  #[test]
  fn test_esc_at_buffer_boundary() {
      let out = run_split(b"hello\x1b", b"[0m");
      assert_eq!(out.passthrough, b"hello\x1b[0m");
      assert!(out.apc_payloads.is_empty());
  }
  ```

### Positive

- The six-state machine is clean and complete. State names and transition comments are precise and match the spec vocabulary exactly.
- `//!` module-level documentation (lines 1–14) clearly describes the pre-filter's role, the three sequence classes it intercepts, and the stateful read-boundary guarantee. This is exactly the level of documentation Wave 2 integrators will need.
- `parse_osc133` (lines 293–316) uses `splitn(3, ...)` correctly, ensuring that a `133;D;0;extra` sequence with spurious trailing fields does not panic or produce garbage — `parts[2]` gets "0;extra" and `parse::<i32>()` fails gracefully on the semicolon, yielding `CommandFinished(None)`. This is sensibly defensive.
- `String::from_utf8_lossy` for OSC 7770 params (line 241) is appropriate — PTY output can contain non-UTF-8 bytes and replacement characters are safer than hard failures.
- `Default` is implemented for `PreFilter` (lines 279–283), which is required by Wave 2 integration patterns where structs are default-initialized before configuration.
- No allocations on the fast path for passthrough-only input — a plain ASCII byte takes one `push` to `out.passthrough` with no branching through the escape machinery.
- PLAN-1.1 conflict: no conflicts. The two plans operated on completely disjoint files. The `mod prefilter;` pre-add by PLAN-1.1 is a harmless anticipation; it does not alter the PLAN-1.2 scope.

---

## Verdict: MINOR_ISSUES

The state machine is correct and the spec is substantially met. The four minor findings are: one behavioral deviation (ST→BEL terminator substitution in passthrough reconstruction), two missing test cases required by the acceptance criteria (empty input, ESC-at-boundary), and one API usability gap (`PreFilterOutput` not implementing `Default`). None of these block Wave 2 integration, but findings 1 and 2 should be resolved before the module is considered final.
