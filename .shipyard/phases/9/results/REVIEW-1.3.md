---
plan: "1.3"
phase: foundation-fixes
reviewer: reviewer-agent
commit: 6f79e5f
---

## Stage 1: Spec Compliance
**Verdict:** PASS

### Task 1: Add ISSUE-001 regression tests (write-after-shutdown + idempotency)
- Status: PASS
- Evidence:
  - `arcterm-pty/src/session.rs:531-543` — `test_write_after_explicit_shutdown` is present. Uses `default_size()` helper (line 282) and `PtySession::new(default_size(), None, None)` exactly as specified. Calls `session.shutdown()`, then `session.write(b"data after shutdown")`, asserts `result.is_err()`, and asserts `result.unwrap_err().kind() == ErrorKind::BrokenPipe`.
  - `arcterm-pty/src/session.rs:545-551` — `test_shutdown_is_idempotent` is present. Calls `shutdown()` twice with no panic assertion needed beyond test completion.
  - Both tests reside inside `#[cfg(test)] mod tests` as specified.
  - The plan noted "adjust GridSize construction to match actual API (check if `default_size()` helper exists)." The builder correctly identified and used `default_size()` at line 282 instead of duplicating `GridSize { rows: 24, cols: 80 }`.
- Notes: Implementation matches the spec action field exactly. The done criteria ("Both tests pass. `write()` returns `BrokenPipe` after `shutdown()`. Double `shutdown()` does not panic.") is satisfied by the implementation and confirmed by SUMMARY-1.3 verification output showing 12/12 passing tests and clean clippy.

---

## Stage 2: Code Quality

### Critical
None.

### Important
None.

### Suggestions
- **`async fn` with no `await` in both new tests** at `arcterm-pty/src/session.rs:531,545`
  - Both `test_write_after_explicit_shutdown` and `test_shutdown_is_idempotent` are declared `async` and decorated with `#[tokio::test]`, but neither contains an `await` point. The tests could be plain `#[test]` functions with no async runtime overhead.
  - Remediation: Change to `#[test] fn test_write_after_explicit_shutdown()` and `#[test] fn test_shutdown_is_idempotent()`. Note that `shutdown()` internally calls `self.child.wait()` (a blocking syscall), which is fine on a synchronous thread but would be a concern if the test ever moved into an async context — keeping them `async` could be argued as a forward-compatibility choice. Low priority either way.

---

## Summary
**Verdict:** APPROVE

Both regression tests are correctly implemented per the spec, use established test-module conventions (`default_size()`, `PtySession::new`, `#[tokio::test]`), and directly exercise the ISSUE-001 fix path through `write()` returning `BrokenPipe` when `self.writer` is `None` post-`shutdown()`. No correctness, security, or quality issues found.

Critical: 0 | Important: 0 | Suggestions: 1
