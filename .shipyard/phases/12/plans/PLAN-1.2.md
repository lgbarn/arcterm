# Plan 1.2: Build the Pre-Filter Byte Stream Scanner

## Context

Alacritty's `EventLoop` silently drops OSC 7770, OSC 133, and APC (Kitty graphics) sequences — it has no extensibility hooks. The pre-filter sits between the raw PTY file descriptor and alacritty's EventLoop, intercepting these sequences and dispatching them to side channels while passing all other bytes through to a pipe that the EventLoop reads from.

This plan builds the pre-filter as a standalone module in `arcterm-app` with comprehensive tests. It does not wire the pre-filter into the terminal yet — that happens in Wave 2.

The pre-filter is modeled on the existing `ApcScanner` in `arcterm-vt/src/processor.rs` but extended to also handle OSC sequences (7770 and 133).

## Dependencies

None — this is Wave 1. Can execute in parallel with Plan 1.1.

## Tasks

### Task 1: Implement PreFilter state machine
**Files:** `arcterm-app/src/prefilter.rs` (new)
**Action:** create
**Description:**
Build a `PreFilter` struct with an `advance(&mut self, input: &[u8]) -> PreFilterOutput` method. The state machine must handle:

1. **APC sequences** (`ESC _ <payload> ESC \`): Collect the payload into a `Vec<u8>`. On completion, emit via `apc_payloads: Vec<Vec<u8>>` in the output. This replaces the existing `ApcScanner`.

2. **OSC 7770 sequences** (`ESC ] 7770 ; <params> BEL` or `ESC ] 7770 ; <params> ESC \`): Collect the parameter string. On completion, emit via `osc7770_params: Vec<String>` in the output.

3. **OSC 133 sequences** (`ESC ] 133 ; <type> BEL` or `ESC ] 133 ; <type> ESC \`): Collect the type character and any parameters. On completion, emit via `osc133_events: Vec<Osc133Event>` in the output. Define `Osc133Event` enum with variants `PromptStart`, `CommandStart`, `CommandExecuted`, `CommandFinished(Option<i32>)` (the D variant carries exit code).

4. **All other bytes**: Passed through unmodified into `passthrough: Vec<u8>` in the output.

The state machine must handle:
- Partial sequences across read boundaries (stateful — each `advance` call continues from the previous state)
- Batching: runs of non-ESC bytes are collected as a single passthrough slice for performance
- Nested edge cases: an ESC inside an APC that is not followed by `\` resumes the APC payload
- BEL (0x07) as an OSC terminator in addition to ST (ESC \)

Define `PreFilterOutput`:
```rust
pub struct PreFilterOutput {
    pub passthrough: Vec<u8>,
    pub apc_payloads: Vec<Vec<u8>>,
    pub osc7770_params: Vec<String>,
    pub osc133_events: Vec<Osc133Event>,
}
```

**Acceptance Criteria:**
- Module compiles: `cargo check -p arcterm-app`
- State machine correctly identifies APC, OSC 7770, OSC 133, and passthrough bytes
- Handles partial sequences across multiple `advance` calls

### Task 2: Write comprehensive tests for PreFilter
**Files:** `arcterm-app/src/prefilter.rs` (tests module)
**Action:** modify (add tests)
**Description:**
Add a `#[cfg(test)] mod tests` section covering:

1. **Passthrough**: Plain ASCII bytes pass through unmodified.
2. **APC complete**: `ESC _ payload ESC \` produces one APC payload, no passthrough for the APC bytes.
3. **APC split across calls**: Feed `ESC _` in one call, `payload ESC \` in the next. Verify payload is assembled correctly.
4. **OSC 7770 complete (BEL terminated)**: `ESC ] 7770 ; type=code ; lang=rs BEL` produces one osc7770_params entry.
5. **OSC 7770 complete (ST terminated)**: `ESC ] 7770 ; start ; type=code ESC \` produces one entry.
6. **OSC 7770 split across calls**: Feed partial sequence, verify state is preserved.
7. **OSC 133 D with exit code**: `ESC ] 133 ; D ; 0 BEL` produces `CommandFinished(Some(0))`.
8. **OSC 133 A/B/C**: Verify each variant.
9. **Mixed sequence**: Input containing plain text, then APC, then more text, then OSC 7770 — verify passthrough contains only non-intercepted bytes and side channels have the correct payloads.
10. **Non-intercepted OSC**: `ESC ] 0 ; title BEL` passes through to passthrough (not intercepted — only 7770 and 133 are intercepted).
11. **ESC followed by non-special**: `ESC [` (CSI) passes through to passthrough unchanged.

**Acceptance Criteria:**
- All tests pass: `cargo test -p arcterm-app -- prefilter`
- Tests cover all state transitions: Normal, PendingEsc, InApc, InApcPendingEsc, InOsc, InOscPendingEsc
- Edge case: empty input produces empty output

### Task 3: Register PreFilter module in arcterm-app
**Files:** `arcterm-app/src/main.rs` (or `lib.rs`)
**Action:** modify
**Description:**
Add `mod prefilter;` to the arcterm-app module tree. Ensure the module is visible to `terminal.rs` (which will use it in Wave 2).

**Acceptance Criteria:**
- `cargo check -p arcterm-app` succeeds
- `cargo test -p arcterm-app -- prefilter` runs all prefilter tests and passes

## Verification

```bash
cargo test -p arcterm-app -- prefilter
```

All pre-filter tests pass. The module is registered and ready for integration in Wave 2.
