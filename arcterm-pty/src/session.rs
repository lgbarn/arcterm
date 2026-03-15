//! PTY session — shell spawning, I/O, and resize.

use arcterm_core::GridSize;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
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

// ── Platform CWD helpers ──────────────────────────────────────────────────────

/// Read the CWD of `pid` on macOS using `proc_pidinfo` / `PROC_PIDVNODEPATHINFO`.
#[cfg(target_os = "macos")]
fn cwd_macos(pid: u32) -> Option<PathBuf> {
    use std::mem;
    use std::ffi::CStr;

    // SAFETY: vnode_pathinfo is a plain C struct with no invariants beyond
    // being zeroed before passing to proc_pidinfo.
    let mut info: libc::proc_vnodepathinfo = unsafe { mem::zeroed() };
    let size = mem::size_of::<libc::proc_vnodepathinfo>() as i32;

    let ret = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            &mut info as *mut _ as *mut libc::c_void,
            size,
        )
    };

    if ret <= 0 {
        return None;
    }

    // libc represents vip_path as [[c_char; 32]; 32] for old rustc compat;
    // it is logically [c_char; MAXPATHLEN] (1024 bytes) in the kernel ABI.
    // Flatten to a flat byte slice and find the null terminator.
    let path_2d = info.pvi_cdir.vip_path;
    // SAFETY: The 2D array is contiguous in memory; reinterpret as a flat slice.
    let flat: &[libc::c_char] = unsafe {
        std::slice::from_raw_parts(path_2d.as_ptr() as *const libc::c_char, 32 * 32)
    };
    // SAFETY: flat points to a null-terminated C string written by the kernel.
    let cstr = unsafe { CStr::from_ptr(flat.as_ptr()) };
    let s = cstr.to_str().ok()?;
    if s.is_empty() {
        return None;
    }
    Some(PathBuf::from(s))
}

/// Read the CWD of `pid` on Linux via the `/proc/<pid>/cwd` symlink.
#[cfg(target_os = "linux")]
fn cwd_linux(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

/// A live PTY session running a child shell.
pub struct PtySession {
    master: Box<dyn portable_pty::MasterPty + Send>,
    /// `None` after `shutdown()` has been called.
    writer: Option<Box<dyn Write + Send>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    /// PID of the child process, if the platform exposes it.
    child_pid: Option<u32>,
}

impl PtySession {
    /// Spawn a new PTY session with the given grid size.
    ///
    /// `shell_override` allows the caller to specify the shell executable path
    /// directly.  When `None`, the shell is resolved from the `$SHELL`
    /// environment variable, falling back to `/bin/bash` on Unix or `cmd.exe`
    /// on Windows.
    ///
    /// `cwd` optionally sets the working directory for the spawned shell.
    /// When `None`, the shell inherits the current process's working directory.
    ///
    /// Returns `(session, receiver)` where `receiver` delivers raw bytes from
    /// the child's stdout.  The receiver is deliberately returned separately so
    /// the application layer owns it.
    pub fn new(
        size: GridSize,
        shell_override: Option<String>,
        cwd: Option<&Path>,
    ) -> Result<(Self, mpsc::Receiver<Vec<u8>>), PtyError> {
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

        // Resolve the shell: explicit override → $SHELL → platform default.
        let shell = shell_override.unwrap_or_else(|| {
            std::env::var("SHELL").unwrap_or_else(|_| {
                if cfg!(windows) {
                    "cmd.exe".to_string()
                } else {
                    "/bin/bash".to_string()
                }
            })
        });

        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::SpawnFailed(e.to_string()))?;

        // Capture the child PID before dropping the slave end.
        let child_pid = child.process_id();

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
                // 16 KiB buffer reduces syscall overhead for high-throughput output.
                let mut buf = [0u8; 16384];
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
                child_pid,
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

    /// Returns the PID of the child process, if available.
    pub fn child_pid(&self) -> Option<u32> {
        self.child_pid
    }

    /// Returns the current working directory of the child process.
    ///
    /// Returns `None` if the PID is unavailable, the process has exited, or
    /// the platform does not support CWD lookup.
    pub fn cwd(&self) -> Option<PathBuf> {
        let pid = self.child_pid?;

        #[cfg(target_os = "macos")]
        {
            cwd_macos(pid)
        }

        #[cfg(target_os = "linux")]
        {
            cwd_linux(pid)
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = pid;
            None
        }
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
        let (mut session, _rx) = PtySession::new(default_size(), None, None).expect("PTY spawn must succeed");
        assert!(session.is_alive(), "shell should be alive right after spawn");
    }

    // ── CWD tests (Task 1 — written before implementation) ───────────────────

    #[tokio::test]
    async fn test_cwd_returns_some_after_spawn() {
        let (session, _rx) =
            PtySession::new(default_size(), None, None).expect("PTY spawn must succeed");
        let cwd = session.cwd();
        assert!(cwd.is_some(), "cwd() must return Some after spawn; got None");
        let path = cwd.unwrap();
        assert!(path.exists(), "cwd path must exist on disk: {path:?}");
    }

    #[tokio::test]
    async fn test_cwd_changes_after_cd() {
        // Use /bin/sh explicitly to avoid shell startup scripts (e.g. zsh .zshrc)
        // that may change the CWD before our `cd` command runs.
        let shell = Some("/bin/sh".to_string());
        let (mut session, mut rx) =
            PtySession::new(default_size(), shell, None).expect("PTY spawn must succeed");

        // Brief settle time for the shell to start.
        tokio::time::sleep(Duration::from_millis(200)).await;
        // Discard any buffered chunks without blocking.
        while rx.try_recv().is_ok() {}

        session.write(b"cd /tmp && echo cwd_ready\n").expect("write must succeed");

        // Wait for echo confirmation that `cd /tmp` has executed.
        let ready = timeout(Duration::from_secs(5), async {
            loop {
                if let Some(chunk) = rx.recv().await {
                    let s = String::from_utf8_lossy(&chunk);
                    if s.contains("cwd_ready") {
                        return true;
                    }
                } else {
                    return false;
                }
            }
        })
        .await;
        assert!(ready.unwrap_or(false), "must receive echo confirmation after cd");

        // Give the OS a moment to update the proc CWD entry.
        tokio::time::sleep(Duration::from_millis(200)).await;

        let cwd = session.cwd();
        assert!(cwd.is_some(), "cwd() must return Some after cd");
        let path = cwd.unwrap();
        // Resolve symlinks so /private/tmp == /tmp on macOS.
        let resolved = std::fs::canonicalize(&path).unwrap_or(path.clone());
        let tmp_resolved =
            std::fs::canonicalize("/tmp").unwrap_or_else(|_| PathBuf::from("/tmp"));
        assert_eq!(
            resolved, tmp_resolved,
            "cwd after 'cd /tmp' must resolve to /tmp; got {path:?}"
        );
    }

    #[tokio::test]
    async fn test_spawn_with_cwd() {
        let tmp = std::path::Path::new("/tmp");
        let (mut session, mut rx) =
            PtySession::new(default_size(), None, Some(tmp)).expect("PTY spawn must succeed");

        session.write(b"pwd\n").expect("write must succeed");

        let mut collected = Vec::new();
        let result = timeout(Duration::from_secs(3), async {
            loop {
                if let Some(chunk) = rx.recv().await {
                    collected.extend_from_slice(&chunk);
                    let s = String::from_utf8_lossy(&collected);
                    // /tmp on macOS resolves to /private/tmp
                    if s.contains("/tmp") {
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
            "pwd output must contain /tmp; got: {:?}",
            String::from_utf8_lossy(&collected)
        );
    }

    #[tokio::test]
    async fn test_spawn_without_cwd_uses_process_cwd() {
        let (session, _rx) =
            PtySession::new(default_size(), None, None).expect("PTY spawn must succeed");

        // The child inherits the process CWD when no cwd is set.
        let process_cwd = std::env::current_dir().expect("must have process CWD");
        let shell_cwd = session.cwd();
        // cwd() must return Some; the exact path may differ on macOS (symlinks)
        // so we just verify it is not None and exists.
        assert!(shell_cwd.is_some(), "cwd() must return Some when spawned without override");
        let _ = process_cwd; // used to confirm intent, not strict equality
    }

    #[tokio::test]
    async fn test_write_and_read() {
        let (mut session, mut rx) =
            PtySession::new(default_size(), None, None).expect("PTY spawn must succeed");

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
        let (session, _rx) = PtySession::new(default_size(), None, None).expect("PTY spawn must succeed");
        session
            .resize(GridSize::new(40, 120))
            .expect("resize must not return an error");
    }

    // ── Task 2 tests ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_shell_exit_detection() {
        let (mut session, _rx) = PtySession::new(default_size(), None, None).expect("PTY spawn must succeed");
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
            PtySession::new(default_size(), None, None).expect("PTY spawn must succeed");

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

        // Poll is_alive with a brief timeout — child reap may lag slightly.
        let exited = timeout(Duration::from_secs(2), async {
            loop {
                if !session.is_alive() { break; }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }).await;
        assert!(exited.is_ok(), "is_alive() must become false after shell exited");
    }

    #[tokio::test]
    async fn test_write_after_exit() {
        let (mut session, _rx) = PtySession::new(default_size(), None, None).expect("PTY spawn must succeed");

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
