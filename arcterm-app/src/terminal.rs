//! Terminal struct — wires PTY, VT processor, and Grid together.

use arcterm_core::{Grid, GridSize};
use arcterm_pty::{PtyError, PtySession};
use arcterm_render::ContentType;
use arcterm_vt::{ApcScanner, GridState};
use crate::kitty_types::{KittyChunkAssembler, KittyCommand, parse_kitty_command};
use crate::osc7770::StructuredContentAccumulator;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

/// Decoded image ready for GPU texture upload.
///
/// Produced by the Kitty graphics pipeline after receiving a complete
/// (possibly chunked) APC sequence and decoding PNG/JPEG to raw RGBA bytes.
/// Delivered to the app layer via an `mpsc::Receiver<PendingImage>` returned
/// by `Terminal::new()`.
pub struct PendingImage {
    /// Parsed Kitty command metadata (action, format, id, etc.).
    pub command: KittyCommand,
    /// Raw RGBA pixel data (width × height × 4 bytes).
    pub rgba: Vec<u8>,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
}

/// Return type of [`Terminal::new()`]: the terminal instance plus its two receivers.
///
/// - `Terminal` — the terminal struct itself.
/// - `mpsc::Receiver<Vec<u8>>` — raw PTY byte stream.
/// - `mpsc::Receiver<PendingImage>` — decoded Kitty images, produced on the
///   tokio blocking thread pool and ready for GPU texture upload.
pub type TerminalChannels = (Terminal, mpsc::Receiver<Vec<u8>>, mpsc::Receiver<PendingImage>);

/// Integrates PTY I/O, VT parsing, and the terminal grid.
pub struct Terminal {
    pty: PtySession,
    scanner: ApcScanner,
    grid_state: GridState,
    /// Kitty chunk assembler — buffers multi-chunk APC transfers by image ID.
    chunk_assembler: KittyChunkAssembler,
    /// Sender half of the image decode channel.
    ///
    /// Cloned into each `tokio::task::spawn_blocking` closure that decodes a
    /// Kitty image.  The receiver is returned by `Terminal::new()` so the app
    /// layer can drain completed images via `try_recv` in `about_to_wait`.
    image_tx: mpsc::Sender<PendingImage>,
}

impl Terminal {
    /// Spawn a new terminal at the given grid size.
    ///
    /// `shell` optionally overrides the shell binary path.  Pass `None` to
    /// auto-detect via `$SHELL` / platform default.
    ///
    /// `cwd` optionally sets the working directory for the spawned shell.
    /// Pass `None` to inherit the current process's working directory.
    ///
    /// Returns `(terminal, pty_rx, image_rx)` where `pty_rx` delivers raw PTY
    /// bytes and `image_rx` delivers decoded `PendingImage` values produced by
    /// `process_pty_output` on the tokio blocking thread pool.  Both receivers
    /// are returned separately so the `App` layer owns them and can drain them
    /// via `try_recv` in `about_to_wait`.
    pub fn new(
        size: GridSize,
        shell: Option<String>,
        cwd: Option<&Path>,
    ) -> Result<TerminalChannels, PtyError> {
        let (pty, rx) = PtySession::new(size, shell, cwd)?;
        let scanner = ApcScanner::new();
        let grid_state = GridState::new(Grid::new(size));
        let (image_tx, image_rx) = mpsc::channel(32);
        Ok((
            Terminal {
                pty,
                scanner,
                grid_state,
                chunk_assembler: KittyChunkAssembler::new(),
                image_tx,
            },
            rx,
            image_rx,
        ))
    }

    /// Feed raw PTY output bytes through the VT processor into the grid.
    ///
    /// Also processes any Kitty APC sequences received during this batch:
    /// payloads are parsed and chunk-assembled here (synchronous, fast).
    /// When a complete image transfer is assembled, decoding is offloaded to
    /// `tokio::task::spawn_blocking`; decoded images are delivered to the app
    /// layer via the `mpsc::Receiver<PendingImage>` returned by `new()`.
    ///
    /// Note: decoded images arrive with one-frame latency relative to the PTY
    /// data that triggered them; this is acceptable for Kitty image rendering.
    pub fn process_pty_output(&mut self, bytes: &[u8]) {
        self.scanner.advance(&mut self.grid_state, bytes);

        // Drain any Kitty payloads the scanner dispatched into GridState.
        let payloads = self.grid_state.take_kitty_payloads();
        for raw in payloads {
            if let Some(cmd) = parse_kitty_command(&raw)
                && let Some((meta, decoded_bytes)) = self.chunk_assembler.receive_chunk(&cmd)
            {
                // Offload PNG/JPEG decoding to the tokio blocking thread pool.
                // The Sender is cheap to clone; the closure captures ownership
                // of the raw bytes and metadata so no shared state is needed.
                let tx = self.image_tx.clone();
                tokio::task::spawn_blocking(move || {
                    match image::load_from_memory(&decoded_bytes) {
                        Ok(dyn_img) => {
                            let rgba_img = dyn_img.to_rgba8();
                            let width = rgba_img.width();
                            let height = rgba_img.height();
                            let img = PendingImage {
                                command: meta,
                                rgba: rgba_img.into_raw(),
                                width,
                                height,
                            };
                            // try_send: if the channel is full or closed, warn
                            // and drop the image rather than blocking.
                            if let Err(e) = tx.try_send(img) {
                                log::warn!("Kitty image channel send failed: {e}");
                            }
                        }
                        Err(e) => {
                            log::warn!(
                                "Kitty image decode failed for image_id={}: {e}",
                                meta.image_id
                            );
                        }
                    }
                });
            }
        }
    }

    /// Drain and return all pending DSR/DA reply bytes queued by the VT processor.
    ///
    /// The caller is responsible for writing each reply to the PTY.
    pub fn take_pending_replies(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.grid_state.grid.pending_replies)
    }

    /// Drain and return all completed OSC 7770 structured content blocks.
    ///
    /// Converts from `arcterm_vt::StructuredContentAccumulator` (which carries
    /// `arcterm_vt::ContentType`) to the local `StructuredContentAccumulator`
    /// (which carries `arcterm_render::ContentType`).  This bridge will be
    /// removed when `GridState` is replaced in Wave 2.
    pub fn take_completed_blocks(&mut self) -> Vec<StructuredContentAccumulator> {
        std::mem::take(&mut self.grid_state.completed_blocks)
            .into_iter()
            .map(|acc| {
                let ct = vt_content_type_to_render(acc.content_type);
                StructuredContentAccumulator {
                    content_type: ct,
                    attrs: acc.attrs,
                    buffer: acc.buffer,
                }
            })
            .collect()
    }

    /// Drain and return all shell exit codes received via OSC 133 D since the
    /// last call.  The app layer stores the last value in the per-pane `PaneContext`.
    pub fn take_exit_codes(&mut self) -> Vec<i32> {
        self.grid_state.take_exit_codes()
    }

    /// Drain and return all pending MCP tool-list queries.
    ///
    /// One `()` per `ESC ] 7770 ; tools/list ST` sequence received since the
    /// last call.  The app layer responds with `ESC ] 7770 ; tools/response ; … ST`.
    pub fn take_tool_queries(&mut self) -> Vec<()> {
        self.grid_state.take_tool_queries()
    }

    /// Drain and return all pending MCP tool calls as `(name, args_json)` pairs.
    ///
    /// One entry per `ESC ] 7770 ; tools/call ; name=… ; args=… ST` sequence received
    /// since the last call.  The app layer invokes the tool and writes back
    /// `ESC ] 7770 ; tools/result ; result=<base64_json> ST`.
    pub fn take_tool_calls(&mut self) -> Vec<(String, String)> {
        self.grid_state.take_tool_calls()
    }

    /// Drain and return all pending cross-pane context queries.
    ///
    /// One `()` per `ESC ] 7770 ; context/query ST` sequence received since the
    /// last call.  The app layer responds with a sibling context JSON block written
    /// back to the querying pane's PTY input.
    pub fn take_context_queries(&mut self) -> Vec<()> {
        self.grid_state.take_context_queries()
    }

    /// Return a reference to the underlying GridState.
    pub fn grid_state(&self) -> &GridState {
        &self.grid_state
    }

    /// Write raw input bytes to the PTY (shell stdin).
    ///
    /// Errors (e.g. broken pipe after shell exit) are logged and swallowed
    /// so the caller does not need to handle them.
    pub fn write_input(&mut self, data: &[u8]) {
        if let Err(e) = self.pty.write(data) {
            log::warn!("PTY write error: {e}");
        }
    }

    /// Return an immutable reference to the terminal grid.
    pub fn grid(&self) -> &Grid {
        &self.grid_state.grid
    }

    /// Return a mutable reference to the terminal grid.
    ///
    /// Used by the application layer to adjust `scroll_offset` and clear
    /// selections without going through the VT processor.
    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.grid_state.grid
    }

    /// Set the viewport scroll offset directly.
    ///
    /// Clamped to `[0, scrollback_len]` by the inner `Grid` accessor.
    #[allow(dead_code)] // Used in Wave 3 integration
    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.grid_state.grid.set_scroll_offset(offset);
    }

    /// Resize both the PTY and the grid.
    pub fn resize(&mut self, size: GridSize) {
        if let Err(e) = self.pty.resize(size) {
            log::warn!("PTY resize error: {e}");
        }
        self.grid_state.grid.resize(size);
        self.grid_state.scroll_bottom = size.rows.saturating_sub(1);
    }

    /// Returns `true` if the child shell process is still alive.
    /// Reserved for future use (e.g. process status monitoring in Phase 2).
    #[allow(dead_code)]
    pub fn is_alive(&mut self) -> bool {
        self.pty.is_alive()
    }

    /// Returns the PID of the child shell process, if available.
    pub fn child_pid(&self) -> Option<u32> {
        self.pty.child_pid()
    }

    /// Returns the current working directory of the child shell process.
    ///
    /// Delegates to `PtySession::cwd()`. Returns `None` if the PID is
    /// unavailable, the process has exited, or the platform is unsupported.
    ///
    /// Used by workspace session save (Wave 2) to capture each pane's CWD.
    #[allow(dead_code)]
    pub fn cwd(&self) -> Option<PathBuf> {
        self.pty.cwd()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Convert `arcterm_vt::ContentType` to `arcterm_render::ContentType`.
///
/// Both enums carry identical variants.  This bridge exists because
/// `arcterm_vt::GridState::completed_blocks` stores the arcterm-vt variant;
/// `take_completed_blocks` converts at the boundary so callers only ever
/// see the canonical `arcterm_render::ContentType`.
fn vt_content_type_to_render(ct: arcterm_vt::ContentType) -> ContentType {
    match ct {
        arcterm_vt::ContentType::CodeBlock => ContentType::CodeBlock,
        arcterm_vt::ContentType::Diff      => ContentType::Diff,
        arcterm_vt::ContentType::Plan      => ContentType::Plan,
        arcterm_vt::ContentType::Markdown  => ContentType::Markdown,
        arcterm_vt::ContentType::Json      => ContentType::Json,
        arcterm_vt::ContentType::Error     => ContentType::Error,
        arcterm_vt::ContentType::Progress  => ContentType::Progress,
        arcterm_vt::ContentType::Image     => ContentType::Image,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::PendingImage;
    use crate::kitty_types::{KittyAction, KittyCommand, KittyFormat};
    use std::io::Cursor;
    use tokio::sync::mpsc;

    /// Build a minimal `KittyCommand` suitable for test assertions.
    fn dummy_kitty_command() -> KittyCommand {
        KittyCommand {
            action: KittyAction::TransmitAndDisplay,
            format: KittyFormat::Png,
            image_id: 1,
            more_chunks: false,
            quiet: 0,
            cols: None,
            rows: None,
            payload_base64: Vec::new(),
        }
    }

    /// Regression test: verify that `spawn_blocking` + `mpsc` channel deliver
    /// a decoded `PendingImage` correctly without requiring a live PTY.
    ///
    /// Constructs a minimal 1×1 PNG in memory, decodes it on the tokio blocking
    /// thread pool, sends it through a bounded channel, and asserts the received
    /// image has the expected dimensions and pixel data length.
    #[tokio::test]
    async fn async_image_decode_via_channel() {
        let (tx, mut rx) = mpsc::channel::<PendingImage>(32);

        let meta = dummy_kitty_command();
        let handle = tokio::task::spawn_blocking(move || {
            // Build a minimal 1×1 RGBA image and encode it to PNG bytes.
            let img = image::RgbaImage::new(1, 1);
            let dyn_img = image::DynamicImage::ImageRgba8(img);
            let mut buf = Cursor::new(Vec::new());
            dyn_img
                .write_to(&mut buf, image::ImageFormat::Png)
                .expect("PNG encode failed");
            let png_bytes = buf.into_inner();

            // Decode back to RGBA via image::load_from_memory.
            let decoded = image::load_from_memory(&png_bytes).expect("PNG decode failed");
            let rgba_img = decoded.to_rgba8();
            let width = rgba_img.width();
            let height = rgba_img.height();

            // Send through the channel exactly as process_pty_output would.
            let pending = PendingImage {
                command: meta,
                rgba: rgba_img.into_raw(),
                width,
                height,
            };
            tx.try_send(pending).expect("channel send failed");
        });

        handle.await.expect("spawn_blocking task panicked");

        // Drain the channel and assert exactly one image arrived.
        let img = rx.try_recv().expect("expected one PendingImage in channel");
        assert_eq!(img.width, 1, "width should be 1");
        assert_eq!(img.height, 1, "height should be 1");
        assert_eq!(img.rgba.len(), 4, "1 pixel × 4 RGBA bytes");

        // Channel should be empty now.
        assert!(rx.try_recv().is_err(), "channel should be empty after one recv");
    }
}
