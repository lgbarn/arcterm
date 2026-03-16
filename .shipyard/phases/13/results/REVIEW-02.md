---
plan: "02"
phase: renderer-optimization
reviewer: claude
verdict: APPROVE
---

# REVIEW-02 — Latency Trace Instrumentation

## Stage 1: Spec Compliance
**Verdict:** PASS

### Task 1: Add wakeup-send timestamps in `ArcTermEventListener::send_event`
- Status: PASS
- Evidence: `/Users/lgbarn/Personal/arcterm/arcterm-app/src/terminal.rs:114-115` — `#[cfg(feature = "latency-trace")]` gate followed by `log::debug!("[latency] wakeup sent at {:?}", std::time::Instant::now());` inserted immediately after `let _ = self.wakeup_tx.send(());` in the `Event::Wakeup` arm. The identical pattern appears at lines 128-129 in the `Event::ChildExit` arm, after the `wakeup_tx.send(())` call.
- Notes: Placement is correct — the log fires after the send succeeds (or is discarded), not before. `std::time::Instant::now()` is called inline rather than captured before the send; because `send()` on an `std_mpsc::Sender<()>` is near-instantaneous, this is functionally equivalent to capturing immediately after and does not affect measurement fidelity. The `#[cfg]` gate is on the correct attribute, not a `cfg!()` macro, so the compiler eliminates the block entirely in non-feature builds.

### Task 2: Add snapshot acquisition timing in `about_to_wait`
- Status: PASS
- Evidence: `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1511-1518` — `#[cfg(feature = "latency-trace")] let t_snap = TraceInstant::now();` is placed before the `let detect_snapshot = { let term = terminal.lock_term(); ... }` block at line 1513. `#[cfg(feature = "latency-trace")] log::debug!("[latency] snapshot acquired in {:?}", t_snap.elapsed());` follows immediately after the closing `};` at line 1517. `TraceInstant` resolves to `std::time::Instant` via the alias at `main.rs:204` under the same feature flag.
- Notes: The instrumented call site (`detect_snapshot` block) is the only `lock_term()` + `snapshot_from_term()` pair in the `had_wakeup || has_exited` processing branch, as stated in the SUMMARY. The three other `lock_term` call sites in `main.rs` (lines ~2081, ~2121, ~2407) are in different arms (exit-banner construction, per-pane snapshot for render, copy-selection) and are correctly outside the scope of this task. The `t_snap` variable and the elapsed log are on separate `#[cfg]` attributes (not a single attribute spanning both), which is the idiomatic Rust pattern when the two lines are not adjacent; here they are adjacent, so a single conditional block would also be valid — this is a style-level observation only, not a defect.

### Verification — Both builds pass
- Status: PASS
- Evidence: `cargo check -p arcterm-app --features latency-trace` completes `Finished dev profile` with no warnings. `cargo check -p arcterm-app` (default, no feature) also completes `Finished dev profile` with no warnings (cached). Both confirmed by live check runs during this review.

### Integration — No conflicts with PLAN-01
- Status: PASS
- Evidence: PLAN-01 touched `arcterm-render/src/text.rs`, `arcterm-render/src/renderer.rs`, `arcterm-render/src/gpu.rs`. Its single `main.rs` change (Task 2-C) was a no-op per SUMMARY-01. PLAN-02 inserts at `main.rs:1511-1518` (inside the per-pane wakeup processing block) and `terminal.rs:114-115`, `128-129`. These regions do not overlap with any PLAN-01 modification. The `pane_row_hashes` dirty-row cache inserted by PLAN-01 is keyed at a different loop level; the `t_snap` timing wraps the `detect_snapshot` block that feeds auto-detection, which is downstream of the cache and unaffected by it.

---

## Stage 2: Code Quality

### Critical
None.

### Important
None.

### Suggestions

- **`t_snap` captured after `send()` rather than before `lock_term()`** at `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1511`
  - `t_snap` is meant to measure the combined duration of `lock_term()` + `snapshot_from_term()`. It is captured at line 1511, one line above the lock block (line 1513), with only the unconditional `let cursor_row = terminal.cursor_row();` between them (line 1510). `cursor_row()` takes a trivial `Mutex::lock`, so the measurement error is negligible in practice. If this becomes a precision concern in the future, move `t_snap` to be immediately before `let detect_snapshot = {` (line 1513) rather than two lines above it, eliminating the `cursor_row()` lock from the measured window entirely.

- **`[latency] wakeup sent at {:?}` logs `Instant::now()` as an opaque `Instant` debug value** at `/Users/lgbarn/Personal/arcterm/arcterm-app/src/terminal.rs:115` and `:129`
  - The format `{:?}` on `std::time::Instant` prints the OS-level representation (e.g., `Instant { tv_sec: 12345, tv_nsec: 678900000 }` on Linux, or `Instant(...)` on macOS), which is not directly human-readable as wall time. The plan's smoke-test chain pairs this with the `PTY wakeup processed in <duration>` line (in `main.rs`) to measure the gap; because both ends are `Instant` values, the gap is computable by the observer. No change is required for correctness, but a follow-up refinement — logging elapsed time since an epoch or printing a monotonic timestamp in microseconds — would make the log directly readable without cross-referencing two lines.

---

## Summary
**Verdict:** APPROVE

Both instrumentation points are implemented exactly as specified: the `wakeup sent` timestamp is present in both `Wakeup` and `ChildExit` arms of `send_event`, and the `snapshot acquired` duration wraps the correct `lock_term()` + `snapshot_from_term()` call site in `about_to_wait`. All `#[cfg(feature = "latency-trace")]` gates are correct, both builds pass with no warnings, and there are no conflicts with PLAN-01.

Critical: 0 | Important: 0 | Suggestions: 2
