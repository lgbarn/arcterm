---
phase: foundation-fixes
plan: "1.3"
wave: 1
dependencies: []
must_haves:
  - ISSUE-001 regression test for write-after-shutdown returning BrokenPipe
files_touched:
  - arcterm-pty/src/session.rs
tdd: true
---

# PLAN-1.3 — PTY Regression Test (arcterm-pty)

## Context

ISSUE-001 is **already fixed** in the codebase. The `PtySession.writer` field is already `Option<Box<dyn Write + Send>>`, `shutdown()` uses `self.writer.take()`, and `write()` returns `Err(BrokenPipe)` when the writer is `None`.

The existing test `test_write_after_exit` (line 496) tests writing after the shell process dies naturally, but does not test the explicit `shutdown()` path. Phase 9 requires a direct regression test for `shutdown()` followed by `write()`.

This plan adds a single, focused regression test in `arcterm-pty/src/session.rs`.

## Dependencies

None. This plan touches only `arcterm-pty/src/session.rs` (test addition).

## Tasks

<task id="1" files="arcterm-pty/src/session.rs" tdd="true">
  <action>
  Add a regression test for ISSUE-001 in the existing `#[cfg(test)] mod tests` block (starts at line 276).

  ```rust
  #[tokio::test]
  async fn test_write_after_explicit_shutdown() {
      let size = arcterm_core::GridSize { rows: 24, cols: 80 };
      let (mut session, _rx) = PtySession::new(size, None, None)
          .expect("should spawn PTY session");
      session.shutdown();
      let result = session.write(b"data after shutdown");
      assert!(result.is_err(), "write after shutdown must fail");
      assert_eq!(
          result.unwrap_err().kind(),
          std::io::ErrorKind::BrokenPipe,
          "write after shutdown must return BrokenPipe"
      );
  }
  ```

  Use the same `PtySession::new` constructor pattern as existing tests. Adjust `GridSize` construction to match actual API (check if `default_size()` helper exists in the test module and use it if available).

  Also add a test confirming `shutdown()` is idempotent (calling it twice does not panic):
  ```rust
  #[tokio::test]
  async fn test_shutdown_is_idempotent() {
      let size = arcterm_core::GridSize { rows: 24, cols: 80 };
      let (mut session, _rx) = PtySession::new(size, None, None)
          .expect("should spawn PTY session");
      session.shutdown();
      session.shutdown();  // second call must not panic
  }
  ```
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-pty -- test_write_after_explicit_shutdown && cargo test -p arcterm-pty -- test_shutdown_is_idempotent</verify>
  <done>Both tests pass. `write()` returns `BrokenPipe` after `shutdown()`. Double `shutdown()` does not panic.</done>
</task>

## Final Verification

```bash
cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-pty && cargo clippy -p arcterm-pty -- -D warnings
```

All `arcterm-pty` tests pass (existing + new regression tests). Clippy is clean.
