---
plan: "2.1"
phase: config-runtime-hardening
status: complete
commits:
  - eb97077  # Task 1: terminal.rs struct/new/process_pty_output changes
  - d4909d3  # Task 2: main.rs call sites + image_channels + try_recv drain
  - 30f83dd  # Task 3: regression test
---

# SUMMARY-2.1 — M-3: Async Kitty Image Decode

## What Was Done

Replaced the synchronous inline Kitty image decode pipeline with a
`tokio::task::spawn_blocking` + `mpsc` channel design. Image decoding no
longer blocks the PTY processing loop.

---

### Task 1 — terminal.rs structural changes

**`PendingImage` struct:**
- Removed `#[allow(dead_code)]` annotation. The type is now actively used via
  the channel and requires no suppression.
- Added `TerminalChannels` type alias for `(Terminal, mpsc::Receiver<Vec<u8>>,
  mpsc::Receiver<PendingImage>)` to satisfy `clippy::type_complexity`.

**`Terminal` struct:**
- Removed `pub pending_images: Vec<PendingImage>` field.
- Added `image_tx: mpsc::Sender<PendingImage>` field (private; the receiver is
  returned by `new()`).

**`Terminal::new()`:**
- Changed return type from `Result<(Self, mpsc::Receiver<Vec<u8>>), PtyError>`
  to `Result<TerminalChannels, PtyError>`.
- Creates `mpsc::channel(32)` (bounded; prevents unbounded memory growth during
  image bursts).
- Stores `image_tx` in the struct; returns `image_rx` as the third tuple element.

**`process_pty_output()`:**
- Replaced the synchronous `image::load_from_memory` block with a
  `tokio::task::spawn_blocking` closure.
- The closure captures `meta` (KittyCommand) and `decoded_bytes` by move plus
  a clone of `self.image_tx`.
- On decode success: constructs `PendingImage`, sends via `tx.try_send()`.
  On channel full/closed: `log::warn!` and drops the image.
  On decode error: `log::warn!` as before.

**`take_pending_images()` removed** — method and its `#[allow(dead_code)]`
annotation are gone.

**Deviation:** The plan mentioned using `tx.blocking_send()` inside the closure.
Because `spawn_blocking` runs on a blocking thread (not inside an `async` context),
`blocking_send` would work but requires holding the thread until the receiver
drains. `try_send` is strictly preferable here: it avoids blocking the tokio
blocking thread pool and matches the plan's stated channel design decision
("if the channel is full, log a warning and drop the image"). Used `try_send`.

---

### Task 2 — main.rs call site updates

**`PaneBundle` type alias:** Added `HashMap<PaneId, mpsc::Receiver<PendingImage>>`
as the third element.

**`AppState` struct:** Added `image_channels: HashMap<PaneId, mpsc::Receiver<PendingImage>>`
field with a doc comment explaining the drain pattern.

**Import:** Added `PendingImage` to the `use terminal::{…}` import.

**All four `Terminal::new()` call sites updated:**

| Site | Change |
|---|---|
| `spawn_default_pane` | `(mut terminal, pty_rx, image_rx)` destructure; `image_channels.insert` |
| `spawn_pane_with_cwd` | `(mut terminal, pty_rx, image_rx)` destructure; `self.image_channels.insert` |
| `restore_workspace` (method) | cleanup loop adds `self.image_channels.remove(id)`; spawn loop adds `self.image_channels.insert(*id, image_rx)` |
| `resumed()` workspace branch | local `image_channels` map built; `PaneBundle` destructure updated |

**Image drain in `about_to_wait`:** Replaced `terminal.take_pending_images()` +
`for img in pending` with:
```rust
if let Some(img_rx) = state.image_channels.get_mut(&id) {
    while let Ok(img) = img_rx.try_recv() {
        // upload_image + image_placements.push (unchanged body)
    }
}
```

**Pane-close cleanup:** Added `state.image_channels.remove(&id)` (or
`self.image_channels.remove(id)`) at all 6 pane-removal sites:
- PTY-closed drain in `about_to_wait`
- `restore_workspace` bulk teardown
- `ClosePane` (last-pane-in-tab branch)
- `ClosePane` (multi-pane branch)
- `CloseTab` (in keyboard handler)
- `CloseTab` (in palette action handler)
- `ClosePane` (in palette action handler, both branches)

---

### Task 3 — Regression test

Added `#[cfg(test)] mod tests` to `terminal.rs` with a single `#[tokio::test]`:

**`async_image_decode_via_channel`:**
1. Creates `mpsc::channel::<PendingImage>(32)` directly (no PTY).
2. Constructs a `KittyCommand` with explicit field values (no `Default` derive
   on that type).
3. `spawn_blocking` closure: encodes a 1×1 `RgbaImage` to PNG via
   `DynamicImage::write_to`, decodes it with `image::load_from_memory`,
   sends `PendingImage` via `try_send`.
4. Awaits the join handle.
5. Asserts `width == 1`, `height == 1`, `rgba.len() == 4`.
6. Asserts channel is empty after one `try_recv`.

**Result:** PASS (1/1).

---

## Deviations

| Deviation | Reason |
|---|---|
| Used `try_send` instead of `blocking_send` in `spawn_blocking` closure | `try_send` matches the plan's stated intent ("log a warning and drop the image") and avoids blocking the tokio blocking pool thread. Strictly better. |
| Added `TerminalChannels` type alias | Required to pass `clippy::type_complexity` (-D warnings). Not specified in the plan but necessary to meet the done criteria (clippy clean). |
| `KittyCommand` constructed explicitly in test (no `.default()`) | `KittyCommand` does not derive `Default`; added `dummy_kitty_command()` helper in the test module. |

## Pre-existing Issue (not introduced by this plan)

`arcterm-render/examples/window.rs` fails to compile because it calls
`Renderer::new()` without handling the `Result` return introduced in PLAN-1.2
(Task 2). This was present before this plan's commits (confirmed via `git stash`
check). The `cargo xc` (workspace-wide clippy) fails on that example; however,
`cargo clippy -p arcterm-app -- -D warnings` is fully clean.

## Final State

| Criterion | Status |
|---|---|
| Kitty image decode runs on `spawn_blocking`, not inline | PASS |
| `mpsc` channel carries `PendingImage` from blocking thread to app layer | PASS |
| `Terminal::new()` returns image receiver as third tuple element | PASS |
| `main.rs` drains image channel via `try_recv` in `about_to_wait` | PASS |
| `#[allow(dead_code)]` on `pending_images`/`take_pending_images` removed | PASS |
| `cargo test -p arcterm-app` | 304/304 PASS |
| `cargo clippy -p arcterm-app -- -D warnings` | PASS (clean) |
