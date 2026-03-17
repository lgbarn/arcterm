# Security Audit Report — Phase 15 (Performance Optimization)

## Executive Summary

**Verdict:** PASS (with advisory findings)
**Risk Level:** Low

The four changed files contain no exploitable vulnerabilities, no secrets, and no
dependency changes. The `AtomicI32` replacement for `Mutex<Option<i32>>` is
memory-safe and uses correct acquire/release ordering. The PTY batching change
preserves byte ordering and handles all exit paths. One advisory issue warrants
attention: when a pane is removed from the layout, its entry is not pruned from
`cached_snapshots`, leaving a stale snapshot in a `HashMap` that will persist
until the next redraw cycle tries to consume it. This is a minor memory hygiene
gap — not exploitable, but worth one line of cleanup.

### What to Do

| Priority | Finding | Location | Effort | Action |
|----------|---------|----------|--------|--------|
| 1 | Stale snapshot not removed on pane close | `main.rs:904-910`, `main.rs:1681-1686` | Trivial | Add `self.cached_snapshots.remove(id)` / `state.cached_snapshots.remove(&id)` alongside the other per-pane cleanup |
| 2 | `AtomicI32` sentinel `-1` collides with valid exit codes on some platforms | `terminal.rs:280` | Small | Use a separate `AtomicBool` for "exited" state, or widen to `AtomicI64` with a clearly out-of-range sentinel |
| 3 | Passthrough flush does not send wakeup on the error path | `terminal.rs:419-428` | Trivial | Add `wakeup_tx_for_reader.send(())` after flushing in the `Err(e)` branch before `break` |

### Themes
- Cleanup of the new `cached_snapshots` field is missing from two pane-removal code paths (workspace reload and per-frame exit detection).
- Atomic sentinel value `-1` is a valid `i32` exit code on some Unix systems; the assumption it will never be a real process exit code needs documentation or a structural fix.

---

## Detailed Findings

### Critical

_None._

### Important

_None._

### Advisory

**[A1] Stale entry in `cached_snapshots` when pane is removed**
- **Location:** `arcterm-app/src/main.rs:904-910` (workspace reload) and `main.rs:1681-1686` (per-frame exit loop)
- **Description:** `cached_snapshots` was added to `AppState` and is written in
  `about_to_wait` (line 1553) and consumed by `remove` in `RedrawRequested`
  (line 2129). However, both pane-removal blocks omit `cached_snapshots.remove(id)`.
  For the workspace-reload path this means all old snapshots survive the reload;
  for the exit path an exited pane's snapshot persists until a `RedrawRequested`
  event happens to call `cached_snapshots.remove` on that ID. The stale value is
  a `RenderSnapshot` (heap data) — not a reference — so there is no
  use-after-free, but it holds cloned terminal cell data for a pane that no
  longer exists.
- **Impact:** Minor memory leak (one snapshot per closed pane, bounded by pane
  count). In the workspace-reload path the leftover entries will be consumed
  harmlessly by the fallback `unwrap_or_else` on the next redraw, but the
  terminal they depict is the one that was just torn down — conceptually stale.
- **Remediation:** Add `self.cached_snapshots.remove(id)` at line 911 in the
  workspace-reload loop, and `state.cached_snapshots.remove(&id)` at line 1687
  in the pane-exit loop. Both are single-line additions consistent with the
  surrounding removal pattern.
- **CWE reference:** CWE-401 (Missing Release of Memory After Effective Lifetime)

---

**[A2] `AtomicI32` sentinel value `-1` overlaps valid process exit codes**
- **Location:** `arcterm-app/src/terminal.rs:280`, `terminal.rs:729`
- **Description:** The atomically-stored `exit_code` uses `-1` as the sentinel
  meaning "not yet exited". On Linux and macOS the raw wait-status is shifted
  so the effective exit code seen by the process is always 0-255. However,
  `ChildExit(code)` is fired by alacritty's vte layer which may pass the raw
  `i32` result of `waitpid` before bit-shifting; if that raw value is `-1`
  (e.g., `ECHILD` or `EINTR` from a failed `waitpid` call) the sentinel would
  collide and `has_exited()` would erroneously return `true` while the child is
  still running.
- **Impact:** False-positive exit detection: a pane could be closed while the
  shell is still alive. Low probability in practice because the upstream
  `EventListener::ChildExit` is only called on confirmed exit, but the
  assumption is undocumented and fragile.
- **Remediation:** Either (a) use an `AtomicBool` for the "has exited" flag
  separately from the stored code, or (b) store exit code as `AtomicI64` with
  sentinel `i64::MIN` (unreachable as a process exit code), or (c) add a
  comment citing the invariant that the upstream caller guarantees `-1` is never
  a valid exit code.
- **CWE reference:** CWE-682 (Incorrect Calculation) / design assumption gap

---

**[A3] PTY reader error path flushes passthrough but does not send wakeup**
- **Location:** `arcterm-app/src/terminal.rs:419-428`
- **Description:** In the `Err(e)` branch of the PTY reader loop the accumulator
  is flushed into the terminal (`parser.advance`) but `wakeup_tx_for_reader.send(())`
  is only called after the flush, not before the `break`. By contrast the
  `WouldBlock` path (line 414) correctly sends the wakeup. Currently the break
  is immediately followed by the wakeup send anyway — the loop exits and the
  send still runs. However the flush advances the parser state into `term` and
  then the wakeup fires from the `break`-path send at line 427, so this is
  functionally correct. The advisory is that the wakeup is placed after
  `passthrough_acc.clear()` (line 426) but before `wakeup_tx_for_reader.send(())`
  (line 427), which means any output flushed in that branch will not trigger a
  redraw until the thread-local send executes — consistent with other paths.
  This is correct but the asymmetry with the `WouldBlock` path (which sends
  inside the flush block) makes the control flow harder to audit. No behavioral
  bug exists.
- **Remediation:** Move the `wakeup_tx_for_reader.send(())` call to immediately
  after the `parser.advance` call inside the flush block, matching the
  `WouldBlock` pattern, for readability.

---

**[A4] `row_hashes` resize guard skips forced-reshape on terminal resize**
- **Location:** `arcterm-render/src/text.rs:260-262`
- **Description:** The old code called `row_hashes.resize(num_rows, u64::MAX)`
  unconditionally. `u64::MAX` is the "force reshape" sentinel — any hash
  comparison against it fails, guaranteeing re-shaping. The new code only
  resizes when `row_hashes.len() != num_rows`, which is correct for the steady
  state (avoids the write). However on the non-resize path the existing hashes
  are preserved exactly as before, so there is no regression. The guard is safe
  because `resize` only runs when the length changes (i.e., on actual terminal
  resize), at which point all rows would re-shape anyway due to new geometry.
  No bug; confirmed correct.
- **Remediation:** None required. Noting for completeness that the optimization
  is sound.

---

## Cross-Component Analysis

**`AtomicI32` + wakeup channel interaction:** The `ChildExit` path in
`ArcTermEventListener` stores the exit code with `Ordering::Release` then sends
on the wakeup channel. The main thread calls `has_exited()` using
`Ordering::Acquire`. This forms a correct release-acquire pair across the
`Arc<AtomicI32>`. The wakeup channel send happens after the store, so the
main thread observing the wakeup is guaranteed to see the stored value. Thread
safety is sound.

**Snapshot caching lifecycle:** `cached_snapshots` is written by `about_to_wait`
(per PTY wakeup) and consumed by `RedrawRequested` (via `remove`). The `remove`
call means each snapshot is used at most once per wakeup cycle — there is no
risk of a stale snapshot persisting across frames for a live pane. The only
accumulation risk is for dead panes (Advisory A1 above).

**PTY batching and OSC 7770 ordering:** The `osc7770_capture` buffer runs
parallel to `passthrough_acc`. Both are flushed when `should_flush` is true.
Since both extend from the same `output.passthrough` slice in document order and
both flush at the same time, the captured OSC 7770 text and the term-visible
passthrough remain in sync. No ordering inversion is possible.

**`build_quad_instances_at` signature change:** Changing from returning `Vec` to
accepting `&mut Vec` is purely a performance refactor. The function appends in
the same iteration order. No GPU data corruption is possible.

---

## Analysis Coverage

| Area | Checked | Notes |
|------|---------|-------|
| Code Security (OWASP) | Yes | No user-controlled input, no injection surfaces in these files |
| Secrets & Credentials | Yes | No secrets found in diff or changed files |
| Dependencies | N/A | No `Cargo.toml` changes in this diff |
| Infrastructure as Code | N/A | No IaC files changed |
| Docker/Container | N/A | No Dockerfile changes in scope |
| Configuration | Yes | No config-file changes; no debug flags introduced |

---

## Dependency Status

No dependencies were added or changed in this diff.

| Package | Version | Known CVEs | Status |
|---------|---------|-----------|--------|
| — | — | — | N/A |

---

## IaC Findings

Not applicable — no IaC files were modified.
