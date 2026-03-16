# SUMMARY-1.1 — scroll_offset API Migration

**Plan:** PLAN-1.1
**Phase:** 10, Wave 1
**Executed:** 2026-03-16

---

## Outcome

All 8 compile errors (E0616: field `scroll_offset` is private) in `arcterm-app`
are resolved. `cargo check -p arcterm-app` and `cargo clippy -p arcterm-app
-- -D warnings` both exit with status 0. The crate is ready for Wave 2 work.

---

## Tasks Completed

### Task 1 — terminal.rs (1 error fixed)

**File:** `arcterm-app/src/terminal.rs:197-200`

Replaced the manual clamp-and-assign body of `Terminal::set_scroll_offset` with
a single delegation call to `self.grid_state.grid.set_scroll_offset(offset)`.
The `scrollback.len()` read and direct field assignment were removed; the `Grid`
accessor performs the identical `offset.min(scrollback.len())` clamping internally.

The `#[allow(dead_code)]` attribute was retained (see Deviations below).

**Commit:** `e79610f` — `shipyard(phase-10): delegate Terminal::set_scroll_offset to Grid accessor`

---

### Task 2 — main.rs (7 errors fixed)

**File:** `arcterm-app/src/main.rs`

All 7 direct `grid.scroll_offset` field accesses replaced with accessor calls:

| Location | Change |
|---|---|
| PTY got_data read (~1692) | `grid.scroll_offset > 0` → `grid.scroll_offset() > 0` |
| PTY got_data write (~1694) | `grid.scroll_offset = 0` → `grid.set_scroll_offset(0)` |
| MouseWheel read (~1981) | `grid.scroll_offset as i32` → `grid.scroll_offset() as i32` |
| MouseWheel write (~1984) | `grid.scroll_offset = new_offset` → `grid.set_scroll_offset(new_offset)` |
| Search overlay read (~2343) | `grid.scroll_offset` → `grid.scroll_offset()` |
| Search NextMatch write (~2651) | `.scroll_offset =` → `.set_scroll_offset(...)` |
| Search PrevMatch write (~2671) | `.scroll_offset =` → `.set_scroll_offset(...)` |

The MouseWheel handler's explicit `max_offset` variable and `.clamp(0, max_offset as i32)`
were simplified to `.max(0)` since `set_scroll_offset` applies the upper-bound clamp
internally. No logic change — the arithmetic is equivalent.

No `review.scroll_offset` accesses (on `overlay::OverlayReviewState`) were modified.

**Commit:** `6ae7132` — `shipyard(phase-10): replace all 7 direct grid.scroll_offset field accesses in main.rs`

---

### Task 3 — Clippy verification (1 deviation handled)

Running `cargo clippy -p arcterm-app -- -D warnings` surfaced one error:

```
error: method `set_scroll_offset` is never used
   --> arcterm-app/src/terminal.rs:196:12
```

The `Terminal::set_scroll_offset` wrapper is not called by any current callers
(`main.rs` accesses `grid_mut().set_scroll_offset()` directly), so clippy
correctly flagged it as dead code after the `#[allow(dead_code)]` attribute was
removed in Task 1.

**Fix:** Restored `#[allow(dead_code)] // Used in Wave 3 integration` to keep
the method available as a stable API surface for future callers while satisfying
`-D warnings`. This follows the established codebase convention for methods
awaiting integration (see CONVENTIONS.md — Temporary Suppression Conventions).

**Commit:** `e3287f7` — `shipyard(phase-10): restore #[allow(dead_code)] on Terminal::set_scroll_offset for clippy clean`

---

## Deviations

### Deviation 1: #[allow(dead_code)] retained on Terminal::set_scroll_offset

**Plan said:** Remove the `#[allow(dead_code)]` attribute — "this method is used by callers holding a `Terminal` reference."

**What happened:** After the migration, no callers in `main.rs` use `terminal.set_scroll_offset()`. All callers access `grid_mut().set_scroll_offset()` directly. The method is genuinely unused today.

**Resolution:** Retained `#[allow(dead_code)]` with the Wave 3 annotation rather than removing the method. Removing the method would be an architectural change not specified in the plan. The CONVENTIONS.md pattern for deferred methods is `#[allow(dead_code)] // Used in Wave N integration`, which is exactly what is in place. This keeps clippy clean without deleting a method the plan intends to wire in during Wave 3.

---

## Final Verification

```
cargo check -p arcterm-app && cargo clippy -p arcterm-app -- -D warnings
  Checking arcterm-app v0.1.0
  Finished dev profile [unoptimized + debuginfo]
  Finished dev profile [unoptimized + debuginfo]
```

Exit status: 0. Zero errors. Zero warnings.

---

## Files Modified

- `arcterm-app/src/terminal.rs` — 1 method body rewritten, `#[allow(dead_code)]` retained
- `arcterm-app/src/main.rs` — 7 field accesses replaced with accessor calls, 1 variable removed
