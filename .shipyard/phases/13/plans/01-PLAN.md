---
phase: renderer-optimization
plan: "01"
wave: 1
dependencies: []
must_haves:
  - Per-pane dirty-row cache in prepare_grid_at (REVIEW-3.1-C)
  - Buffer pool truncation fix (REVIEW-3.1-B)
  - Frame pacing via PresentMode::Fifo
files_touched:
  - arcterm-render/src/text.rs
  - arcterm-render/src/renderer.rs
  - arcterm-render/src/gpu.rs
  - arcterm-app/src/main.rs
tdd: false
---

# PLAN-01 — Dirty-Row Cache, Buffer Pool Fix, and Frame Pacing

## Goal

Eliminate the three known renderer performance defects: (1) `prepare_grid_at` re-shapes every row every frame, (2) `reset_frame` destroys the buffer pool every frame, and (3) `PresentMode::Mailbox` causes uncapped frame rate and CPU spinning.

After this plan, multi-pane rendering skips unchanged rows, buffer allocations stabilize after the first frame, and the GPU presents at display refresh rate.

## Tasks

<task id="1" files="arcterm-render/src/text.rs:129-135" tdd="false">
  <action>In `reset_frame` (text.rs line 134), remove the line `self.pane_buffer_pool.truncate(0);`. Keep only `self.pane_slots.clear();` at line 132. This stops destroying all Buffer allocations every frame, allowing the pool to stabilize at the max pane count. The existing growth guard in `prepare_grid_at` (line 244: `if slot_idx >= self.pane_buffer_pool.len()`) already handles expansion correctly.</action>
  <verify>cd /Users/lgbarn/Personal/arcterm && cargo check -p arcterm-render 2>&1 | tail -5</verify>
  <done>`cargo check` succeeds. `reset_frame` body contains only `self.pane_slots.clear();` with no `truncate` call.</done>
</task>

<task id="2" files="arcterm-render/src/text.rs:229-280, arcterm-render/src/renderer.rs:60-76,109-120,152-196, arcterm-app/src/main.rs:1654-1672" tdd="false">
  <action>
Add per-pane dirty-row hashing to the multi-pane production path:

**A. `renderer.rs` — add field and threading:**
1. Add `pane_row_hashes: HashMap<usize, Vec<u64>>` field to the `Renderer` struct (after `image_placements`).
2. Initialize it as `HashMap::new()` in `Renderer::new`.
3. In `render_multipane`, before the pane loop, compute slot indices. Inside the loop, for each pane at slot index `i`, call `prepare_grid_at` with an additional `row_hashes` argument: `&mut self.pane_row_hashes.entry(i).or_default()`. This requires splitting the borrow — extract `pane_row_hashes` into a local `let hashes = &mut self.pane_row_hashes;` before the loop, then pass `hashes.entry(i).or_default()` while calling `self.text.prepare_grid_at(...)`. Because `self.text` and `self.pane_row_hashes` are separate fields, this compiles with a field-level split borrow (access `self.text` and `self.pane_row_hashes` as separate borrows, not through `&mut self`).
4. In `Renderer::resize`, add `self.pane_row_hashes.clear();` alongside the existing `self.text.row_hashes.clear();`.
5. In `Renderer::set_palette`, add `self.pane_row_hashes.clear();` alongside the existing `self.text.row_hashes.clear();`.

**B. `text.rs` — extend `prepare_grid_at` signature and add skip logic:**
1. Add `row_hashes: &mut Vec<u64>` as the last parameter of `prepare_grid_at`.
2. Before the row loop (after `buf_vec.truncate(num_rows)` at line 257), add `row_hashes.resize(num_rows, u64::MAX);`.
3. Inside the row loop (lines 260-270), before `buf.set_size`, add:
   ```
   let cursor_col = if row_idx == snapshot.cursor_row { Some(snapshot.cursor_col) } else { None };
   let row_hash = hash_row(row, row_idx, cursor_col);
   if row_idx != snapshot.cursor_row && row_hashes.get(row_idx) == Some(&row_hash) {
       continue;
   }
   row_hashes[row_idx] = row_hash;
   ```
4. Remove the existing `cursor_col` computation at line 268 (now moved above the skip check).

**C. `main.rs` — evict hash entries on pane close:**
In the `closed_panes` cleanup block (line 1654-1672), there is no direct access to renderer slot indices (slots are positional per frame). The simplest correct approach: no change needed here. The `pane_row_hashes` map is keyed by slot index (0, 1, 2...) which is reassigned each frame based on pane order. Stale entries for slot indices beyond the current pane count are harmless (never accessed). On resize, `Renderer::resize` already clears the entire map.
  </action>
  <verify>cd /Users/lgbarn/Personal/arcterm && cargo check -p arcterm-render -p arcterm-app 2>&1 | tail -5</verify>
  <done>`cargo check` succeeds for both crates. `prepare_grid_at` signature includes `row_hashes: &mut Vec<u64>`. The row loop in `prepare_grid_at` contains a `hash_row` call and a `continue` branch that skips `shape_row_into_buffer` for unchanged non-cursor rows. `Renderer` struct has a `pane_row_hashes: HashMap<usize, Vec<u64>>` field. `resize` and `set_palette` both clear `pane_row_hashes`.</done>
</task>

<task id="3" files="arcterm-render/src/gpu.rs:56-61" tdd="false">
  <action>In `GpuState::new_async` (gpu.rs lines 56-61), replace the Mailbox-preferring present mode selection with a direct assignment to Fifo:
  ```
  let present_mode = wgpu::PresentMode::Fifo;
  ```
  Remove the `if caps.present_modes.contains(...)` conditional entirely. Update the comment from "Prefer Mailbox (non-blocking, low-latency) for 120+ FPS; fall back to Fifo." to "VSync: cap frame rate to display refresh rate, preventing tearing and idle GPU spinning."</action>
  <verify>cd /Users/lgbarn/Personal/arcterm && cargo check -p arcterm-render 2>&1 | tail -5</verify>
  <done>`cargo check` succeeds. `gpu.rs` contains `let present_mode = wgpu::PresentMode::Fifo;` with no Mailbox preference. The `caps.present_modes.contains` conditional is gone.</done>
</task>

## Verification — Full Build

After all three tasks:

```bash
cd /Users/lgbarn/Personal/arcterm && cargo build -p arcterm-app 2>&1 | tail -5
```

Success criteria: clean build with no warnings in `arcterm-render` or `arcterm-app`.

## Manual Smoke Tests

1. **Dirty-row skip verification:** Run `RUST_LOG=debug cargo run -p arcterm-app`. Open two panes (`Ctrl+Shift+D`). In one pane, type a command and observe output. The other pane should not cause frame-level reshaping. Add a temporary `log::trace!("skip row {row_idx}")` inside the `continue` branch to confirm rows are being skipped.

2. **Buffer pool stability:** After the first frame, `pane_buffer_pool.len()` should remain constant. Verify via a temporary log in `prepare_grid_at` that no new `Buffer::new` calls occur after initial setup.

3. **Frame pacing:** After switching to Fifo, confirm via `log::debug!("wgpu present mode: {:?}", present_mode)` output that Fifo is selected. Verify no tearing and no CPU spinning when idle (Activity Monitor should show near-zero CPU when terminal is idle).

4. **`cat /dev/urandom | head -c 10M` isolation:** Run the command in one pane while interacting with the other. The adjacent pane should show no visible lag.
