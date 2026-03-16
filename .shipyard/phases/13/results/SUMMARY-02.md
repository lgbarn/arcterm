---
plan: "02"
phase: renderer-optimization
status: complete
commits:
  - f5f8869
  - 9ac1ce5
---

# SUMMARY-02 — Latency Trace Instrumentation

## What Was Done

Completed the latency measurement chain behind the `latency-trace` feature flag by adding two instrumentation points that were previously unmeasured.

### Task 1 — Wakeup send timestamps (`arcterm-app/src/terminal.rs`)

Added `[latency] wakeup sent at {:?}` log lines gated by `#[cfg(feature = "latency-trace")]` in `ArcTermEventListener::send_event`, after both `wakeup_tx.send(())` calls:
- `Event::Wakeup` arm (line 114)
- `Event::ChildExit` arm (line 127)

This records the timestamp at which the reader thread signals the main thread, filling the previously unmeasured gap between `wakeup_tx.send()` and the `about_to_wait` drain.

### Task 2 — Snapshot lock duration (`arcterm-app/src/main.rs`)

Added `t_snap = TraceInstant::now()` before `terminal.lock_term()` and `log::debug!("[latency] snapshot acquired in {:?}", t_snap.elapsed())` after the `snapshot_from_term(&*term)` call in the per-pane wakeup processing block (around line 1511). Both lines are gated by `#[cfg(feature = "latency-trace")]`.

`TraceInstant` is the existing alias for `std::time::Instant` already imported under the feature flag at `main.rs:204`.

## Verification

| Build | Result |
|---|---|
| `cargo check -p arcterm-app --features latency-trace` | Finished, no warnings |
| `cargo check -p arcterm-app` (default, no feature) | Finished, no warnings |

## Deviations

None. Implementation matched the plan exactly. The `detect_snapshot` block at line 1511 was the correct call site for Task 2 — it is the only `lock_term()` + `snapshot_from_term()` pair in the `had_wakeup || has_exited` branch.

## Final Latency Chain

When running with `RUST_LOG=debug cargo run -p arcterm-app --features latency-trace`, the full chain is now observable:

```
[latency] wakeup sent at <Instant>          ← NEW (terminal.rs)
[latency] PTY wakeup processed in <dur>     ← pre-existing (main.rs:1650)
[latency] snapshot acquired in <dur>        ← NEW (main.rs ~1514)
[latency] frame submitted in <dur>          ← pre-existing
[latency] key -> frame presented: <dur>     ← pre-existing
```
