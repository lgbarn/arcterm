---
plan: "1.3"
phase: foundation-fixes
status: completed
commit: 6f79e5f
---

# SUMMARY-1.3 — PTY Regression Test (ISSUE-001)

## What Was Done

Added two regression tests to `arcterm-pty/src/session.rs` in the existing `#[cfg(test)] mod tests` block:

1. **`test_write_after_explicit_shutdown`** — Spawns a PTY session, calls `shutdown()`, then asserts that `write()` returns `Err` with `ErrorKind::BrokenPipe`. Confirms the ISSUE-001 fix is exercised by the test suite.

2. **`test_shutdown_is_idempotent`** — Spawns a PTY session, calls `shutdown()` twice, and confirms no panic occurs. Verifies the `Option::take()` approach handles repeated calls safely.

## Deviations

None. Tests used the existing `default_size()` helper (line 282) and `PtySession::new(size, None, None)` constructor pattern matching all other tests in the module.

## Verification

```
cargo test -p arcterm-pty -- test_write_after_explicit_shutdown  → PASS
cargo test -p arcterm-pty -- test_shutdown_is_idempotent         → PASS
cargo test -p arcterm-pty                                        → 12/12 PASS
cargo clippy -p arcterm-pty -- -D warnings                       → CLEAN
```

## Commit

`6f79e5f shipyard(phase-9): add PTY shutdown regression tests (ISSUE-001)`
