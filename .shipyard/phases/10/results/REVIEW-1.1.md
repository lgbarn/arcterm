# REVIEW-1.1 — scroll_offset API Migration

**Plan:** PLAN-1.1
**Phase:** 10, Wave 1
**Reviewed:** 2026-03-16
**Reviewer:** Review Agent

---

## Stage 1: Spec Compliance

**Verdict:** PASS

### Task 1: terminal.rs — delegate Terminal::set_scroll_offset to Grid accessor

- Status: PASS
- Evidence: `arcterm-app/src/terminal.rs:195-199` — the method body now reads `self.grid_state.grid.set_scroll_offset(offset);` (single line). The manual `scrollback.len()` read and direct `scroll_offset` field assignment are gone. The doc comment was updated to reflect delegation.
- Notes: The `#[allow(dead_code)]` attribute was retained with a `// Used in Wave 3 integration` comment, contrary to the plan's instruction to remove it. This is addressed as a deviation below.

### Task 2: main.rs — replace 7 direct grid.scroll_offset field accesses with accessor calls

- Status: PASS
- Evidence (from `git diff pre-build-phase-10..HEAD -- arcterm-app/`):
  1. PTY got_data read (~1692): `grid.scroll_offset > 0` → `grid.scroll_offset() > 0` — confirmed.
  2. PTY got_data write (~1694): `grid.scroll_offset = 0` → `grid.set_scroll_offset(0)` — confirmed.
  3. MouseWheel read (~1981): `grid.scroll_offset as i32` → `grid.scroll_offset() as i32` — confirmed.
  4. MouseWheel write (~1984): `grid.scroll_offset = new_offset` → `grid.set_scroll_offset(new_offset)` — confirmed. The `max_offset` variable and `.clamp(0, max_offset as i32)` were removed; `.max(0)` remains. The setter applies the upper-bound clamp internally — logic is equivalent.
  5. Search overlay read (~2339): `grid.scroll_offset` → `grid.scroll_offset()` — confirmed.
  6. Search NextMatch write (~2648): assignment converted to `set_scroll_offset(...)` call — confirmed.
  7. Search PrevMatch write (~2668): same pattern — confirmed.
- Notes: All seven replacements match the plan's specified substitutions. The `review.scroll_offset` accesses on `overlay::OverlayReviewState` (lines 2595, 2605) were correctly left untouched per the plan's explicit exclusion. Grep for `\.scroll_offset\b[^(]` confirms no remaining direct field accesses on `Grid` inside `arcterm-app/src/`.

### Task 3: clippy verification — cargo clippy -p arcterm-app -- -D warnings exits 0

- Status: PASS
- Evidence: `cargo clippy -p arcterm-app -- -D warnings` exits status 0 with zero errors and zero warnings (verified by running the command directly).
- Notes: The deviation on Task 1 (`#[allow(dead_code)]` retained) was necessary to satisfy this task. The builder correctly identified the conflict and resolved it using the codebase convention from CONVENTIONS.md — Temporary Suppression Conventions: `#[allow(dead_code)] // Used in Wave N integration`.

---

### Deviation Assessment: #[allow(dead_code)] retained on Terminal::set_scroll_offset

The plan instructed removal of `#[allow(dead_code)]` with the rationale that the method is used by callers. After the migration, that rationale turned out to be incorrect — `main.rs` calls `grid_mut().set_scroll_offset()` directly rather than `terminal.set_scroll_offset()`. The method is genuinely dead today.

The builder's resolution — retain the attribute with a Wave 3 annotation — is correct. Removing the method would be an out-of-scope architectural change; deleting the attribute without a suppression would fail clippy (`-D warnings` is a must-have). The CONVENTIONS.md pattern is followed exactly. This deviation is acceptable.

---

## Stage 2: Code Quality

### Critical

None.

### Important

None.

### Suggestions

- **`Terminal::set_scroll_offset` wrapper has no current caller and its future caller is unspecified** — `arcterm-app/src/terminal.rs:196-199`
  - The `// Used in Wave 3 integration` comment is informal. If Wave 3 is delayed or the call site ends up calling `grid_mut().set_scroll_offset()` directly (as the current code does), the dead wrapper will persist indefinitely.
  - Remediation: At the start of Wave 3, verify whether callers actually use `Terminal::set_scroll_offset` and remove the method (or the suppress attribute) at that time. No action needed now.

---

## Summary

**Verdict:** APPROVE

All 8 compile errors are resolved exactly as specified. The three commits are clean, each scoped to a single logical change. The single deviation (`#[allow(dead_code)]` retained) is properly reasoned and follows the established codebase convention. `cargo check -p arcterm-app` and `cargo clippy -p arcterm-app -- -D warnings` both exit 0. The crate is ready for Wave 2.

Critical: 0 | Important: 0 | Suggestions: 1
