---
phase: renderer-optimization
plan: "02"
wave: 2
dependencies: ["01"]
must_haves:
  - Latency measurement for key-to-screen exists
  - Wakeup-send timestamp fills the unmeasured gap in the latency chain
  - Snapshot lock duration is measured
files_touched:
  - arcterm-app/src/terminal.rs
  - arcterm-app/src/main.rs
tdd: false
---

# PLAN-02 — Latency Trace Instrumentation

## Goal

Complete the latency measurement chain behind the existing `latency-trace` feature flag. Two gaps exist in the current instrumentation: (1) the time from `wakeup_tx.send()` to `about_to_wait` drain is unmeasured, and (2) the duration of `lock_term()` + `snapshot_from_term()` is unmeasured. After this plan, the full key-to-screen and PTY-byte-to-screen chains are observable via `RUST_LOG=debug`.

## Context

The existing `latency-trace` instrumentation (see RESEARCH.md Section 4) already covers:
- `key_press_t0` capture at keyboard input (main.rs:2387-2389)
- `t0` at start of per-pane wakeup processing (main.rs:1446-1447)
- PTY wakeup processing duration (main.rs:1649-1650)
- `t0` at start of RedrawRequested (main.rs:1988-1989)
- Frame submission + key-to-frame presented (main.rs:2358-2375)
- Key-to-PTY-write (main.rs:2707-2712)
- Cold start to first frame (main.rs:2358-2375)

The unmeasured gap is: `wakeup_tx.send()` (terminal.rs:113) to `about_to_wait` drain.

## Tasks

<task id="1" files="arcterm-app/src/terminal.rs:97-126" tdd="false">
  <action>Add a timestamp at the wakeup send point in `ArcTermEventListener::send_event`:

1. After line 113 (`let _ = self.wakeup_tx.send(());` in the `Event::Wakeup` arm), add:
   ```rust
   #[cfg(feature = "latency-trace")]
   log::debug!("[latency] wakeup sent at {:?}", std::time::Instant::now());
   ```

2. After line 125 (`let _ = self.wakeup_tx.send(());` in the `Event::ChildExit` arm), add the same log line.

This runs in the alacritty reader thread. `log::debug!` is thread-safe (env_logger serializes writes). The `#[cfg]` gate ensures zero cost in release builds.
  </action>
  <verify>cd /Users/lgbarn/Personal/arcterm && cargo check -p arcterm-app --features latency-trace 2>&1 | tail -5</verify>
  <done>`cargo check` succeeds with the `latency-trace` feature enabled. `terminal.rs` contains `[latency] wakeup sent` log lines in both the `Wakeup` and `ChildExit` arms, gated by `#[cfg(feature = "latency-trace")]`.</done>
</task>

<task id="2" files="arcterm-app/src/main.rs:1500-1520" tdd="false">
  <action>Add a timestamp around the snapshot acquisition in `about_to_wait`. Find the `lock_term()` + `snapshot_from_term()` call site inside the per-pane wakeup processing block (approximately line 1512, inside the `if had_wakeup` branch). Add:

```rust
#[cfg(feature = "latency-trace")]
let t_snap = TraceInstant::now();
```

before the `let term = terminal.lock_term();` call, and:

```rust
#[cfg(feature = "latency-trace")]
log::debug!("[latency] snapshot acquired in {:?}", t_snap.elapsed());
```

after the `snapshot_from_term(&*term)` call and term lock drop. `TraceInstant` is already imported as `std::time::Instant` under the feature flag (main.rs:204).
  </action>
  <verify>cd /Users/lgbarn/Personal/arcterm && cargo check -p arcterm-app --features latency-trace 2>&1 | tail -5</verify>
  <done>`cargo check` succeeds with the `latency-trace` feature enabled. The `about_to_wait` wakeup processing block contains `[latency] snapshot acquired` log output gated by `#[cfg(feature = "latency-trace")]`.</done>
</task>

## Verification — Feature-Gated Build

After both tasks:

```bash
cd /Users/lgbarn/Personal/arcterm && cargo build -p arcterm-app --features latency-trace 2>&1 | tail -5
```

And confirm no cost in default builds:

```bash
cd /Users/lgbarn/Personal/arcterm && cargo build -p arcterm-app 2>&1 | tail -5
```

Both must succeed with no warnings.

## Manual Smoke Test

Run with latency tracing enabled:

```bash
cd /Users/lgbarn/Personal/arcterm && RUST_LOG=debug cargo run -p arcterm-app --features latency-trace
```

Type a few characters. The log output should show the full latency chain:

```
[latency] wakeup sent at <Instant>
[latency] PTY wakeup processed in <duration>
[latency] snapshot acquired in <duration>
[latency] frame submitted in <duration>
[latency] key -> frame presented: <duration>
```

The `wakeup sent` timestamp and `snapshot acquired` duration are the two new additions. All other lines are pre-existing.
