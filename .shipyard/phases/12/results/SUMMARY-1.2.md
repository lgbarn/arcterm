# SUMMARY-1.2: Build the Pre-Filter Byte Stream Scanner

**Plan:** PLAN-1.2
**Branch:** phase-12-engine-swap
**Worktree:** /Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap
**Completed:** 2026-03-16

---

## What Was Done

### Task 1 — Implement PreFilter state machine

Created `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/prefilter.rs` with:

- `PreFilter` struct with a 6-state machine (`Normal`, `PendingEsc`, `InApc`, `InApcPendingEsc`, `InOsc`, `InOscPendingEsc`)
- `advance(&mut self, input: &[u8]) -> PreFilterOutput` method that classifies raw PTY bytes per call
- `PreFilterOutput` with four buckets: `passthrough`, `apc_payloads`, `osc7770_params`, `osc133_events`
- `Osc133Event` enum: `PromptStart`, `CommandStart`, `CommandExecuted`, `CommandFinished(Option<i32>)`
- Private `dispatch_osc` routing OSC sequences: OSC 7770 → `osc7770_params`, OSC 133 → `osc133_events`, all others → `passthrough` (reconstructed with BEL terminator)
- `reconstruct_osc_passthrough` for non-intercepted OSC sequences
- `parse_osc133` free function to convert the raw parameter buffer to `Osc133Event`

The implementation was modeled on the existing `ApcScanner` in `arcterm-vt/src/processor.rs`, extending it to also handle OSC sequences.

**Commit:** `7b7ef26 shipyard(phase-12): implement PreFilter byte-stream state machine`

### Task 2 — Comprehensive tests

Added `#[cfg(test)] mod tests` with 14 tests covering all required scenarios:

| Test | Description |
|---|---|
| `test_plain_ascii_passthrough` | Plain ASCII bytes pass through unmodified |
| `test_apc_complete` | `ESC _ payload ESC \` emits one APC payload |
| `test_apc_split` | APC sequence split across two `advance` calls |
| `test_osc7770_bel` | OSC 7770 with BEL terminator |
| `test_osc7770_st` | OSC 7770 with ST terminator |
| `test_osc7770_split` | OSC 7770 split across two calls |
| `test_osc133_d_with_exit_code` | `133;D;0` → `CommandFinished(Some(0))` |
| `test_osc133_a` | `133;A` → `PromptStart` |
| `test_osc133_b` | `133;B` → `CommandStart` |
| `test_osc133_c` | `133;C` → `CommandExecuted` |
| `test_osc133_d_no_exit_code` | `133;D` → `CommandFinished(None)` |
| `test_mixed_sequence` | text + APC + text + OSC 7770 in one input |
| `test_non_intercepted_osc_passthrough` | `OSC 0;title BEL` reconstructed in passthrough |
| `test_csi_passthrough` | `ESC [` (CSI) passes through unchanged |

All 14 tests pass: `cargo test -p arcterm-app -- prefilter` → `test result: ok. 14 passed; 0 failed`

The tests were written alongside the implementation in the initial file creation (co-located with the state machine as is idiomatic Rust). Both Task 1 and Task 2 are covered by commit `7b7ef26`.

### Task 3 — Register PreFilter module

`mod prefilter;` was already present in `arcterm-app/src/main.rs` as part of commit `33cf57a` (PLAN-1.1: relocate ContentType enum), which anticipated this module. No additional change was needed.

**Verification:** `cargo check -p arcterm-app` passes with only expected dead_code warnings (the module is registered but not yet consumed by terminal.rs — that is Wave 2 work).

---

## Deviations and Findings

### Pre-existing PLAN-1.1 build failure (resolved externally)

When running `cargo test -p arcterm-app` early in the session, a `ContentType` type mismatch error appeared at `arcterm-app/src/main.rs:1459`. Investigation showed:

- The error was caused by a stale incremental build cache from the PLAN-1.1 `ContentType` relocation commit (`33cf57a`)
- The actual source file was already correct: the `vt_ct_to_render_ct` bridge function existed at line 3442
- Re-running `cargo check -p arcterm-app` with a fresh cache confirmed the file was correct and the package compiled cleanly

**Root cause:** Rust's incremental compilation served a cached error from a previous state. The fix was a clean rebuild (triggered by touching the source).

### `mod prefilter;` pre-added by PLAN-1.1

The PLAN-1.1 commit pre-emptively added `mod prefilter;` to `main.rs`. Task 3 was therefore already satisfied. This is documented as a no-op for PLAN-1.2 Task 3.

### `libprefilter.rlib` artifact

A `libprefilter.rlib` appeared in the worktree root from a manual `rustc` invocation used to verify standalone compilation of `prefilter.rs` during debugging. This file is not tracked by git (`.gitignore` covers `*.rlib`) and has no impact.

---

## Final State

| File | Status |
|---|---|
| `arcterm-app/src/prefilter.rs` | Created, 516 lines, committed |
| `arcterm-app/src/main.rs` | `mod prefilter;` already present from PLAN-1.1 |

**Verification commands:**
- `cargo check -p arcterm-app` — passes (7 expected dead_code warnings for unused module)
- `cargo test -p arcterm-app -- prefilter` — `14 passed; 0 failed`
