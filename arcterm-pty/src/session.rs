//! PTY session — shell spawning, I/O, and resize.

use arcterm_core::GridSize;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{self, Read, Write};
use tokio::sync::mpsc;

/// Errors that can arise from PTY operations.
#[derive(Debug)]
pub enum PtyError {
    SpawnFailed(String),
    IoError(io::Error),
    ResizeFailed(String),
}

impl From<io::Error> for PtyError {
    fn from(e: io::Error) -> Self {
        PtyError::IoError(e)
    }
}

impl std::fmt::Display for PtyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PtyError::SpawnFailed(s) => write!(f, "spawn failed: {s}"),
            PtyError::IoError(e) => write!(f, "I/O error: {e}"),
            PtyError::ResizeFailed(s) => write!(f, "resize failed: {s}"),
        }
    }
}

impl std::error::Error for PtyError {}

/// A live PTY session running a child shell.
pub struct PtySession {
    master: Box<dyn portable_pty::MasterPty + Send>,
    /// `None` after `shutdown()` has been called.
    writer: Option<Box<dyn Write + Send>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl PtySession {
    /// Spawn a new PTY session with the given grid size.
    ///
    /// Returns `(session, receiver)` where `receiver` delivers raw bytes from
    /// the child's stdout.  The receiver is deliberately returned separately so
    /// the application layer owns it.
    pub fn new(size: GridSize) -> Result<(Self, mpsc::Receiver<Vec<u8>>), PtyError> {
        let pty_system = NativePtySystem::default();

        let pty_size = PtySize {
            rows: size.rows as u16,
            cols: size.cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(pty_size)
            .map_err(|e| PtyError::SpawnFailed(e.to_string()))?;

        // Detect shell: $SHELL → /bin/bash (Unix) / cmd.exe (Windows).
        let shell = std::env::var("SHELL").unwrap_or_else(|_| {
            if cfg!(windows) {
                "cmd.exe".to_string()
            } else {
                "/bin/bash".to_string()
            }
        });

        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::SpawnFailed(e.to_string()))?;

        // Drop the slave end so EOF propagates correctly when the child exits.
        drop(pair.slave);

        let master = pair.master;

        let writer = master
            .take_writer()
            .map_err(|e| PtyError::SpawnFailed(e.to_string()))?;

        let mut reader = master
            .try_clone_reader()
            .map_err(|e| PtyError::SpawnFailed(e.to_string()))?;

        let (tx, rx) = mpsc::channel::<Vec<u8>>(64);

        // Dedicated OS thread for the blocking read loop.
        std::thread::Builder::new()
            .name("pty-reader".to_string())
            .spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if tx.blocking_send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| PtyError::SpawnFailed(e.to_string()))?;

        Ok((
            PtySession {
                master,
                writer: Some(writer),
                child,
            },
            rx,
        ))
    }

    /// Write raw bytes to the shell's stdin.
    ///
    /// Returns `Err(BrokenPipe)` if `shutdown()` has already been called.
    pub fn write(&mut self, data: &[u8]) -> io::Result<()> {
        match self.writer.as_mut() {
            Some(w) => {
                w.write_all(data)?;
                w.flush()
            }
            None => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "PTY session has been shut down",
            )),
        }
    }

    /// Resize the PTY to the new grid dimensions.
    pub fn resize(&self, size: GridSize) -> Result<(), PtyError> {
        self.master
            .resize(PtySize {
                rows: size.rows as u16,
                cols: size.cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::ResizeFailed(e.to_string()))
    }

    /// Returns `true` if the child process is still running.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Gracefully shut down: drop the writer (send EOF) and wait for the child
    /// to exit.
    ///
    /// After `shutdown()` returns, `is_alive()` will be `false` and the
    /// reader thread will have terminated because the master PTY closed.
    /// Subsequent calls to `write()` will return `Err(BrokenPipe)`.
    pub fn shutdown(&mut self) {
        // Explicitly take and drop the writer so the underlying file descriptor
        // is closed unambiguously, signalling EOF to the shell.
        drop(self.writer.take());
        let _ = self.child.wait();
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    fn default_size() -> GridSize {
        GridSize::new(24, 80)
    }

    // ── Task 1 tests ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_spawn_shell() {
        let (mut session, _rx) = PtySession::new(default_size()).expect("PTY spawn must succeed");
        assert!(session.is_alive(), "shell should be alive right after spawn");
    }

    #[tokio::test]
    async fn test_write_and_read() {
        let (mut session, mut rx) =
            PtySession::new(default_size()).expect("PTY spawn must succeed");

        session.write(b"echo hello_pty_test\n").expect("write must succeed");

        let mut collected = Vec::new();
        let result = timeout(Duration::from_secs(2), async {
            loop {
                if let Some(chunk) = rx.recv().await {
                    collected.extend_from_slice(&chunk);
                    let s = String::from_utf8_lossy(&collected);
                    if s.contains("hello_pty_test") {
                        return true;
                    }
                } else {
                    return false;
                }
            }
        })
        .await;

        assert!(
            result.unwrap_or(false),
            "must receive 'hello_pty_test' within 2 s; got: {:?}",
            String::from_utf8_lossy(&collected)
        );
    }

    #[tokio::test]
    async fn test_resize() {
        let (session, _rx) = PtySession::new(default_size()).expect("PTY spawn must succeed");
        session
            .resize(GridSize::new(40, 120))
            .expect("resize must not return an error");
    }

    // ── Task 2 tests ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_shell_exit_detection() {
        let (mut session, _rx) = PtySession::new(default_size()).expect("PTY spawn must succeed");
        session.write(b"exit\n").expect("write must succeed");

        let exited = timeout(Duration::from_secs(5), async {
            loop {
                if !session.is_alive() {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;

        assert!(
            exited.unwrap_or(false),
            "shell must exit within 5 s after 'exit' command"
        );
    }

    #[tokio::test]
    async fn test_recv_after_exit() {
        let (mut session, mut rx) =
            PtySession::new(default_size()).expect("PTY spawn must succeed");

        session
            .write(b"echo goodbye && exit\n")
            .expect("write must succeed");

        let mut all_output = Vec::new();
        let result = timeout(Duration::from_secs(5), async {
            while let Some(chunk) = rx.recv().await {
                all_output.extend_from_slice(&chunk);
            }
        })
        .await;

        assert!(result.is_ok(), "channel must close (None) after shell exits");

        let output = String::from_utf8_lossy(&all_output);
        assert!(
            output.contains("goodbye"),
            "output must contain 'goodbye'; got: {output:?}"
        );

        assert!(
            !session.is_alive(),
            "is_alive() must be false after shell exited"
        );
    }

    #[tokio::test]
    async fn test_write_after_exit() {
        let (mut session, _rx) = PtySession::new(default_size()).expect("PTY spawn must succeed");

        // Send exit command and wait for the shell to terminate.
        session.write(b"exit\n").expect("initial write must succeed");

        let exited = timeout(Duration::from_secs(5), async {
            loop {
                if !session.is_alive() {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;

        assert!(
            exited.unwrap_or(false),
            "shell must exit within 5 s before testing write-after-exit"
        );

        // A small extra delay ensures the OS has fully closed the PTY fd.
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Writing to a dead shell must return an error (broken pipe or similar).
        let result = session.write(b"this should fail\n");
        assert!(
            result.is_err(),
            "write after shell exit must return an error; got: {:?}",
            result
        );
    }
}
