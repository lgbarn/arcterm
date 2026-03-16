# Research: Phase 13 — Renderer Optimization

## Context

Arcterm is a native Rust terminal emulator using wgpu for GPU rendering and glyphon
(cosmic-text) for text shaping. The codebase is on branch `phase-12-engine-swap`
located at `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap`.

Phase 13 targets three performance improvements:

1. Per-pane dirty-row cache for `TextRenderer` — eliminate redundant text
   re-shaping on frames where pane content has not changed.
2. Frame pacing via `PresentMode::Fifo` — deterministic VSync; stop burning CPU
   in `ControlFlow::Poll` when the terminal is idle.
3. Latency measurement behind the `latency-trace` feature flag — add timestamps
   at PTY-byte-received and snapshot-locked points so end-to-end latency is
   observable.

A related performance defect (REVIEW-3.1-B, buffer pool truncation) is included as
background because it interacts with the dirty-row change.

---

## 1. Current Dirty-Row Mechanism

### How single-pane dirty-row skipping works (`prepare_grid`)

File: `arcterm-render/src/text.rs`, lines 141–217.

`TextRenderer` owns a `pub row_hashes: Vec<u64>` field (line 75). `prepare_grid`
iterates every row and calls `hash_row(row, row_idx, cursor_col_opt)` (line 171).
`hash_row` (lines 759–783) hashes each cell's codepoint, fg/bg color, bold/italic/
underline/inverse flags, the row index, and — when this is the cursor row — the
cursor column. If the computed hash matches `self.row_hashes[row_idx]` AND the row
is not the cursor row, the loop issues a `continue`, skipping `shape_row_into_buffer`
entirely (lines 173–176). The hash vec is length-synced to `num_rows` at line 162;
a resize enlarges or shrinks it. `row_hashes` is initialized to `u64::MAX` so every
row is considered dirty on the first frame.

### What `prepare_grid_at` does instead (the actual production path)

File: `arcterm-render/src/text.rs`, lines 229–280.

`prepare_grid_at` is the multi-pane path called by `Renderer::render_multipane`
(renderer.rs, line 189). It contains **no hash check**. Lines 260–270 iterate every
row unconditionally and call `shape_row_into_buffer` for each one. There is no
reference to `row_hashes`, no per-pane hash storage, and no skip logic. This is the
production hot path for all real arcterm sessions, which are always multi-pane (even
a single-pane layout goes through `render_multipane`).

### What REVIEW-3.1-C says

ISSUES.md, lines 312–318:

> `prepare_grid_at` re-shapes all rows every frame; dirty-row optimization absent
> from hot path ... For applications with largely static output, nearly all
> 80-col x 24-row cell iterations are wasted.
>
> Remediation: Add per-pane row hash vecs in `AppState` (one `Vec<u64>` per
> `PaneId`), pass them into `prepare_grid_at` as `row_hashes: &mut Vec<u64>`,
> and apply the same `hash_row` skip logic as `prepare_grid`. Clear the hash vec
> for a pane on resize or palette change.

### What needs to change

There are three required changes across two crates:

**A. `arcterm-render/src/text.rs` — extend `prepare_grid_at` signature and add skip
logic.**

Current signature (line 229):
```
pub fn prepare_grid_at(
    &mut self,
    snapshot: &RenderSnapshot,
    offset_x: f32,
    offset_y: f32,
    clip: Option<ClipRect>,
    scale_factor: f32,
    palette: &RenderPalette,
)
```

New signature adds `row_hashes: &mut Vec<u64>` as the last parameter. Inside the
row loop (lines 260–270) add the same pattern as `prepare_grid`:

```rust
let cursor_col = if row_idx == snapshot.cursor_row { Some(snapshot.cursor_col) } else { None };
let row_hash = hash_row(row, row_idx, cursor_col);
row_hashes.resize(num_rows, u64::MAX); // once before the loop, not per-iteration
if row_idx != snapshot.cursor_row && row_hashes.get(row_idx) == Some(&row_hash) {
    continue;
}
row_hashes[row_idx] = row_hash;
```

The `buf.set_size` and `shape_row_into_buffer` calls move inside the `else` branch
(or after the `continue`).

**B. `arcterm-render/src/renderer.rs` — thread per-pane hash vecs through
`render_multipane`.**

`render_multipane` calls `self.text.prepare_grid_at(...)` at line 189. It currently
has no access to per-pane hash state. Two design options:

- Option 1 (preferred, avoids crate boundary coupling): Add a
  `HashMap<usize, Vec<u64>>` field to `Renderer` itself, keyed by pane slot index
  (the position in the `panes` slice). Slot indices are stable within a session;
  when a pane is removed the corresponding entry can be dropped. Internally
  `render_multipane` passes `&mut self.pane_row_hashes.entry(slot_idx).or_default()`
  to each `prepare_grid_at` call.

- Option 2: Add `pane_row_hashes: HashMap<PaneId, Vec<u64>>` to `AppState` in
  `arcterm-app/src/main.rs` and pass them into `render_multipane` via `PaneRenderInfo`.
  This crosses a crate boundary (arcterm-render would need to know about `PaneId` from
  arcterm-app, which is the reverse dependency direction) — not viable as-is.

  Alternative: add an opaque `cache_key: u64` field to `PaneRenderInfo` and key the
  hash map inside `Renderer` on that, keeping arcterm-render self-contained.

Option 1 is cleaner and requires no API changes at the `main.rs` call site.

**C. `arcterm-app/src/main.rs` — invalidate hash vecs on resize and palette change.**

When a pane is resized (`WindowEvent::Resized` handler, line 1744) or when the
palette changes (config hot-reload block, line 1358), the relevant pane's hash vec
must be cleared. With Option 1 above the invalidation is: call a new
`renderer.invalidate_pane_hashes(slot_idx)` method, or more simply wipe the entire
`pane_row_hashes` map on resize (acceptable since resize is infrequent).

### REVIEW-3.1-B: buffer pool truncation (related)

ISSUES.md, lines 304–310:

`reset_frame` at line 134 calls `self.pane_buffer_pool.truncate(0)`, destroying all
`Vec<Buffer>` entries every frame. The comment on line 133 says "Truncate pool to
match" but this defeats the pool entirely, causing `~48 Buffer` allocations per frame
at 60fps for a 2-pane layout. The fix is to remove line 134, keeping only
`self.pane_slots.clear()` at line 132. The `prepare_grid_at` growth guard at line 244
already handles pool expansion; stability at max pane count is the desired behavior.

This fix must land before or alongside the dirty-row work because both touch
`reset_frame` and `prepare_grid_at`.

---

## 2. Current Frame Pacing

### Present mode selection

File: `arcterm-render/src/gpu.rs`, lines 56–61.

```rust
let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
    wgpu::PresentMode::Mailbox
} else {
    wgpu::PresentMode::Fifo
};
```

The current code **prefers `Mailbox`** (non-blocking, uncapped frame rate) and only
falls back to `Fifo` (VSync, frame pacing) when `Mailbox` is unavailable. On macOS
with Metal, `Mailbox` is typically available, so `Fifo` is never selected in practice
on the primary development platform.

`desired_maximum_frame_latency: 2` is set at line 72, which is appropriate for Fifo
but has no effect under Mailbox.

### ControlFlow behavior

File: `arcterm-app/src/main.rs`, `about_to_wait`, lines 1681–1700.

```rust
if got_data {
    state.window.request_redraw();
    state.idle_cycles = 0;
    event_loop.set_control_flow(ControlFlow::Poll);  // line 1692
} else {
    state.idle_cycles = state.idle_cycles.saturating_add(1);
    if state.idle_cycles >= 3 {
        event_loop.set_control_flow(ControlFlow::Wait); // line 1696
    } else {
        event_loop.set_control_flow(ControlFlow::Poll); // line 1698
    }
}
```

The event loop switches between `Poll` (spin, ~thousands of iterations/second) and
`Wait` (block until event) based on idle cycle count. When active, the loop spins at
full CPU. There is no frame-rate cap under `Mailbox`.

`request_redraw()` is called on nearly every non-trivial event throughout the file
(>40 call sites). These all translate to a `RedrawRequested` event which triggers a
full `render_multipane` call.

### Implication for Phase 13

Phase 13 scope says "frame pacing with `PresentMode::Fifo`". This means:

1. Change `GpuState::new_async` to prefer `Fifo` over `Mailbox`:
   ```rust
   let present_mode = wgpu::PresentMode::Fifo;
   ```
   or add a configuration option that defaults to `Fifo`.

2. Under `Fifo`, `get_current_texture()` blocks until the next VSync, providing
   natural frame pacing. This eliminates the CPU spin problem and makes the
   `ControlFlow::Poll` path far less expensive when there is PTY data.

3. The `idle_cycles` throttle (lines 1694–1699) can remain as a defense against
   redundant `request_redraw` calls when truly idle. `ControlFlow::Wait` is still
   correct for the zero-activity state.

No changes to `main.rs` ControlFlow logic are required to achieve basic VSync
pacing — the change is entirely in `gpu.rs`.

---

## 3. PTY Output Coalescing

### Current wakeup processing

File: `arcterm-app/src/main.rs`, `about_to_wait`, lines 1432–1652.

File: `arcterm-app/src/terminal.rs`, lines 541–569.

`has_wakeup()` drains the `wakeup_rx` channel completely in a `while try_recv()` loop
(lines 545–547). This correctly coalesces all pending wakeup signals into a single
boolean. Multiple `Wakeup` events sent by the alacritty reader thread between two
`about_to_wait` calls all resolve to a single `had_wakeup = true`. The drain also
processes APC (Kitty), OSC 7770, and OSC 133 payloads in the same call.

### Is there batching?

Yes, at the wakeup signal level. All `Wakeup` events that arrive in the interval
between two `about_to_wait` calls are coalesced into a single `got_data = true`
flag. The alacritty `Term` internal state accumulates all byte processing performed
between calls, so a single `lock_term()` and `snapshot_from_term()` captures the
full accumulated state regardless of how many `Wakeup` events fired.

### Remaining coalescing opportunity

The current flow processes each pane independently in sequence. For each pane with a
wakeup, the OSC block drain, auto-detection scan, and `snapshot_from_term` lock all
execute in the same `about_to_wait` call. A single `render_multipane` then renders
all panes. This is already one-render-per-frame regardless of how many panes woke up,
which is the correct batching model.

The main latency opportunity is not further coalescing but rather that `ControlFlow::Poll`
under `Mailbox` causes `about_to_wait` to fire multiple times per display refresh,
doing redundant wakeup checks. Switching to `Fifo` + `ControlFlow::Wait` when idle
directly addresses this by making the loop fire at most once per display refresh.

### Where coalescing logic lives

`about_to_wait` (line 1261) is the single correct location for all PTY drain and
coalesce logic. No additional coalescing abstraction is needed.

---

## 4. Latency Trace Feature

### Feature flag

File: `arcterm-app/Cargo.toml`, lines 18–20:

```toml
[features]
## Enables fine-grained timestamp logging for latency measurement.
latency-trace = []
```

The feature exists and is empty (compile-time conditional only).

### Existing instrumentation points

All guarded by `#[cfg(feature = "latency-trace")]`:

| Location | Line | What it measures |
|----------|------|-----------------|
| `main.rs` | 514–515, 529–530 | Cold start timestamp (`cold_start: TraceInstant`) captured before event loop |
| `main.rs` | 1446–1447 | `t0 = TraceInstant::now()` at start of per-pane wakeup processing in `about_to_wait` |
| `main.rs` | 1649–1650 | Log "PTY wakeup processed in {:?}" — measures wakeup drain time |
| `main.rs` | 1988–1989 | `t0 = TraceInstant::now()` at start of `RedrawRequested` handler |
| `main.rs` | 2358–2375 | Log "frame submitted in {:?}" + "key → frame presented: {:?}" + "cold start → first frame: {:?}" |
| `main.rs` | 2383–2384 | Spare `t0` at start of keyboard input handler (not logged separately) |
| `main.rs` | 2387–2389 | `key_press_t0 = Some(TraceInstant::now())` — captured for key→frame latency |
| `main.rs` | 2707–2712 | Log "key → PTY write ({bytes}) after {:?}" |
| `AppState` | 658–659 | `key_press_t0: Option<Instant>` field |

### What is currently NOT instrumented

The gap between PTY bytes arriving at the alacritty reader thread and `about_to_wait`
picking up the wakeup is not measured. The full latency chain is:

```
PTY byte arrives on fd
  → alacritty reader thread processes byte through Term
  → Event::Wakeup sent via ArcTermEventListener::send_event
  → wakeup_tx.send(()) [terminal.rs:113]
  ── interval: not measured ──
  → about_to_wait fires, drains wakeup_rx
  → snapshot_from_term() called
  → render_multipane() called
  → frame presented
```

The unmeasured segment is the time from `wakeup_tx.send(())` to the `about_to_wait`
invocation that drains it. This could be 0ms (if `Poll`) or up to a full frame
interval (if `Wait`).

### Where to add timestamps for Phase 13

Two additions would complete the chain:

**Addition 1: timestamp at wakeup send**

File: `arcterm-app/src/terminal.rs`, `ArcTermEventListener::send_event`, line 113.

```rust
#[cfg(feature = "latency-trace")]
{
    use std::time::SystemTime;
    if let Ok(ts) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        log::debug!("[latency] wakeup sent at {}ms", ts.as_millis());
    }
}
```

This is in the reader thread; using `log::debug!` is safe (env_logger is thread-safe).
Alternatively, store the `Instant` in an `Arc<Mutex<Option<Instant>>>` shared with
`Terminal` so it can be retrieved in `has_wakeup()` and included in the drain log.

**Addition 2: timestamp at snapshot lock**

File: `arcterm-app/src/main.rs`, inside the `had_wakeup` branch, just before
`terminal.lock_term()` (line 1512).

```rust
#[cfg(feature = "latency-trace")]
let t_snapshot = TraceInstant::now();
let detect_snapshot = {
    let term = terminal.lock_term();
    arcterm_render::snapshot_from_term(&*term)
};
#[cfg(feature = "latency-trace")]
log::debug!("[latency] snapshot acquired in {:?}", t_snapshot.elapsed());
```

This measures how long the `Term` lock is held and the snapshot copy takes.

Both additions are non-intrusive (compile-time gated) and require no new types.

---

## 5. PaneId Type

File: `arcterm-app/src/layout.rs`, lines 11–22.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PaneId(pub u64);

static NEXT_PANE_ID: AtomicU64 = AtomicU64::new(1);

impl PaneId {
    pub fn next() -> Self {
        PaneId(NEXT_PANE_ID.fetch_add(1, Ordering::Relaxed))
    }
}
```

`PaneId` is a `Copy` newtype over `u64` with `Hash` derived — directly usable as a
`HashMap` key. It lives in `arcterm-app/src/layout.rs` and is imported elsewhere
via `use crate::layout::PaneId`.

For REVIEW-3.1-C (`HashMap<PaneId, Vec<u64>>`), this type can be used directly.
However, `arcterm-render` does not depend on `arcterm-app`, so `PaneId` cannot be
used inside `arcterm-render/src/renderer.rs`. The recommended design (see Section 1B,
Option 1) avoids this by keying on slot index (`usize`) inside `Renderer`, not on
`PaneId`.

---

## 6. Comparison Matrix — Dirty-Row Hash Storage Options

| Criteria | Option A: Hash map in `Renderer` (slot index key) | Option B: Hash map in `AppState` (PaneId key) | Option C: Hash vec field in `TextRenderer` (single pane only) |
|----------|----------------------------------------------------|-----------------------------------------------|---------------------------------------------------------------|
| Crate boundary | Self-contained in arcterm-render | Requires passing hashes through PaneRenderInfo; crosses crate boundary | Self-contained but only covers single-pane path |
| PaneId coupling | None | Requires arcterm-render to import or generic over PaneId | None |
| Invalidation on resize | `renderer.pane_row_hashes.clear()` in resize handler | Same map clear in resize handler | `text.row_hashes.clear()` |
| Implementation complexity | Low — add one HashMap field to Renderer | Medium — add to AppState, add field to PaneRenderInfo | N/A (already exists for single-pane) |
| Covers production path | Yes (`render_multipane`) | Yes | No (`prepare_grid` is unused in production) |

---

## 7. Specific Code Locations for Each Change

### Dirty-row cache

| Change | File | Lines affected |
|--------|------|----------------|
| Add `pane_row_hashes: HashMap<usize, Vec<u64>>` field to `Renderer` | `arcterm-render/src/renderer.rs` | after line 70 (existing fields) |
| Add `row_hashes: &mut Vec<u64>` parameter to `prepare_grid_at` | `arcterm-render/src/text.rs` | line 229 (signature) |
| Add hash-check + skip logic in `prepare_grid_at` row loop | `arcterm-render/src/text.rs` | lines 260–270 |
| Pass `&mut self.pane_row_hashes.entry(slot).or_default()` in caller | `arcterm-render/src/renderer.rs` | line 189 |
| Invalidate on resize | `arcterm-app/src/main.rs` | line 1748 (resize handler) |
| Invalidate on palette change | `arcterm-app/src/main.rs` | line 1358 (config reload) |

### Buffer pool truncation fix (REVIEW-3.1-B)

| Change | File | Line |
|--------|------|------|
| Remove `self.pane_buffer_pool.truncate(0)` | `arcterm-render/src/text.rs` | 134 |

### Frame pacing (PresentMode::Fifo)

| Change | File | Lines affected |
|--------|------|----------------|
| Replace Mailbox preference with `PresentMode::Fifo` | `arcterm-render/src/gpu.rs` | 56–61 |

### Latency trace additions

| Change | File | Lines affected |
|--------|------|----------------|
| Timestamp at wakeup send | `arcterm-app/src/terminal.rs` | 113 (inside `Event::Wakeup` arm) |
| Timestamp at snapshot lock | `arcterm-app/src/main.rs` | ~1512 (before `terminal.lock_term()`) |

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Per-pane hash map grows unbounded as panes are opened and closed | Low | Low | Clear entry from `pane_row_hashes` when a pane closes (same `closed_panes` cleanup block at main.rs line 1655) |
| Hash collision causes stale row display | Very Low | Medium | `DefaultHasher` collision probability across 80 cells is negligible; use `u64::MAX` sentinel for "always-dirty" to handle edge cases |
| `Fifo` on Linux/X11 introduces input latency on slow vsync | Low | Medium | Make present mode configurable in `ArctermConfig`; default to `Fifo`, allow `Mailbox` opt-in |
| Removing `pane_buffer_pool.truncate(0)` causes stale glyph data from a deleted pane to render in a new pane's slot | Low | Low | Pool entries are overwritten by `prepare_grid_at` via `buf_vec.truncate(num_rows)` (line 257) before any row is used; no stale data escapes |
| Latency trace log volume is high under heavy PTY load | Medium | Low | Log at `debug` level (already the case); users must set `RUST_LOG=debug` explicitly |

---

## Implementation Considerations

### Integration points with existing code

- `reset_frame` is called once per frame inside `render_multipane` (renderer.rs
  line 165). The buffer pool fix (remove line 134) is safe only because `prepare_grid_at`
  already handles pool growth at line 244. Verify no path shrinks `pane_buffer_pool`
  between calls.

- The `prepare_grid` single-pane path (`text.rs` line 141) is currently unused in
  the production codepath (all rendering goes through `render_multipane`). Its
  `row_hashes` mechanism can be left as-is or removed; it should not be modified as
  part of this work to avoid scope creep.

- `PaneRenderInfo` does not carry a `PaneId`. If Option 1 (slot-index-keyed hash map
  in `Renderer`) is used, no changes to `PaneRenderInfo` or `main.rs`'s render call
  sites are needed.

### Migration path

No migration required. Changes are additive (new field on `Renderer`, new parameter
on `prepare_grid_at`) or subtractive (remove `truncate(0)` line). No stored state
needs to be converted. The hash vecs auto-populate on first use (initialized to
`u64::MAX` = always dirty).

### Testing strategy

- Unit test: verify `prepare_grid_at` skips `shape_row_into_buffer` for an unchanged
  row (mock a `RenderSnapshot` with a fixed hash vec; call twice; assert second call
  produces the same `Buffer` state without reshaping). This is testable without a GPU
  via `cfg(test)` with a headless path.
- Regression test: verify that modifying a single cell in one pane does not skip
  reshaping for that row in any other pane (hash vec is per-pane, not shared).
- Manual: run `RUST_LOG=debug cargo run -p arcterm-app --features latency-trace` and
  verify frame latency logs appear; run `vim` and `htop` and confirm no rendering
  artifacts from stale buffer pool entries.

### Performance implications

At 60fps with a 2-pane layout, each 24-row pane incurs 24 × `hash_row` calls per
frame even when idle. `hash_row` hashes 80 cells × ~8 fields each = ~640 hash
operations per row. This is approximately 24 × 640 × 2 = 30,720 hash operations per
frame per 24-row pane, which at ~2ns each is ~60 microseconds total. This is well
within budget compared to the text-shaping cost it replaces (glyphon `set_rich_text`
+ `shape_until_scroll` per row is order-of-magnitude more expensive).

---

## Sources

All findings are based on direct codebase reading. No external sources were consulted
for this research — all claims are grounded in specific file lines cited above.

1. `arcterm-render/src/text.rs` — full read, all 912 lines
2. `arcterm-render/src/gpu.rs` — full read, 110 lines
3. `arcterm-render/src/renderer.rs` — lines 1–250
4. `arcterm-app/src/main.rs` — lines 1–220, 500–660, 1190–1260, 1261–1717,
   1980–2100, 2350–2450, 2690–2750
5. `arcterm-app/src/terminal.rs` — lines 90–145, 541–570
6. `arcterm-app/src/layout.rs` — lines 1–55
7. `arcterm-app/Cargo.toml` — full read
8. `.shipyard/ISSUES.md` — REVIEW-3.1-B (lines 304–310), REVIEW-3.1-C (lines 312–318)

---

## Uncertainty Flags

- **`Mailbox` availability on macOS**: The code assumes `Mailbox` is available on
  Metal and falls back to `Fifo` otherwise. The research confirms this is the code
  path, but actual runtime behavior depends on the Metal driver version. If `Mailbox`
  is unavailable on the developer's machine, `Fifo` is already in use and the frame
  pacing change has no observable effect.

- **Hash collision sensitivity**: `DefaultHasher` is not a cryptographic hash and
  its output can change between Rust versions. This is acceptable for dirty-row
  detection (a false-negative means an unnecessary re-shape, not a correctness bug),
  but it means tests that assert specific hash values will be Rust-version-sensitive.
  Tests should assert structural properties (same input → same hash) rather than
  specific numeric values.

- **`prepare_grid` single-pane path reachability**: Based on reading `renderer.rs`,
  `prepare_grid` (the single-pane path with `row_hashes`) does not appear to be
  called from `render_multipane`. Whether any code path outside `render_multipane`
  calls `prepare_grid` was not exhaustively traced; a full grep would confirm.
  Current evidence: no call site found for `prepare_grid` outside of `text.rs`
  itself (tests only).
