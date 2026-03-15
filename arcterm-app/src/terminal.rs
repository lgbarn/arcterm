//! Terminal struct — wires PTY, VT processor, and Grid together.

use arcterm_core::{Grid, GridSize};
use arcterm_pty::{PtyError, PtySession};
use arcterm_vt::Processor;
use tokio::sync::mpsc;

/// Integrates PTY I/O, VT parsing, and the terminal grid.
pub struct Terminal {
    pty: PtySession,
    processor: Processor,
    grid: Grid,
}

impl Terminal {
    /// Spawn a new terminal at the given grid size.
    ///
    /// Returns `(terminal, receiver)` where `receiver` delivers raw PTY bytes.
    /// The receiver is returned separately so the `App` layer owns it and can
    /// poll it in `about_to_wait`.
    pub fn new(size: GridSize) -> Result<(Self, mpsc::Receiver<Vec<u8>>), PtyError> {
        let (pty, rx) = PtySession::new(size)?;
        let processor = Processor::new();
        let grid = Grid::new(size);
        Ok((
            Terminal {
                pty,
                processor,
                grid,
            },
            rx,
        ))
    }

    /// Feed raw PTY output bytes through the VT processor into the grid.
    pub fn process_pty_output(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.grid, bytes);
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
        &self.grid
    }

    /// Resize both the PTY and the grid.
    pub fn resize(&mut self, size: GridSize) {
        if let Err(e) = self.pty.resize(size) {
            log::warn!("PTY resize error: {e}");
        }
        self.grid.resize(size);
    }

    /// Returns `true` if the child shell process is still alive.
    /// Reserved for future use (e.g. process status monitoring in Phase 2).
    #[allow(dead_code)]
    pub fn is_alive(&mut self) -> bool {
        self.pty.is_alive()
    }
}
