---
phase: config-runtime-hardening
plan: "2.1"
wave: 2
dependencies: ["1.2"]
must_haves:
  - Kitty image decode runs on spawn_blocking, not inline in process_pty_output
  - mpsc channel carries PendingImage from blocking thread to app layer
  - Terminal::new() returns image receiver alongside PTY byte receiver
  - main.rs drains image channel via try_recv in about_to_wait
  - #[allow(dead_code)] annotations on pending_images/take_pending_images removed
  - one-frame latency for decoded images is acceptable (documented)
files_touched:
  - arcterm-app/src/terminal.rs
  - arcterm-app/src/main.rs
tdd: false
---

# PLAN-2.1 — M-3: Async Kitty Image Decode

## Context

`process_pty_output()` in terminal.rs (line 92) calls `image::load_from_memory()`
synchronously inline in the PTY processing loop, blocking the main event loop on large
images. The fix uses `tokio::task::spawn_blocking` to decode images on the tokio
blocking thread pool and an `mpsc::Sender<PendingImage>` to deliver results.

This plan depends on PLAN-1.2 (M-5) because both touch `main.rs`. PLAN-1.2 changes
`Renderer::new()` at line 1003; this plan changes the image drain at line 1452 and the
`Terminal::new()` return at line ~1030. Applying M-5 first avoids merge conflicts.

### Channel design decisions
- **Bounded channel** with capacity 32: prevents unbounded memory growth during image
  bursts while being generous enough that `try_send` failures are rare.
- **`try_send` in spawn_blocking**: if the channel is full, log a warning and drop
  the image (preferable to blocking the tokio blocking thread).
- **`try_recv` drain in about_to_wait**: non-blocking, matches existing PTY byte
  drain pattern.

### Key structural changes
- `Terminal` struct: replace `pending_images: Vec<PendingImage>` with
  `image_tx: mpsc::Sender<PendingImage>`.
- `Terminal::new()`: create `mpsc::channel(32)`, store `tx` in Terminal, return `rx`
  as a third element: `(Self, pty_rx, image_rx)`.
- Remove `take_pending_images()` method and its `#[allow(dead_code)]`.
- Remove `#[allow(dead_code)]` on `PendingImage` struct (it will be used via channel).
- All call sites of `Terminal::new()` in main.rs must destructure the new 3-tuple and
  store the image receiver (in `image_channels` map alongside `pty_channels`).

## Tasks

<task id="1" files="arcterm-app/src/terminal.rs" tdd="false">
  <action>
  Modify the `Terminal` struct:
  1. Remove `pub pending_images: Vec<PendingImage>` field.
  2. Add `image_tx: mpsc::Sender<PendingImage>` field.
  3. Remove `#[allow(dead_code)]` from the `PendingImage` struct definition (line 13).

  Modify `Terminal::new()`:
  1. Create `let (image_tx, image_rx) = mpsc::channel(32);`.
  2. Store `image_tx` in the Terminal struct (replacing `pending_images: Vec::new()`).
  3. Change return type to `Result<(Self, mpsc::Receiver<Vec<u8>>, mpsc::Receiver<PendingImage>), PtyError>`.
  4. Return the `image_rx` as the third tuple element.

  Modify `process_pty_output()`:
  1. Replace the inline `image::load_from_memory` block (lines 92-110) with a
     `tokio::task::spawn_blocking` call.
  2. Clone `self.image_tx` (Sender is cheap to clone) and move it into the closure
     along with `decoded_bytes` and `meta`.
  3. Inside the closure: decode the image, construct `PendingImage`, send via
     `tx.blocking_send()`. On decode error, `log::warn!` as before. On channel
     full/closed, `log::warn!` and drop the image.

  Remove `take_pending_images()` method entirely (lines 115-119), including its
  `#[allow(dead_code)]` annotation.
  </action>
  <verify>cargo check --package arcterm-app 2>&1 | head -30</verify>
  <done>`Terminal::new()` returns 3-tuple with image receiver. `process_pty_output` spawns blocking decode. No `pending_images` vec or `take_pending_images` method remains. `#[allow(dead_code)]` on PendingImage removed.</done>
</task>

<task id="2" files="arcterm-app/src/main.rs" tdd="false">
  <action>
  Update all `Terminal::new()` call sites to destructure the 3-tuple. There are
  four call sites in main.rs (lines ~351, ~832, ~912, ~1067) plus the initial spawn
  in `resumed()`. At each site:
  1. Change `let (terminal, pty_rx) = Terminal::new(...)` to
     `let (terminal, pty_rx, image_rx) = Terminal::new(...)`.
  2. Store `image_rx` in a parallel `image_channels: HashMap<PaneId, mpsc::Receiver<PendingImage>>`
     map (add this field to `AppState` or alongside `pty_channels`).

  Update the image drain in `about_to_wait` (line ~1452):
  1. Replace `let pending = terminal.take_pending_images();` and the `for img in pending`
     loop with a `try_recv` drain loop on the corresponding `image_rx`:
     ```
     if let Some(img_rx) = state.image_channels.get_mut(&pane_id) {
         while let Ok(img) = img_rx.try_recv() {
             // existing upload_image + placement logic unchanged
         }
     }
     ```
  2. The upload/placement body inside the loop stays identical.

  Update `close_pane()` (wherever pane cleanup happens) to also remove the
  `image_channels` entry for the closed pane.

  Add the necessary import for `PendingImage` if not already imported in main.rs.
  </action>
  <verify>cargo check --package arcterm-app 2>&1 | head -30 && cargo xc 2>&1 | tail -5</verify>
  <done>All `Terminal::new()` call sites updated. Image drain uses `try_recv` on channel receiver. `cargo check` and `cargo xc` (clippy) pass clean. No `take_pending_images` calls remain in main.rs. No `#[allow(dead_code)]` on image-related items.</done>
</task>

<task id="3" files="arcterm-app/src/terminal.rs" tdd="true">
  <action>
  Add a `#[cfg(test)] mod tests` block to `terminal.rs`. Write a regression test
  `async_image_decode_via_channel` using `#[tokio::test]`:

  1. Create an `mpsc::channel::<PendingImage>(32)` directly (bypassing Terminal::new
     which requires a live PTY).
  2. Clone the sender. Spawn a `tokio::task::spawn_blocking` closure that:
     - Creates a minimal 1x1 PNG in memory (use `image::RgbaImage::new(1, 1)` +
       `image::DynamicImage::ImageRgba8(img).write_to(&mut cursor, image::ImageFormat::Png)`).
     - Decodes it with `image::load_from_memory`.
     - Sends a `PendingImage` through the channel.
  3. Await the join handle.
  4. `try_recv()` on the receiver and assert:
     - Received exactly one `PendingImage`.
     - `width == 1`, `height == 1`.
     - `rgba.len() == 4` (1 pixel x 4 bytes).

  This tests the async decode + channel delivery pattern without needing a PTY.
  </action>
  <verify>cargo test --package arcterm-app -- terminal::tests::async_image_decode_via_channel --exact</verify>
  <done>Test passes. Confirms spawn_blocking + mpsc channel pattern delivers decoded images correctly.</done>
</task>
