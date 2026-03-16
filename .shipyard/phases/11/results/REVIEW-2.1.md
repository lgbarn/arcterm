---
plan: "2.1"
phase: config-runtime-hardening
reviewer: claude-sonnet-4-6
verdict: APPROVE
stage1: PASS
stage2: PASS
critical: 0
important: 0
suggestions: 1
---

# REVIEW-2.1 — M-3: Async Kitty Image Decode

## Stage 1: Spec Compliance
**Verdict:** PASS

### Task 1: terminal.rs structural changes

- Status: PASS
- Evidence:
  - `arcterm-app/src/terminal.rs:15` — `PendingImage` struct has no `#[allow(dead_code)]`; the annotation is replaced with a doc comment describing channel delivery.
  - `arcterm-app/src/terminal.rs:38-46` — `Terminal` struct contains `image_tx: mpsc::Sender<PendingImage>`; the `pub pending_images: Vec<PendingImage>` field is gone.
  - `arcterm-app/src/terminal.rs:67` — `Terminal::new()` returns `Result<TerminalChannels, PtyError>` where `TerminalChannels` is `(Terminal, mpsc::Receiver<Vec<u8>>, mpsc::Receiver<PendingImage>)` (line 32 type alias).
  - `arcterm-app/src/terminal.rs:69-80` — `new()` calls `mpsc::channel(32)`, stores `image_tx` in the struct, returns `image_rx` as the third element.
  - `arcterm-app/src/terminal.rs:108-130` — `process_pty_output()` calls `tokio::task::spawn_blocking`; the closure moves `decoded_bytes`, `meta`, and a cloned `tx`, decodes via `image::load_from_memory`, sends via `tx.try_send(img)`, logs on error.
  - `take_pending_images()` method is absent from the file. No `#[allow(dead_code)]` remains on image-related items.
- Notes: The `TerminalChannels` type alias (line 32) was added beyond the plan's minimal spec to satisfy `clippy::type_complexity`. This is correct and harmless.

  **Deviation — `try_send` vs `blocking_send`:** The plan's action section says `blocking_send`, but the plan's channel design section (the authoritative policy statement) says "if the channel is full, log a warning and drop the image". `blocking_send` would block the tokio blocking thread until a slot opens, which contradicts the stated policy. `try_send` implements the stated policy exactly. The deviation is correct and the summary explains it clearly.

### Task 2: main.rs call site updates

- Status: PASS
- Evidence:
  - `arcterm-app/src/main.rs:215` — `use terminal::{PendingImage, Terminal};` import added.
  - `arcterm-app/src/main.rs:330-338` — `PaneBundle` type alias now includes `HashMap<PaneId, mpsc::Receiver<PendingImage>>` as the third element.
  - `arcterm-app/src/main.rs:555` — `AppState` struct has `image_channels: HashMap<PaneId, mpsc::Receiver<PendingImage>>` field with doc comment.
  - All four `Terminal::new()` call sites updated: `spawn_default_pane` (line 347), `spawn_pane_with_cwd` (line 843), `restore_workspace` (line 920), and the `resumed()` workspace branch (line 1084). Each destructures `(mut terminal, pty_rx, image_rx)` and calls `image_channels.insert`.
  - `arcterm-app/src/main.rs:1477-1500` — `about_to_wait` image drain uses `if let Some(img_rx) = state.image_channels.get_mut(&id) { while let Ok(img) = img_rx.try_recv() { ... } }`. The `id` variable is the pane ID from the enclosing `for id in pane_ids` loop (line 1434), which is correct.
  - `image_channels.remove` is called at 8 sites (lines 888, 1691, 2975, 3001, 3081, 3278, 3293, 3337). Every `pty_channels.remove` call in the file has a matching `image_channels.remove` on the immediately following line — no cleanup site is missing.
  - No `take_pending_images` calls remain in main.rs.
- Notes: The SUMMARY claims 6 pane-removal sites; the actual diff covers 8 `image_channels.remove` calls. The additional coverage (palette action handlers for `ClosePane`) is correct and not a concern.

### Task 3: Regression test

- Status: PASS
- Evidence:
  - `arcterm-app/src/terminal.rs:255-324` — `#[cfg(test)] mod tests` block present. Contains `#[tokio::test] async fn async_image_decode_via_channel`.
  - Test creates `mpsc::channel::<PendingImage>(32)` directly (no PTY), calls `tokio::task::spawn_blocking`, encodes a 1×1 `RgbaImage` to PNG via `DynamicImage::write_to`, decodes with `image::load_from_memory`, sends `PendingImage` via `try_send`, awaits the handle, asserts `width == 1`, `height == 1`, `rgba.len() == 4`, and that the channel is empty after one recv.
  - Live run: `cargo test --package arcterm-app -- terminal::tests::async_image_decode_via_channel --exact` — **1 passed / 0 failed**.
  - `cargo check --package arcterm-app` — clean.
  - `cargo clippy -p arcterm-app -- -D warnings` — clean.

---

## Stage 2: Code Quality

### Critical
None.

### Important
None.

### Suggestions

- **Spawned `JoinHandle` from `spawn_blocking` is dropped silently**
  - File: `arcterm-app/src/terminal.rs:108`
  - `tokio::task::spawn_blocking(...)` returns a `JoinHandle<()>` that is immediately dropped by the `process_pty_output` call site. Dropping a `JoinHandle` does not cancel the task (tokio detaches it), so this is safe and matches the fire-and-forget intent. However, if the blocking closure panics, the panic is silently swallowed; there is no way to observe it from the PTY loop.
  - Remediation: If panic observability matters, assign the handle to a `Vec<JoinHandle<()>>` drained in `about_to_wait` alongside the image receiver. For the current phase this is low priority — log-on-decode-failure is already in place — but worth noting for Phase 5 image hardening.

---

## Stage 1 + 2 Integration Check

**Wave 1 compatibility:** The diff shows PLAN-1.2's `Renderer::new()` `Result` handling (lines 1013-1019) is intact and unmodified by this plan's commits. No conflicts with Wave 1 changes.

**Conventions:** `image_channels` follows the exact naming and access pattern established by `pty_channels`. Doc comments match the codebase style. Type alias follows the `PaneBundle` pattern already present.

**Pane-close hygiene:** Every code path that calls `panes.remove` + `pty_channels.remove` also calls `image_channels.remove` on the immediately following line. The HashMap will naturally drop the `Receiver`, closing the channel and allowing any pending `spawn_blocking` tasks to complete their `try_send` with an `Err(SendError)` — no resource leak.

---

## Summary
**Verdict:** APPROVE

All three plan tasks are correctly implemented. The `try_send` deviation from the plan's action text is the right call and is consistent with the plan's own stated channel design policy. The regression test passes, `cargo check` is clean, and `cargo clippy -p arcterm-app -- -D warnings` is clean.

Critical: 0 | Important: 0 | Suggestions: 1
