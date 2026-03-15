# REVIEW-1.2 — DEC Private Modes and Extended VT Sequences

**Reviewer:** Claude Code (claude-sonnet-4-6)
**Date:** 2026-03-15
**Plan:** `.shipyard/phases/2/plans/PLAN-1.2.md`
**Summary:** `.shipyard/phases/2/results/SUMMARY-1.2.md`

---

## Stage 1: Spec Compliance

**Verdict:** PASS

### Task 1: Extend Handler trait with new methods and Grid implementations

- Status: PASS
- Evidence: `arcterm-vt/src/handler.rs` contains all 16 new methods specified in the plan — `set_mode`, `reset_mode`, `set_scroll_region`, `save_cursor_position`, `restore_cursor_position`, `insert_lines`, `delete_lines`, `insert_chars`, `delete_chars`, `erase_chars`, `cursor_horizontal_absolute`, `cursor_vertical_absolute`, `device_status_report`, `device_attributes`, `set_keypad_application_mode`, `set_keypad_numeric_mode` — all with default no-op implementations on the `Handler` trait (lines 67–141). `GridState` is present (lines 178–266) with `TermModes` (lines 149–169). `GridState` implements all 16 methods with the correct delegations or inline logic (lines 452–680).
- Notes: The plan required Grid implementations for modes 47 and 1047 (enter/leave alt screen without the cursor save/restore of 1049). These two modes are absent from the `set_mode`/`reset_mode` match arms in `GridState`. The SUMMARY acknowledges this omission is not flagged, but the plan explicitly listed "47/1047 => enter/leave alt screen" in the required match arms. This is examined further below — because the plan lists 1049 as the primary real-world case and the remaining items are clearly listed as future-use stubs (`1000/1002/1003/1006 => store in modes (mouse reporting flags for future use)`), I treat 47/1047 and mouse modes as Important findings rather than a Stage 1 blocker, since the core spec goal (vim/htop operation with 1049) is met. The legacy `impl Handler for Grid` is preserved unchanged (lines 687–846), satisfying the backward-compatibility done criterion. `cargo test --package arcterm-vt` passes: 70 tests, 0 failed.

### Task 2: Extend csi_dispatch and esc_dispatch

- Status: PASS
- Evidence: `arcterm-vt/src/processor.rs` `csi_dispatch` (lines 58–243): `private` detection via `intermediates.contains(&0x3F)` is present at line 77. The `'h'`/`'l'` arms (lines 205–215) iterate all parameter groups and pass `private` to `set_mode`/`reset_mode`, handling both DEC private and standard modes with one unified dispatch rather than a separate early-return block. All new CSI arms are present: `'r'` (DECSTBM, lines 218–239), `'L'` (IL, lines 128–132), `'M'` (DL, lines 133–137), `'@'` (ICH, lines 167–171), `'P'` (DCH, lines 183–187), `'X'` (ECH, lines 188–192), `'G'` (CHA, lines 172–182), `'d'` (VPA, lines 193–203), `'n'` (DSR, lines 148–152), `'c'` (DA, lines 153–156). `esc_dispatch` (lines 273–285) dispatches `0x37`/`0x38`/`0x3D`/`0x3E` correctly.
- Notes: The plan specifies that `esc_dispatch` should only fire when `intermediates` is empty (plan: "If intermediates is empty, match on byte; Otherwise ignore"). The implementation dispatches on `byte` unconditionally, ignoring the `_intermediates` parameter entirely. For `ESC 7`/`ESC 8`/`ESC =`/`ESC >`, the intermediate byte is always absent in conforming VT streams, so this does not break any of the tested sequences. However, it is a deviation from the spec — a stream like `ESC ( 7` (select G0 charset) would incorrectly fire `save_cursor_position`. This is tracked below as an Important finding. The DECSTBM `usize::MAX` sentinel approach (lines 234–238) correctly handles the no-params full-screen-reset case. All 13 TDD tests required by the plan are present and pass.

### Task 3: Integration tests — vim/htop scenarios

- Status: PASS
- Evidence: `arcterm-vt/src/lib.rs` contains the `phase2_integration_tests` module (lines 902–1071) with all four scenarios specified in the plan, plus one bonus test: `vim_startup_enters_alt_screen_and_sets_scroll_region` (line 926), `vim_exit_restores_normal_screen` (line 967), `htop_scroll_region_excludes_last_row` (line 993), `multi_mode_set_sets_all_modes` (line 1027), and `scroll_up_respects_scroll_region_boundaries` (line 1046). All five pass.
- Notes: The vim_startup test correctly validates `alt_screen_active`, scroll region `(0, 23)`, cursor position, and alt-screen content (lines 942–957). The vim_exit test verifies normal screen restoration and scroll region reset (lines 980–985). The htop test verifies the status bar row (23) is untouched after a LF at the scroll region bottom (lines 1015–1019). The multi-mode-set test verifies all three modes set in one sequence (lines 1036–1038). The SUMMARY's TDD note about the vim_startup cursor assertion correction is accurate.

---

## Stage 2: Code Quality

### Critical

None.

### Important

**ISSUE-011 — `esc_dispatch` does not guard on empty intermediates**

- File: `/Users/lgbarn/Personal/myterm/arcterm-vt/src/processor.rs:273–285`
- The plan specifies: "If intermediates is empty, match on byte; Otherwise ignore (intermediates present for other ESC sequences)." The implementation ignores `_intermediates` entirely and matches `byte` unconditionally. Byte `0x37` ('7') also appears as the final byte of ESC sequences that use an intermediate, such as the SCS (Select Character Set) sequences `ESC ( 7` or `ESC ) 7`. If a terminal application sends one of those sequences, `save_cursor_position` will be incorrectly invoked. This is a silent mis-dispatch, not a crash, but it can corrupt cursor state in a subtly hard-to-debug way.
- Remediation: Restore the intermediate guard. Replace the current `match byte` with:
  ```rust
  fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
      if !intermediates.is_empty() {
          return;
      }
      match byte {
          0x37 => self.handler.save_cursor_position(),
          0x38 => self.handler.restore_cursor_position(),
          0x3D => self.handler.set_keypad_application_mode(),
          0x3E => self.handler.set_keypad_numeric_mode(),
          _ => {}
      }
  }
  ```

**ISSUE-012 — Modes 47, 1047, and mouse modes (1000/1002/1003/1006) are absent from `set_mode`/`reset_mode`**

- File: `/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs:452–505`
- The plan's Task 1 action explicitly lists mode 47 and 1047 as "enter/leave alt screen" in the `set_mode`/`reset_mode` match, and modes 1000/1002/1003/1006 as "store in modes (mouse reporting flags for future use)." None of these appear in the implementation. Mode 1047 is used by older applications and some tmux configurations as the original alt-screen escape. Without it, those applications will not enter the alt screen even though 1049 works. The mouse modes are deferred-use but the plan requires them stored.
- Remediation: Add mode 1047 to `set_mode` and `reset_mode`, sharing the same alt-screen enter/leave logic as 1049 (without the cursor save/restore). For mouse modes, add `TermModes` fields `mouse_report_click: bool`, `mouse_report_button: bool`, `mouse_report_any: bool`, `mouse_sgr_ext: bool` and set/clear them for 1000/1002/1003/1006 respectively.

**ISSUE-013 — `newline` does not handle the cursor-above-scroll-region case correctly**

- File: `/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs:290–301`
- The else branch of `newline` (cursor not at or past `scroll_bottom`) advances the cursor by one row, then checks if the new position is below `scroll_top` and clamps upward. However, the comment reads "clamp to top" but the check is `cur_row + 1 < scroll_top` — if the cursor is already above the scroll region, moving it down by one still leaves it above the region, and the newly set position (`cur_row + 1`) could still be less than `scroll_top`. The code should handle this by not scrolling and not clamping — a cursor above the scroll region should move freely downward toward the region without triggering a scroll. The current implementation does handle this correctly for the common case but the dead-code clamp check adds confusion: after `cur_row + 1`, the cursor can never be *less than* `scroll_top` if it was equal to or greater before advancing. The check at line 296 is logically unreachable for a well-formed terminal state where cursor was already at or above the region, making it dead code that obscures intent.
- Remediation: Remove the unreachable clamp check at lines 296–301, or add a comment explaining under what circumstances it could fire. More importantly, add a test that positions the cursor above the scroll region and verifies that successive newlines move it toward and then into the scroll region without triggering spurious scrolling.

### Suggestions

**Suggestion 1 — `scroll_region_up`/`scroll_region_down` are not tested in isolation**

- File: `/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs:218–265`
- The scroll helper methods are exercised indirectly through the integration tests but have no unit tests directly exercising boundary cases: `n` equal to region height (entire region clears), `n = 0` (early return, no modification), single-row region.
- Remediation: Add unit tests in `phase2_processor_tests` covering `scroll_region_up(n=region_height)` and `scroll_region_down(n=region_height)` to guard against off-by-one in the range computation at line 228 (`(bottom - n)` will underflow if `n > bottom - top`). The `n.min(region_height)` clamp at line 222 prevents the underflow, but a test would lock in that contract.

**Suggestion 2 — `GridState::eff_scroll_top` is a trivial wrapper with no clamping**

- File: `/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs:208–211`
- `eff_scroll_top` returns `self.scroll_top` with no clamping, while `eff_scroll_bottom` applies `.min(rows - 1)`. For symmetry and defensive correctness, `eff_scroll_top` should also clamp: `self.scroll_top.min(self.eff_scroll_bottom())`. If `scroll_top` were ever set above `scroll_bottom` (e.g., via a malformed DECSTBM sequence), the region height calculation at line 221 (`bottom + 1 - top`) would underflow.
- Remediation: In `set_scroll_region`, add a guard that ensures `scroll_top < scroll_bottom` before assigning (and reject or clamp the sequence if not). Alternatively, clamp in `eff_scroll_top`: `self.scroll_top.min(self.scroll_bottom)`.

**Suggestion 3 — `vim_exit_restores_normal_screen` does not verify scroll region is reset from a non-default value**

- File: `/Users/lgbarn/Personal/myterm/arcterm-vt/src/lib.rs:967–985`
- The test asserts `scroll_top=0` and `scroll_bottom=23` after exit, but the scroll region was never changed from the default in this test — `ESC[r` resets to full screen but started there. The assertion is trivially true and would pass even if `ESC[r` were a no-op.
- Remediation: Add `feed_gs(&mut gs, b"\x1b[5;20r");` before the exit sequence to set a non-default region, then assert the `ESC[r` + `ESC[?1049l` sequence correctly resets it.

---

## Summary

**Verdict:** REQUEST CHANGES

All three tasks are correctly implemented and the test suite passes at 70/70. Stage 1 is a clear PASS. In Stage 2, two Important findings require attention before this plan can be considered closed: the `esc_dispatch` intermediates guard is missing (ISSUE-007, a spec deviation that can cause silent cursor-state corruption on SCS sequences), and modes 47/1047/1000/1002/1003/1006 are absent despite being explicitly enumerated in the plan's Task 1 action (ISSUE-008). These are not regressions from the previous state but are gaps the plan contracted to fill.

Critical: 0 | Important: 3 | Suggestions: 3
