---
plan: "01"
phase: renderer-optimization
status: complete
date: 2026-03-16
commits:
  - 152e7a5
  - a9b0d3d
  - 05d8926
---

# SUMMARY-01 — Dirty-Row Cache, Buffer Pool Fix, and Frame Pacing

## What Was Done

All three tasks from PLAN-01 were executed sequentially, each verified and
committed atomically.

---

### Task 1 — Buffer Pool Truncation Fix

**File:** `arcterm-render/src/text.rs`

Removed `self.pane_buffer_pool.truncate(0);` from `reset_frame`. The method
body now contains only `self.pane_slots.clear();`. The pool grows to the max
pane count on the first frame and is reused every subsequent frame with no
new allocations. The growth guard at `prepare_grid_at` line 244 was already
correct and handles pool expansion for new panes.

**Verify result:** `cargo check -p arcterm-render` passed.

---

### Task 2 — Per-Pane Dirty-Row Hash Cache

**Files:** `arcterm-render/src/text.rs`, `arcterm-render/src/renderer.rs`

**`text.rs` changes:**
- Added `row_hashes: &mut Vec<u64>` as the last parameter of `prepare_grid_at`.
- Before the row loop, added `row_hashes.resize(num_rows, u64::MAX)` to ensure
  the hash vec is always as long as the row count (u64::MAX forces re-shape on
  first frame).
- Inside the row loop, moved `cursor_col` computation above the skip check.
- Added `hash_row` call and a `continue` branch that skips `set_size` and
  `shape_row_into_buffer` for unchanged non-cursor rows.
- Removed the duplicate `cursor_col` computation that was at the bottom of the
  loop (now computed once above the skip check).

**`renderer.rs` changes:**
- Added `pub pane_row_hashes: HashMap<usize, Vec<u64>>` field to `Renderer`
  (after `image_placements`).
- Initialized `pane_row_hashes: HashMap::new()` in `Renderer::new`.
- Added `self.pane_row_hashes.clear()` in both `set_palette` and `resize`.
- In `render_multipane`, extracted `let hashes = &mut self.pane_row_hashes`
  before the pane loop (field-level split borrow, allowing `self.text` to be
  borrowed independently). Changed `for pane in panes` to
  `for (slot_idx, pane) in panes.iter().enumerate()` and passed
  `hashes.entry(slot_idx).or_default()` to `prepare_grid_at`.

**Borrow checker note:** Rust's field-level split borrow compiled without
issues because `self.pane_row_hashes` and `self.text` are distinct struct
fields. No workarounds or `unsafe` were needed.

**`main.rs` change:** Per the plan, no change was needed. The hash map is
keyed by slot index (0-based pane order per frame); stale entries for slot
indices beyond the current pane count are never accessed. `Renderer::resize`
clears the entire map whenever the layout changes.

**Verify result:** `cargo check -p arcterm-render -p arcterm-app` passed.

---

### Task 3 — PresentMode::Fifo (VSync)

**File:** `arcterm-render/src/gpu.rs`

Replaced the Mailbox-preferring conditional:
```rust
let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
    wgpu::PresentMode::Mailbox
} else {
    wgpu::PresentMode::Fifo
};
```

With a direct assignment:
```rust
let present_mode = wgpu::PresentMode::Fifo;
```

Updated the comment from "Prefer Mailbox (non-blocking, low-latency) for
120+ FPS; fall back to Fifo." to "VSync: cap frame rate to display refresh
rate, preventing tearing and idle GPU spinning."

**Verify result:** `cargo check -p arcterm-render` passed.

---

## Full Build Verification

```
cargo build -p arcterm-app
Finished `dev` profile [unoptimized + debuginfo] target(s) in 53.03s
```

No warnings in `arcterm-render` or `arcterm-app`.

```
cargo test -p arcterm-render
test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured
```

All 41 unit tests passed, including all dirty-row hash tests in
`text::tests` and structured rendering tests.

---

## Deviations from Plan

None. All tasks were implemented exactly as specified. The field-level split
borrow in `render_multipane` compiled without any need for temporary variable
gymnastics beyond what the plan described.

---

## Final State

| Defect | Status |
|--------|--------|
| Buffer pool destroyed every frame (`truncate(0)`) | Fixed |
| `prepare_grid_at` re-shapes every row every frame | Fixed (dirty-row skip) |
| `PresentMode::Mailbox` — uncapped frame rate | Fixed (Fifo) |
