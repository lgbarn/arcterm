---
plan: "01"
phase: renderer-optimization
reviewer: claude-sonnet-4-6
date: 2026-03-16
verdict: APPROVE
---

## Stage 1: Spec Compliance

**Verdict:** PASS

### Task 1: Buffer Pool Truncation Fix

- Status: PASS
- Evidence: `arcterm-render/src/text.rs:129-133` — `reset_frame` body is exactly two lines: the doc comment and `self.pane_slots.clear();`. The `self.pane_buffer_pool.truncate(0)` line is absent.
- Notes: The growth guard at `text.rs:243` (`if slot_idx >= self.pane_buffer_pool.len()`) remains correct and handles pool expansion for new panes. The pool will stabilize at max concurrent pane count after the first frame, as intended.

### Task 2: Per-Pane Dirty-Row Hash Cache

- Status: PASS
- Evidence:
  - **`text.rs:235`** — `prepare_grid_at` signature includes `row_hashes: &mut Vec<u64>` as the last parameter.
  - **`text.rs:259`** — `row_hashes.resize(num_rows, u64::MAX)` appears before the row loop.
  - **`text.rs:264`** — `cursor_col` is computed above the skip check: `let cursor_col = if row_idx == snapshot.cursor_row { Some(snapshot.cursor_col) } else { None };`
  - **`text.rs:267-271`** — `hash_row` is called, the `continue` branch skips `set_size` + `shape_row_into_buffer` for unchanged non-cursor rows, and `row_hashes[row_idx]` is updated on entry.
  - **`renderer.rs:79`** — `pub pane_row_hashes: HashMap<usize, Vec<u64>>` field present after `image_placements`.
  - **`renderer.rs:107`** — `pane_row_hashes: HashMap::new()` in `Renderer::new`.
  - **`renderer.rs:119-120`** — both `self.text.row_hashes.clear()` and `self.pane_row_hashes.clear()` in `set_palette`.
  - **`renderer.rs:127-128`** — both clears present in `resize`.
  - **`renderer.rs:181-209`** — `let hashes = &mut self.pane_row_hashes` extracted before the pane loop; `panes.iter().enumerate()` used; `hashes.entry(slot_idx).or_default()` passed to `prepare_grid_at`. Field-level split borrow compiles: `self.text` and `self.pane_row_hashes` accessed as separate fields.
  - **Task 2C (main.rs):** No change was needed per the plan. Confirmed at `main.rs:1655-1672` — `pane_row_hashes` is not keyed by `PaneId` and stale slot-index entries are harmless.
- Notes: The skip condition at `text.rs:268` uses `row_hashes.get(row_idx) == Some(&row_hash)` rather than a direct index, which is correct and avoids a panic if the vec is shorter than `row_idx` (though `resize` at line 259 makes this impossible in practice). All 41 tests pass including the dirty-row hash suite.

### Task 3: PresentMode::Fifo

- Status: PASS
- Evidence: `arcterm-render/src/gpu.rs:57` — `let present_mode = wgpu::PresentMode::Fifo;`. The Mailbox-preferring `if caps.present_modes.contains(...)` conditional is gone. Comment at line 56 reads "VSync: cap frame rate to display refresh rate, preventing tearing and idle GPU spinning." The `log::debug!` call at line 58 confirms the mode is logged at startup.
- Notes: The `caps` variable is still used at `gpu.rs:54` (for `surface_format`), so its presence is not dead code despite the Mailbox check being removed.

---

## Stage 2: Code Quality

### Critical

None.

### Important

- **`submit_text_areas` zip truncates to shorter of pool vs slots — pre-existing but now more observable** at `arcterm-render/src/text.rs:308`
  - `self.pane_buffer_pool.iter().zip(self.pane_slots.iter())` stops at the shorter of the two slices. Before this plan, `reset_frame` called `truncate(0)` on the pool, so both were always in sync (pool rebuilt every frame). Now the pool is longer than the slots between frames (pool holds N from last high-water mark; slots holds current-frame count). `zip` stops at `pane_slots.len()` which is always correct — extra pool entries are safely ignored. This is correct behavior, but the code comment does not explain why the lengths can differ safely. A new reader will wonder if the mismatch is a bug.
  - Remediation: Add a comment above the zip: `// pane_buffer_pool may be longer than pane_slots (pool grows monotonically; slots reset each frame). zip stops at pane_slots.len(), which is correct.`

- **`pane_row_hashes` is `pub` but should be `pub(crate)` or private** at `arcterm-render/src/renderer.rs:79`
  - The field is `pub pane_row_hashes`. The struct doc comment does not indicate this is part of the public API. Direct external mutation bypasses the invariant that the map is cleared on resize/palette change, which is the sole correctness guarantee of the dirty-row system.
  - Remediation: Change `pub pane_row_hashes` to `pub(crate) pane_row_hashes`. If downstream crates in the workspace need access, expose a `clear_row_hashes()` method instead of the raw field.

### Suggestions

- **No test for the `prepare_grid_at` dirty-row skip in the multi-pane path** at `arcterm-render/src/text.rs:262-279`
  - All 9 hash-related tests in `text::tests` test `hash_row` and `substitute_cursor_char` directly. None exercise `prepare_grid_at`'s skip branch. A test that calls `prepare_grid_at` twice with an identical snapshot and a mutable `row_hashes` vec, then asserts the second call does not call `shape_row_into_buffer` for unchanged rows, would guard the skip logic against regression. This can be approximated by asserting that the row `Buffer`'s internal content (e.g., glyph run count) remains unchanged after the second call.
  - Remediation: Add `fn prepare_grid_at_skips_unchanged_rows` in a `#[cfg(test)]` block: construct a minimal `RenderSnapshot`, call `prepare_grid_at` twice, and verify `row_hashes` is not all `u64::MAX` after the first call and is unchanged after the second.

- **`hash_row` uses `DefaultHasher` which is not deterministic across Rust versions** at `arcterm-render/src/text.rs:774`
  - `std::collections::hash_map::DefaultHasher` is documented as having no stability guarantee across Rust releases. Hash values are not persisted between processes so this is not a correctness risk today, but a future toolchain upgrade could silently reset all dirty-row caches on the first frame after the upgrade (all hashes mismatch, full reshape). For an in-process ephemeral cache this is acceptable but worth noting.
  - Remediation: If stability becomes a concern, replace with a fixed-seed `FxHasher` (from `rustc-hash`) or a simple FNV-1a implementation. No change needed today; add a comment: `// DefaultHasher is not stable across Rust releases; this is acceptable because hashes are ephemeral (in-process, no persistence).`

---

## Summary

**Verdict:** APPROVE

All three tasks from PLAN-01 are correctly implemented and match their done criteria exactly. `reset_frame` no longer truncates the pool, `prepare_grid_at` skips unchanged non-cursor rows via `hash_row`, `pane_row_hashes` is threaded correctly through the split-borrow pattern in `render_multipane`, and `gpu.rs` unconditionally uses `PresentMode::Fifo`. The full build is clean and all 41 tests pass.

Critical: 0 | Important: 2 | Suggestions: 3

Two Important findings (zip comment gap, overly-public field) are appended to `.shipyard/ISSUES.md` below.
