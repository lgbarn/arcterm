//! Terminal struct — wires PTY, VT processor, and Grid together.

use arcterm_core::{Grid, GridSize};
use arcterm_pty::{PtyError, PtySession};
use arcterm_vt::{ApcScanner, GridState, KittyChunkAssembler, KittyCommand, StructuredContentAccumulator, parse_kitty_command};
use tokio::sync::mpsc;

/// Decoded image ready for GPU texture upload.
///
/// Produced by the Kitty graphics pipeline after receiving a complete
/// (possibly chunked) APC sequence and decoding PNG/JPEG to raw RGBA bytes.
#[allow(dead_code)] // Used in Phase 5 image rendering integration
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

/// Integrates PTY I/O, VT parsing, and the terminal grid.
pub struct Terminal {
    pty: PtySession,
    scanner: ApcScanner,
    grid_state: GridState,
    /// Kitty chunk assembler — buffers multi-chunk APC transfers by image ID.
    chunk_assembler: KittyChunkAssembler,
    /// Decoded images ready for GPU texture upload.
    ///
    /// Populated by `process_pty_output` when a complete Kitty image transfer
    /// is received.  The app layer drains these via `take_pending_images`.
    ///
    /// TODO(phase-5): move PNG/JPEG decoding to a background thread for
    /// images larger than 1MB to avoid blocking the PTY processing thread.
    pub pending_images: Vec<PendingImage>,
}

impl Terminal {
    /// Spawn a new terminal at the given grid size.
    ///
    /// `shell` optionally overrides the shell binary path.  Pass `None` to
    /// auto-detect via `$SHELL` / platform default.
    ///
    /// Returns `(terminal, receiver)` where `receiver` delivers raw PTY bytes.
    /// The receiver is returned separately so the `App` layer owns it and can
    /// poll it in `about_to_wait`.
    pub fn new(
        size: GridSize,
        shell: Option<String>,
    ) -> Result<(Self, mpsc::Receiver<Vec<u8>>), PtyError> {
        let (pty, rx) = PtySession::new(size, shell)?;
        let scanner = ApcScanner::new();
        let grid_state = GridState::new(Grid::new(size));
        Ok((
            Terminal {
                pty,
                scanner,
                grid_state,
                chunk_assembler: KittyChunkAssembler::new(),
                pending_images: Vec::new(),
            },
            rx,
        ))
    }

    /// Feed raw PTY output bytes through the VT processor into the grid.
    ///
    /// Also processes any Kitty APC sequences received during this batch:
    /// payloads are parsed, chunk-assembled, and decoded to RGBA pixels.
    /// Completed images are stored in `pending_images` for the next render.
    pub fn process_pty_output(&mut self, bytes: &[u8]) {
        self.scanner.advance(&mut self.grid_state, bytes);

        // Drain any Kitty payloads the scanner dispatched into GridState.
        let payloads = self.grid_state.take_kitty_payloads();
        for raw in payloads {
            if let Some(cmd) = parse_kitty_command(&raw)
                && let Some((meta, decoded_bytes)) = self.chunk_assembler.receive_chunk(&cmd)
            {
                // Decode PNG/JPEG to RGBA using the `image` crate.
                // Synchronous decode is acceptable for Phase 4 basic support
                // (images under 1MB). For large images, async decode is a
                // Phase 5 improvement (see TODO above in pending_images).
                match image::load_from_memory(&decoded_bytes) {
                    Ok(dyn_img) => {
                        let rgba_img = dyn_img.to_rgba8();
                        let width = rgba_img.width();
                        let height = rgba_img.height();
                        self.pending_images.push(PendingImage {
                            command: meta,
                            rgba: rgba_img.into_raw(),
                            width,
                            height,
                        });
                    }
                    Err(e) => {
                        log::warn!(
                            "Kitty image decode failed for image_id={}: {e}",
                            meta.image_id
                        );
                    }
                }
            }
        }
    }

    /// Drain and return all decoded images ready for GPU texture upload.
    #[allow(dead_code)] // Wired in Phase 5 image rendering
    pub fn take_pending_images(&mut self) -> Vec<PendingImage> {
        std::mem::take(&mut self.pending_images)
    }

    /// Drain and return all pending DSR/DA reply bytes queued by the VT processor.
    ///
    /// The caller is responsible for writing each reply to the PTY.
    pub fn take_pending_replies(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.grid_state.grid.pending_replies)
    }

    /// Drain and return all completed OSC 7770 structured content blocks.
    pub fn take_completed_blocks(&mut self) -> Vec<StructuredContentAccumulator> {
        std::mem::take(&mut self.grid_state.completed_blocks)
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
    /// Clamped to `[0, scrollback_len]`.
    #[allow(dead_code)] // Used in Wave 3 integration
    pub fn set_scroll_offset(&mut self, offset: usize) {
        let max = self.grid_state.grid.scrollback.len();
        self.grid_state.grid.scroll_offset = offset.min(max);
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
}
