//! Terminal struct — wires alacritty_terminal PTY, Term, PreFilter, and Kitty pipeline.
//!
//! The `Terminal` struct replaces the previous `PtySession + GridState + ApcScanner`
//! combination. It wraps `Arc<FairMutex<Term<ArcTermEventListener>>>` and drives the
//! VT parser directly (bypassing alacritty's `EventLoop` to allow byte-level pre-filtering).
//!
//! ## Architecture
//!
//! ```text
//! shell process
//!     │
//! Pty (master fd)
//!     │
//! [Reader thread: reads raw PTY bytes → PreFilter → vte::ansi::Processor::advance → Term]
//!     │ (side channels)
//!     ├─ APC payloads    → KittyChunkAssembler → image_tx
//!     ├─ OSC 7770 params → dispatch_osc7770 → completed_blocks / tool_queries / tool_calls
//!     └─ OSC 133 events  → osc133_events (exit codes)
//!
//! [Writer thread: mpsc::Receiver<Cow<[u8]>> → writes to PTY master fd]
//!
//! [Main thread: locks Term on wakeup, calls renderable_content()]
//! ```

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self as std_mpsc};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty::{self, Options, Shell};
use alacritty_terminal::vte::ansi;

use arcterm_render::ContentType;
use tokio::sync::mpsc;

use crate::kitty_types::{KittyChunkAssembler, KittyCommand, parse_kitty_command};
use crate::osc7770::StructuredContentAccumulator;
use crate::prefilter::{Osc133Event, PreFilter};

// ---------------------------------------------------------------------------
// ArcTermSize — implements alacritty's Dimensions trait
// ---------------------------------------------------------------------------

/// Terminal grid dimensions: `(columns, screen_lines)`.
///
/// Implements `alacritty_terminal::grid::Dimensions` for use with `Term::new`.
/// We define our own type rather than using `term::test::TermSize` to avoid
/// depending on a test-module struct in production code.
#[derive(Copy, Clone, Debug)]
pub struct ArcTermSize {
    /// Number of grid columns.
    pub columns: usize,
    /// Number of visible rows (screen lines, not counting scrollback).
    pub screen_lines: usize,
}

impl ArcTermSize {
    /// Create a new size from `(cols, rows)`.
    pub fn new(columns: usize, screen_lines: usize) -> Self {
        Self { columns, screen_lines }
    }
}

impl alacritty_terminal::grid::Dimensions for ArcTermSize {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

// ---------------------------------------------------------------------------
// ArcTermEventListener
// ---------------------------------------------------------------------------

/// `EventListener` implementation for arcterm.
///
/// Receives events dispatched by `Term` during VT processing and routes them:
/// - `Wakeup` → signals the main thread that new data is available
/// - `PtyWrite(s)` → queues the reply bytes for writing back to the PTY
/// - `ChildExit(code)` → stores the exit code
/// - `Title(s)` → stores the window title
/// - Others → logged at debug level and ignored
#[derive(Clone)]
pub struct ArcTermEventListener {
    /// Channel to notify the main thread of display updates.
    wakeup_tx: std_mpsc::Sender<()>,
    /// Channel to write DSR/DA reply bytes back to the PTY.
    write_tx: std_mpsc::SyncSender<Cow<'static, [u8]>>,
    /// Shared exit-code storage (set on `ChildExit`).
    exit_code: Arc<AtomicI32>,
    /// Shared window title storage.
    title: Arc<Mutex<Option<String>>>,
}

impl EventListener for ArcTermEventListener {
    fn send_event(&self, event: Event) {
        match event {
            Event::Wakeup => {
                // Best-effort; ignore if the receiver has been dropped.
                let _ = self.wakeup_tx.send(());
                #[cfg(feature = "latency-trace")]
                log::debug!("[latency] wakeup sent at {:?}", std::time::Instant::now());
            }
            Event::PtyWrite(s) => {
                // Write the reply back to the PTY master fd via the writer thread.
                let bytes: Cow<'static, [u8]> = Cow::Owned(s.into_bytes());
                let _ = self.write_tx.try_send(bytes);
            }
            Event::ChildExit(code) => {
                self.exit_code.store(code, Ordering::Release);
                // Also wake up the main thread so it can detect exit.
                let _ = self.wakeup_tx.send(());
                #[cfg(feature = "latency-trace")]
                log::debug!("[latency] wakeup sent at {:?}", std::time::Instant::now());
            }
            Event::Title(s) => {
                if let Ok(mut guard) = self.title.lock() {
                    *guard = Some(s);
                }
            }
            Event::Bell
            | Event::ResetTitle
            | Event::MouseCursorDirty
            | Event::CursorBlinkingChange
            | Event::Exit => {
                log::debug!("Terminal event (ignored): {:?}", event);
            }
            Event::ClipboardStore(_, _)
            | Event::ClipboardLoad(_, _)
            | Event::ColorRequest(_, _)
            | Event::TextAreaSizeRequest(_) => {
                log::debug!("Terminal event (unhandled): {:?}", event);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PendingImage
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Terminal
// ---------------------------------------------------------------------------

/// Integrates PTY I/O, VT parsing, pre-filtering, and the alacritty terminal grid.
///
/// The `Terminal` struct is the primary integration point between the raw PTY byte
/// stream and the alacritty `Term` state machine. It drives the VT parser directly
/// (bypassing alacritty's `EventLoop`) so that the `PreFilter` can intercept
/// OSC 7770, OSC 133, and APC sequences before they reach alacritty's `Term`.
pub struct Terminal {
    /// The alacritty terminal state machine.
    term: Arc<FairMutex<Term<ArcTermEventListener>>>,
    /// Channel sender for writing bytes to the PTY master fd.
    write_tx: std_mpsc::SyncSender<Cow<'static, [u8]>>,
    /// PID of the spawned child shell process.
    child_pid: u32,
    /// Number of grid columns (for resize operations).
    cols: usize,
    /// Number of grid rows (for resize operations).
    rows: usize,
    /// Pre-filter state machine (only used indirectly via the reader thread,
    /// but stored here for a future direct-advance path if needed).
    _prefilter: std::marker::PhantomData<PreFilter>,
    /// Completed OSC 7770 structured content blocks, drained from the side channel.
    completed_blocks: Vec<StructuredContentAccumulator>,
    /// OSC 133 shell-integration events, drained from the side channel.
    osc133_events: Vec<Osc133Event>,
    /// Kitty chunk assembler — buffers multi-chunk APC transfers by image ID.
    chunk_assembler: KittyChunkAssembler,
    /// Sender half of the Kitty image decode channel.
    image_tx: mpsc::Sender<PendingImage>,
    /// Shared exit-code storage (populated by `ArcTermEventListener`).
    exit_code: Arc<AtomicI32>,
    /// Shared window title storage (populated by `ArcTermEventListener`).
    title: Arc<Mutex<Option<String>>>,
    /// Wakeup signal receiver — signals that Term has new data.
    wakeup_rx: std_mpsc::Receiver<()>,
    /// Side channel receiver for raw APC Kitty payloads from the reader thread.
    apc_rx: std_mpsc::Receiver<Vec<u8>>,
    /// Side channel receiver for OSC 7770 events: `(params, captured_text)`.
    ///
    /// For `"start;..."` params the `captured_text` is empty.
    /// For `"end"` params the `captured_text` contains the raw bytes that were
    /// written to the terminal between the `start` and `end` OSC 7770 markers.
    osc7770_rx: std_mpsc::Receiver<(String, String)>,
    /// Side channel receiver for OSC 133 events from the reader thread.
    osc133_rx: std_mpsc::Receiver<Osc133Event>,
    /// Owns the `Pty` so the child shell is not SIGHUP'd until `Terminal` drops.
    ///
    /// CRITICAL-1: Without this field, `pty` would drop at the end of `Terminal::new()`,
    /// causing `Pty::drop` to call `libc::kill(child_pid, SIGHUP)` and `child.wait()`,
    /// killing the shell immediately after construction.
    _pty: alacritty_terminal::tty::Pty,
    /// Raw file descriptor of the PTY master, used for `TIOCSWINSZ` ioctl.
    ///
    /// CRITICAL-2: The fd is captured before the reader/writer threads take ownership
    /// of `File` clones. The fd remains valid for the lifetime of `Terminal` because
    /// `_pty` owns the master `File` that backs it.
    pty_master_fd: std::os::unix::io::RawFd,
    /// In-progress OSC 7770 structured content accumulator for this terminal instance.
    ///
    /// REVIEW-2.1-D: Stored per-`Terminal` instead of a thread-local so that OSC 7770
    /// state from one pane cannot leak into another pane's accumulator.
    active_osc7770: Option<StructuredContentAccumulator>,
}

impl Terminal {
    /// Spawn a new terminal at the given grid size.
    ///
    /// `cols` and `rows` define the initial grid dimensions. `shell` optionally
    /// overrides the shell binary. `cwd` optionally sets the working directory.
    ///
    /// Returns `(terminal, image_rx)` where `image_rx` delivers decoded
    /// `PendingImage` values produced asynchronously on the tokio blocking thread pool.
    pub fn new(
        cols: usize,
        rows: usize,
        cell_width: u16,
        cell_height: u16,
        shell: Option<String>,
        cwd: Option<&std::path::Path>,
    ) -> Result<(Terminal, mpsc::Receiver<PendingImage>), std::io::Error> {
        // ── 1. Build PTY options ────────────────────────────────────────────
        let options = Options {
            shell: shell.map(|s| Shell::new(s, vec![])),
            working_directory: cwd.map(|p| p.to_path_buf()),
            drain_on_exit: false,
            env: HashMap::new(),
        };

        let window_size = WindowSize {
            num_lines: rows as u16,
            num_cols: cols as u16,
            cell_width,
            cell_height,
        };

        // ── 2. Create PTY ───────────────────────────────────────────────────
        let pty = tty::new(&options, window_size, 0)?;
        let child_pid = pty.child().id();
        // Capture the master fd before reader/writer threads take File clones.
        // The fd remains valid for the lifetime of Terminal because _pty keeps
        // the underlying File open. Used for TIOCSWINSZ in resize().
        let pty_master_fd = pty.file().as_raw_fd();

        // ── 3. Set up shared state ──────────────────────────────────────────
        let exit_code: Arc<AtomicI32> = Arc::new(AtomicI32::new(-1));
        let title: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let (wakeup_tx, wakeup_rx) = std_mpsc::channel::<()>();
        // Clone before wakeup_tx is moved into ArcTermEventListener so the reader
        // thread can send a final wakeup on EOF or error.
        let wakeup_tx_for_reader = wakeup_tx.clone();
        // Clone exit_code for the reader thread — it sets exit on EOF since we
        // bypass alacritty's EventLoop which normally fires ChildExit.
        let exit_code_for_reader = Arc::clone(&exit_code);
        // Bounded sync channel for write-back to PTY (16 slots = enough for DSR/DA bursts).
        let (write_tx, write_rx) = std_mpsc::sync_channel::<Cow<'static, [u8]>>(16);

        let listener = ArcTermEventListener {
            wakeup_tx,
            write_tx: write_tx.clone(),
            exit_code: Arc::clone(&exit_code),
            title: Arc::clone(&title),
        };

        // ── 4. Create Term ──────────────────────────────────────────────────
        let term_config = Config {
            scrolling_history: 10_000,
            ..Default::default()
        };
        let size = ArcTermSize::new(cols.max(1), rows.max(1));
        let term = Arc::new(FairMutex::new(Term::new(term_config, &size, listener)));

        // ── 5. Side channels for pre-filter output ──────────────────────────
        let (apc_tx, apc_rx) = std_mpsc::channel::<Vec<u8>>();
        // Sends `(params, captured_text)` pairs. For `"start"` params, `captured_text`
        // is empty; for `"end"` it contains the text accumulated between start and end.
        let (osc7770_tx, osc7770_rx) = std_mpsc::channel::<(String, String)>();
        let (osc133_tx, osc133_rx) = std_mpsc::channel::<Osc133Event>();

        // ── 6. Spawn PTY writer thread ──────────────────────────────────────
        //
        // Takes ownership of `pty.file()` clone for writes. We clone the file
        // handle so the reader thread can keep the original for reads.
        let pty_file_for_write = pty.file().try_clone().map_err(|e| {
            std::io::Error::new(
                e.kind(),
                format!("failed to clone PTY file for writer thread: {e}"),
            )
        })?;

        std::thread::Builder::new()
            .name("arcterm-pty-writer".to_string())
            .spawn(move || {
                let mut writer = pty_file_for_write;
                for bytes in write_rx.iter() {
                    if bytes.is_empty() {
                        continue;
                    }
                    let mut remaining = &bytes[..];
                    while !remaining.is_empty() {
                        match writer.write(remaining) {
                            Ok(0) => break,
                            Ok(n) => remaining = &remaining[n..],
                            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                // PTY not ready — drop remainder and move on.
                                break;
                            }
                            Err(e) => {
                                log::warn!("PTY writer error: {e}");
                                return;
                            }
                        }
                    }
                }
            })
            .map_err(|e| std::io::Error::new(e.kind(), format!("spawn writer thread: {e}")))?;

        // ── 7. Spawn PTY reader + pre-filter thread ─────────────────────────
        //
        // Reads raw PTY bytes, runs PreFilter, dispatches intercepted sequences
        // to side channels, and advances the vte parser with passthrough bytes.
        let term_for_reader = Arc::clone(&term);
        let pty_file_for_read = pty.file().try_clone().map_err(|e| {
            std::io::Error::new(
                e.kind(),
                format!("failed to clone PTY file for reader thread: {e}"),
            )
        })?;

        std::thread::Builder::new()
            .name("arcterm-pty-reader".to_string())
            .spawn(move || {
                let mut reader = pty_file_for_read;
                let mut prefilter = PreFilter::new();
                let mut parser = ansi::Processor::<ansi::StdSyncHandler>::new();
                let mut buf = vec![0u8; 65536];
                // OSC 7770 text capture: when we're inside a start/end block,
                // passthrough bytes are copied here to accumulate the content.
                let mut osc7770_capture: Option<Vec<u8>> = None;

                // Passthrough accumulator: collects bytes across multiple reads
                // and flushes under a single lock acquisition to reduce contention.
                let mut passthrough_acc: Vec<u8> = Vec::with_capacity(65536);
                // Flush threshold: flush when accumulated bytes exceed this or
                // when a read returns fewer bytes than the buffer (likely no more
                // data waiting in the kernel).
                const FLUSH_THRESHOLD: usize = 64 * 1024;

                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) => {
                            // PTY closed (shell exited). Set exit code so
                            // has_exited() returns true — ChildExit never
                            // fires because we bypass alacritty's EventLoop.
                            log::debug!("PTY reader: EOF");
                            // Flush any remaining accumulated passthrough.
                            if !passthrough_acc.is_empty() {
                                let mut term_guard = term_for_reader.lock();
                                parser.advance(&mut *term_guard, &passthrough_acc);
                                passthrough_acc.clear();
                            }
                            // Set exit code to 0 if not already set by ChildExit.
                            let _ = exit_code_for_reader.compare_exchange(
                                -1, 0, Ordering::AcqRel, Ordering::Relaxed,
                            );
                            let _ = wakeup_tx_for_reader.send(());
                            break;
                        }
                        Ok(n) => n,
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // Should not happen with blocking reads, but handle gracefully.
                            // Flush accumulated passthrough before yielding.
                            if !passthrough_acc.is_empty() {
                                let mut term_guard = term_for_reader.lock();
                                parser.advance(&mut *term_guard, &passthrough_acc);
                                passthrough_acc.clear();
                                let _ = wakeup_tx_for_reader.send(());
                            }
                            std::thread::yield_now();
                            continue;
                        }
                        Err(e) => {
                            log::debug!("PTY reader: {e}");
                            // Flush any remaining accumulated passthrough.
                            if !passthrough_acc.is_empty() {
                                let mut term_guard = term_for_reader.lock();
                                parser.advance(&mut *term_guard, &passthrough_acc);
                                passthrough_acc.clear();
                            }
                            let _ = wakeup_tx_for_reader.send(());
                            break;
                        }
                    };

                    let output = prefilter.advance(&buf[..n]);

                    // Dispatch intercepted sequences to side channels.
                    for payload in output.apc_payloads {
                        let _ = apc_tx.send(payload);
                    }
                    for params in output.osc7770_params {
                        // Determine if this is a start, end, or other event.
                        let is_start = params.starts_with("start");
                        let is_end = params == "end" || params.starts_with("end;");
                        if is_start {
                            // Begin capturing text written between start and end.
                            osc7770_capture = Some(Vec::new());
                            let _ = osc7770_tx.send((params, String::new()));
                        } else if is_end {
                            // End capture; send accumulated text with the end event.
                            let captured_text = osc7770_capture
                                .take()
                                .and_then(|bytes| String::from_utf8(bytes).ok())
                                .unwrap_or_default();
                            let _ = osc7770_tx.send((params, captured_text));
                        } else {
                            let _ = osc7770_tx.send((params, String::new()));
                        }
                    }
                    for event in output.osc133_events {
                        let _ = osc133_tx.send(event);
                    }

                    // Accumulate passthrough bytes and flush when:
                    // 1. Accumulated data exceeds threshold (heavy output)
                    // 2. Read returned less than buffer size (likely no more kernel data)
                    if !output.passthrough.is_empty() {
                        if let Some(ref mut cap) = osc7770_capture {
                            cap.extend_from_slice(&output.passthrough);
                        }
                        passthrough_acc.extend_from_slice(&output.passthrough);
                    }

                    let should_flush = !passthrough_acc.is_empty()
                        && (passthrough_acc.len() >= FLUSH_THRESHOLD || n < buf.len());

                    if should_flush {
                        let mut term_guard = term_for_reader.lock();
                        parser.advance(&mut *term_guard, &passthrough_acc);
                        drop(term_guard);
                        passthrough_acc.clear();
                        let _ = wakeup_tx_for_reader.send(());
                    }
                }
            })
            .map_err(|e| std::io::Error::new(e.kind(), format!("spawn reader thread: {e}")))?;

        // ── 8. Image decode channel ─────────────────────────────────────────
        let (image_tx, image_rx) = mpsc::channel(32);

        Ok((
            Terminal {
                term,
                write_tx,
                child_pid,
                cols,
                rows,
                _prefilter: std::marker::PhantomData,
                completed_blocks: Vec::new(),
                osc133_events: Vec::new(),
                chunk_assembler: KittyChunkAssembler::new(),
                image_tx,
                exit_code,
                title,
                wakeup_rx,
                apc_rx,
                osc7770_rx,
                osc133_rx,
                _pty: pty,
                pty_master_fd,
                active_osc7770: None,
            },
            image_rx,
        ))
    }

    // ── PTY I/O ──────────────────────────────────────────────────────────────

    /// Write raw input bytes to the PTY (shell stdin).
    ///
    /// Errors (e.g. broken pipe after shell exit) are logged and swallowed
    /// so the caller does not need to handle them.
    ///
    /// Uses blocking `send` instead of `try_send` so that keystrokes are never
    /// silently dropped when the channel is momentarily full (REVIEW-2.1-G).
    pub fn write_input(&self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let bytes: Cow<'static, [u8]> = Cow::Owned(data.to_vec());
        if let Err(e) = self.write_tx.send(bytes) {
            log::warn!("PTY write channel send failed (writer thread exited?): {e}");
        }
    }

    /// Resize the terminal to `(cols, rows)`.
    ///
    /// Sends a `WindowSize` to the writer thread (which will call `on_resize`
    /// on the PTY) and resizes the `Term` grid. The `cell_w` and `cell_h`
    /// parameters set the pixel dimensions used for TIOCSWINSZ.
    pub fn resize(&mut self, cols: usize, rows: usize, cell_width: u16, cell_height: u16) {
        self.cols = cols;
        self.rows = rows;
        let window_size = WindowSize {
            num_lines: rows as u16,
            num_cols: cols as u16,
            cell_width,
            cell_height,
        };
        let size = ArcTermSize::new(cols.max(1), rows.max(1));
        // Resize the Term grid.
        self.term.lock().resize(size);
        // NOTE: TIOCSWINSZ is handled by the PTY's on_resize. Since we bypassed
        // EventLoop, we need to call it directly. We stored a reference to the
        // raw PTY fd in the writer thread, but it doesn't have on_resize access.
        // We'll send a special resize message via the write channel by encoding
        // it as a side-band. For now, we use libc TIOCSWINSZ directly here via
        // the child_pid's proc/fd/0, or we accept that the PTY won't be resized
        // at the OS level until Wave 4 refines this.
        //
        // Pragmatic approach: use libc TIOCSWINSZ via /dev/fd if available.
        self.tiocswinsz(window_size);
    }

    /// Send TIOCSWINSZ to the PTY master fd and then SIGWINCH to the child process.
    ///
    /// Calls `ioctl(TIOCSWINSZ)` on the stored PTY master fd so that programs that
    /// query `ioctl(TIOCGWINSZ)` (vim, tmux, etc.) see the updated dimensions. The
    /// SIGWINCH signal is sent afterward so the shell and foreground process group
    /// receive the standard resize notification.
    fn tiocswinsz(&self, window_size: WindowSize) {
        #[cfg(unix)]
        unsafe {
            let ws = libc::winsize {
                ws_row: window_size.num_lines,
                ws_col: window_size.num_cols,
                ws_xpixel: window_size.cell_width.saturating_mul(window_size.num_cols),
                ws_ypixel: window_size.cell_height.saturating_mul(window_size.num_lines),
            };
            let ret = libc::ioctl(self.pty_master_fd, libc::TIOCSWINSZ, &ws);
            if ret != 0 {
                log::warn!(
                    "TIOCSWINSZ ioctl failed (fd={}): {}",
                    self.pty_master_fd,
                    std::io::Error::last_os_error()
                );
            }
            // Send SIGWINCH to notify the child process group of the resize.
            libc::kill(self.child_pid as i32, libc::SIGWINCH);
        }
    }

    // ── Wakeup ────────────────────────────────────────────────────────────────

    /// Drain all pending wakeup signals and return `true` if at least one was present.
    ///
    /// Also drains the pre-filter side channels (APC, OSC 7770, OSC 133) so that
    /// structured content and image data is processed during each wakeup.
    pub fn has_wakeup(&mut self) -> bool {
        let mut had_wakeup = false;

        // Drain wakeup signals.
        while self.wakeup_rx.try_recv().is_ok() {
            had_wakeup = true;
        }

        // Drain APC (Kitty) payloads.
        while let Ok(raw) = self.apc_rx.try_recv() {
            self.process_kitty_payload(raw);
        }

        // Drain OSC 7770 params (each message is (params, captured_text)).
        while let Ok((params, captured_text)) = self.osc7770_rx.try_recv() {
            dispatch_osc7770(
                &params,
                &captured_text,
                &mut self.active_osc7770,
                &mut self.completed_blocks,
            );
        }

        // Drain OSC 133 events.
        while let Ok(event) = self.osc133_rx.try_recv() {
            self.osc133_events.push(event);
        }

        had_wakeup
    }

    // ── Term access ───────────────────────────────────────────────────────────

    /// Run a closure with a locked `Term` reference.
    ///
    /// The lock is held only for the duration of the closure. Use this for
    /// all `Term` read/write access from the main thread.
    pub fn with_term<R>(&self, f: impl FnOnce(&Term<ArcTermEventListener>) -> R) -> R {
        let guard = self.term.lock();
        f(&guard)
    }

    /// Run a closure with a mutable locked `Term` reference.
    pub fn with_term_mut<R>(&self, f: impl FnOnce(&mut Term<ArcTermEventListener>) -> R) -> R {
        let mut guard = self.term.lock();
        f(&mut guard)
    }

    /// Acquire the `FairMutex` guard for direct `Term` access.
    ///
    /// Use this when you need to pass a `&Term` to a function (e.g.
    /// `snapshot_from_term`) and release the lock immediately afterwards.
    /// Prefer [`with_term`] / [`with_term_mut`] for simple read/write operations.
    ///
    /// The returned guard implements `Deref<Target = Term<...>>`.
    pub fn lock_term(&self) -> impl std::ops::Deref<Target = Term<ArcTermEventListener>> + '_ {
        self.term.lock()
    }

    // ── Grid dimensions ───────────────────────────────────────────────────────

    /// Number of grid columns.
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Number of grid rows.
    pub fn rows(&self) -> usize {
        self.rows
    }

    // ── Structured content drains ─────────────────────────────────────────────

    /// Drain and return all completed OSC 7770 structured content blocks since
    /// the last call.
    pub fn take_completed_blocks(&mut self) -> Vec<StructuredContentAccumulator> {
        std::mem::take(&mut self.completed_blocks)
    }

    /// Drain and return all shell exit codes received via OSC 133 D since the
    /// last call.
    pub fn take_exit_codes(&mut self) -> Vec<i32> {
        let codes: Vec<i32> = self
            .osc133_events
            .drain(..)
            .filter_map(|ev| {
                if let Osc133Event::CommandFinished(Some(code)) = ev {
                    Some(code)
                } else {
                    None
                }
            })
            .collect();
        codes
    }

    /// Drain and return all pending MCP tool-list queries.
    pub fn take_tool_queries(&mut self) -> Vec<()> {
        Vec::new() // Populated by dispatch_osc7770 in a future pass
    }

    /// Drain and return all pending MCP tool calls as `(name, args_json)` pairs.
    pub fn take_tool_calls(&mut self) -> Vec<(String, String)> {
        Vec::new() // Populated by dispatch_osc7770 in a future pass
    }

    /// Drain and return all pending cross-pane context queries.
    pub fn take_context_queries(&mut self) -> Vec<()> {
        Vec::new() // Populated by dispatch_osc7770 in a future pass
    }

    // ── Process info ──────────────────────────────────────────────────────────

    /// Returns the PID of the child shell process.
    pub fn child_pid(&self) -> Option<u32> {
        Some(self.child_pid)
    }

    /// Returns the current working directory of the child shell process.
    ///
    /// Uses `/proc/{pid}/cwd` on Linux, `lsof` or `proc_pidinfo` on macOS.
    /// Returns `None` if the process has exited or CWD cannot be determined.
    pub fn cwd(&self) -> Option<PathBuf> {
        cwd_for_pid(self.child_pid)
    }

    /// Returns the current window title, if one has been set via OSC 0/1/2.
    pub fn title(&self) -> Option<String> {
        self.title.lock().ok()?.clone()
    }

    /// Returns `true` if the child process has exited.
    pub fn has_exited(&self) -> bool {
        self.exit_code.load(Ordering::Acquire) != -1
    }

    /// Returns the current cursor row in the viewport (0-based).
    pub fn cursor_row(&self) -> usize {
        self.with_term(|t| {
            let content = t.renderable_content();
            content.cursor.point.line.0.max(0) as usize
        })
    }

    /// Extract all visible rows of text from the terminal grid.
    ///
    /// Returns a `Vec<String>` of length `self.rows`, one per visible line.
    /// Each string has trailing spaces stripped.
    pub fn all_text_rows(&self) -> Vec<String> {
        use alacritty_terminal::grid::Dimensions;
        self.with_term(|t| {
            let cols = t.columns();
            let rows = t.screen_lines();
            let mut result = vec![String::new(); rows];

            // Build a 2D buffer of chars from the display iterator.
            let mut grid_chars: Vec<Vec<char>> = vec![vec![' '; cols]; rows];
            let content = t.renderable_content();
            for indexed in content.display_iter {
                let row = indexed.point.line.0;
                let col = indexed.point.column.0;
                if row >= 0 && (row as usize) < rows && col < cols {
                    grid_chars[row as usize][col] = indexed.c;
                }
            }

            for (r, chars) in grid_chars.iter().enumerate() {
                let s: String = chars.iter().collect();
                result[r] = s.trim_end().to_string();
            }
            result
        })
    }

    /// Returns the current scroll offset (0 = no scroll, positive = scrolled back).
    pub fn scroll_offset(&self) -> usize {
        self.with_term(|t| {
            let content = t.renderable_content();
            content.display_offset
        })
    }

    /// Set the viewport scroll offset.
    ///
    /// Positive values scroll backward (into scrollback). 0 = live screen.
    pub fn set_scroll_offset(&mut self, offset: usize) {
        use alacritty_terminal::grid::Scroll;
        let current = self.scroll_offset();
        let delta = offset as i32 - current as i32;
        if delta != 0 {
            self.with_term_mut(|t| {
                t.scroll_display(Scroll::Delta(-delta));
            });
        }
    }

    /// Returns `true` if the terminal is in bracketed paste mode.
    pub fn bracketed_paste(&self) -> bool {
        use alacritty_terminal::term::TermMode;
        self.with_term(|t| t.mode().contains(TermMode::BRACKETED_PASTE))
    }

    /// Returns `true` if the terminal has application cursor keys mode active.
    pub fn app_cursor_keys(&self) -> bool {
        use alacritty_terminal::term::TermMode;
        self.with_term(|t| t.mode().contains(TermMode::APP_CURSOR))
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    /// Process a raw Kitty APC payload: parse, chunk-assemble, and decode.
    fn process_kitty_payload(&mut self, raw: Vec<u8>) {
        if let Some(cmd) = parse_kitty_command(&raw)
            && let Some((meta, decoded_bytes)) = self.chunk_assembler.receive_chunk(&cmd)
        {
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

// ---------------------------------------------------------------------------
// OSC 7770 dispatcher
// ---------------------------------------------------------------------------

/// Parse an OSC 7770 parameter string and accumulate it into `completed_blocks`.
///
/// The `params` string is the part after `7770;`, e.g.:
/// - `"start;type=code;lang=rust"` → opens a new accumulator
/// - `"end"` → closes the active accumulator
/// - Other variants (tools/list, tools/call, context/query) are not yet wired;
///   they are handled by the existing MCP pipeline in Wave 4.
///
/// `active` is per-`Terminal` instance state so that OSC 7770 accumulators
/// cannot leak between panes (REVIEW-2.1-D).
fn dispatch_osc7770(
    params: &str,
    captured_text: &str,
    active: &mut Option<StructuredContentAccumulator>,
    completed_blocks: &mut Vec<StructuredContentAccumulator>,
) {
    let parts: Vec<&str> = params.splitn(3, ';').collect();
    match parts.first().copied().unwrap_or("") {
        "start" => {
            // Parse key=value pairs.
            let mut attrs: HashMap<String, String> = HashMap::new();
            let mut content_type = ContentType::CodeBlock;
            for part in &parts[1..] {
                if let Some((k, v)) = part.split_once('=') {
                    match k {
                        "type" => {
                            content_type = match v {
                                "code" => ContentType::CodeBlock,
                                "diff" => ContentType::Diff,
                                "plan" => ContentType::Plan,
                                "markdown" => ContentType::Markdown,
                                "json" => ContentType::Json,
                                "error" => ContentType::Error,
                                "progress" => ContentType::Progress,
                                "image" => ContentType::Image,
                                _ => ContentType::CodeBlock,
                            };
                        }
                        _ => {
                            attrs.insert(k.to_string(), v.to_string());
                        }
                    }
                }
            }
            *active = Some(StructuredContentAccumulator::new(content_type, attrs));
        }
        "end" => {
            if let Some(mut acc) = active.take() {
                // Store the text captured between start and end by the reader thread.
                // The captured_text is the raw terminal output (may include ANSI codes).
                // Strip ANSI escape sequences so the buffer contains only text.
                acc.buffer = strip_ansi(captured_text);
                completed_blocks.push(acc);
            }
        }
        _ => {
            // Other variants (tools/list, tools/call, context/query) — not yet wired here.
            log::debug!("OSC 7770 unhandled: {params}");
        }
    }
}

/// Strip ANSI escape sequences from a string, leaving only printable text.
///
/// This is a minimal implementation that removes CSI and OSC sequences.
/// Used to clean the raw terminal output captured between OSC 7770 start/end markers.
///
/// The implementation operates on the byte level for ANSI detection but appends
/// validated UTF-8 characters to the output, so multi-byte codepoints are preserved
/// correctly (REVIEW-2.1-E).
fn strip_ansi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i >= bytes.len() {
                break;
            }
            match bytes[i] {
                b'[' => {
                    // CSI sequence: ESC [ ... letter
                    i += 1;
                    while i < bytes.len() && !(bytes[i].is_ascii_alphabetic() || bytes[i] == b'~') {
                        i += 1;
                    }
                    i += 1; // skip the terminating letter
                }
                b']' => {
                    // OSC sequence: ESC ] ... BEL or ESC \
                    i += 1;
                    while i < bytes.len() {
                        if bytes[i] == 0x07 {
                            i += 1;
                            break;
                        }
                        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    // Other ESC sequences: skip one char.
                    i += 1;
                }
            }
        } else if bytes[i] < 0x20 && bytes[i] != b'\n' && bytes[i] != b'\t' {
            // Skip other control characters (except newline and tab).
            i += 1;
        } else {
            // Decode one UTF-8 character starting at `i` and append it.
            // This correctly handles multi-byte sequences; `s` is already valid UTF-8
            // so `std::str::from_utf8` on the remaining slice will succeed.
            let remaining = &s[i..];
            let ch = remaining.chars().next().unwrap_or('\0');
            if ch != '\0' {
                result.push(ch);
                i += ch.len_utf8();
            } else {
                i += 1;
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// CWD helper
// ---------------------------------------------------------------------------

/// Determine the current working directory of a process by PID.
///
/// On Linux, reads `/proc/{pid}/cwd`. On macOS, uses `proc_pidinfo`.
/// Returns `None` if unavailable.
fn cwd_for_pid(pid: u32) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let link = format!("/proc/{pid}/cwd");
        std::fs::read_link(&link).ok()
    }
    #[cfg(target_os = "macos")]
    {
        // Use PROC_PIDVNODEPATHINFO via libproc.
        #[allow(unused_imports)]
        unsafe extern "C" {
            fn proc_pidinfo(
                pid: i32,
                flavor: i32,
                arg: u64,
                buffer: *mut libc::c_void,
                buffersize: i32,
            ) -> i32;
        }
        // PROC_PIDVNODEPATHINFO = 9
        const PROC_PIDVNODEPATHINFO: i32 = 9;
        // struct proc_vnodepathinfo has two vnode_info_path fields.
        // vnode_info_path = vnode_info (64 bytes) + char path[1024] = 1088 bytes.
        // cwd path starts at offset 1088.
        const VNODE_INFO_PATH_SIZE: usize = 1088; // 64 (vnode_info) + 1024 (MAXPATHLEN)
        const BUF_SIZE: usize = VNODE_INFO_PATH_SIZE * 2;

        let mut buf = [0u8; BUF_SIZE];
        let ret = unsafe {
            proc_pidinfo(
                pid as i32,
                PROC_PIDVNODEPATHINFO,
                0,
                buf.as_mut_ptr() as *mut libc::c_void,
                BUF_SIZE as i32,
            )
        };
        if ret <= 0 {
            return None;
        }
        // CWD path is in the second vnode_info_path, at offset VNODE_INFO_PATH_SIZE + 64.
        let cwd_offset = VNODE_INFO_PATH_SIZE + 64; // skip vnode_info header in second vip
        if cwd_offset >= BUF_SIZE {
            return None;
        }
        let cwd_bytes = &buf[cwd_offset..];
        let nul = cwd_bytes.iter().position(|&b| b == 0).unwrap_or(cwd_bytes.len());
        let cwd_str = std::str::from_utf8(&cwd_bytes[..nul]).ok()?;
        if cwd_str.is_empty() {
            return None;
        }
        Some(PathBuf::from(cwd_str))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        None
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

            // Send through the channel exactly as process_kitty_payload would.
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

    /// Test that dispatch_osc7770 accumulates a start/end pair correctly.
    #[test]
    fn osc7770_dispatch_start_end() {
        use super::dispatch_osc7770;

        let mut active = None;
        let mut completed = Vec::new();
        dispatch_osc7770("start;type=code;lang=rust", "", &mut active, &mut completed);
        // No completed blocks yet — accumulator is open.
        assert!(completed.is_empty());
        // Active accumulator should be set.
        assert!(active.is_some());

        dispatch_osc7770("end", "fn hello() {}", &mut active, &mut completed);
        // Now we should have one completed block and no active accumulator.
        assert_eq!(completed.len(), 1);
        assert!(active.is_none());
        let block = &completed[0];
        assert_eq!(
            block.content_type,
            arcterm_render::ContentType::CodeBlock,
            "expected CodeBlock"
        );
        assert_eq!(block.buffer, "fn hello() {}", "buffer should contain captured text");
    }
}
