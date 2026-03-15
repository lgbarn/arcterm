//! VT byte-stream processor — bridges vte::Parser to the Handler trait.

use crate::Handler;

// ---------------------------------------------------------------------------
// ApcScanner — filters APC sequences before the vte::Parser sees them
// ---------------------------------------------------------------------------

/// State machine states for scanning APC (Application Program Command) sequences.
///
/// APC sequences have the form:  ESC _ <payload> ESC \
///
/// The `vte` crate's `hook`/`put`/`unhook` callbacks handle DCS (ESC P) but
/// not APC (ESC _).  ApcScanner sits in front of `Processor` and intercepts
/// APC sequences at the byte level before passing remaining bytes to the inner
/// `Processor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApcState {
    /// Normal passthrough — not inside an APC sequence.
    Normal,
    /// Just saw a bare ESC byte; waiting to see whether it begins APC (`_`) or
    /// something else.
    PendingEsc,
    /// Inside an APC sequence, collecting payload bytes.
    InApc,
    /// Inside an APC sequence and just saw an ESC; waiting for `\` (ST) or
    /// another APC-body byte.
    InApcPendingEsc,
}

/// Wraps a `Processor` and intercepts Kitty Graphics Protocol APC sequences
/// (`ESC _ … ESC \`) before forwarding remaining bytes to the inner parser.
///
/// Non-APC bytes are passed through to the inner `Processor` unchanged,
/// preserving full VT handling.  For performance, runs of non-ESC bytes are
/// batched into a single `inner.advance` call rather than processed one-by-one.
pub struct ApcScanner {
    inner: Processor,
    state: ApcState,
    /// Payload buffer — accumulates APC body bytes between ESC _ and ESC \.
    payload: Vec<u8>,
}

impl ApcScanner {
    /// Create a new `ApcScanner` wrapping a fresh `Processor`.
    pub fn new() -> Self {
        Self {
            inner: Processor::new(),
            state: ApcState::Normal,
            payload: Vec::new(),
        }
    }

    /// Feed raw PTY bytes into the scanner.
    ///
    /// APC sequences are extracted and dispatched via
    /// `Handler::kitty_graphics_command`.  All other bytes are forwarded to
    /// the inner `Processor`.
    pub fn advance<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) {
        // Index into `bytes` marking the start of the current passthrough run.
        let mut pass_start: Option<usize> = None;

        // Flush any accumulated passthrough bytes to the inner Processor.
        let flush_pass = |inner: &mut Processor, handler: &mut H, end: usize, start: &mut Option<usize>| {
            if let Some(s) = start.take() {
                inner.advance(handler, &bytes[s..end]);
            }
        };

        let mut i = 0;
        while i < bytes.len() {
            let byte = bytes[i];

            match self.state {
                // -----------------------------------------------------------------
                ApcState::Normal => {
                    if byte == 0x1b {
                        // Flush any accumulated passthrough bytes up to (not including) ESC.
                        flush_pass(&mut self.inner, handler, i, &mut pass_start);
                        self.state = ApcState::PendingEsc;
                    } else {
                        // Accumulate into passthrough run.
                        if pass_start.is_none() {
                            pass_start = Some(i);
                        }
                    }
                }

                // -----------------------------------------------------------------
                ApcState::PendingEsc => {
                    if byte == b'_' {
                        // ESC _ → start of APC sequence.
                        self.payload.clear();
                        self.state = ApcState::InApc;
                    } else {
                        // ESC followed by something other than '_' — forward both
                        // bytes to the inner Processor as a combined slice.
                        let esc_and_byte = [0x1b, byte];
                        self.inner.advance(handler, &esc_and_byte);
                        self.state = ApcState::Normal;
                    }
                }

                // -----------------------------------------------------------------
                ApcState::InApc => {
                    if byte == 0x1b {
                        self.state = ApcState::InApcPendingEsc;
                    } else {
                        self.payload.push(byte);
                    }
                }

                // -----------------------------------------------------------------
                ApcState::InApcPendingEsc => {
                    if byte == b'\\' {
                        // ESC \ (ST) — end of APC sequence.  Dispatch the payload.
                        handler.kitty_graphics_command(&self.payload);
                        self.payload.clear();
                        self.state = ApcState::Normal;
                    } else {
                        // ESC inside APC body followed by non-'\' — include the
                        // ESC and this byte as payload bytes and stay in InApc.
                        self.payload.push(0x1b);
                        self.payload.push(byte);
                        self.state = ApcState::InApc;
                    }
                }
            }

            i += 1;
        }

        // Flush any remaining passthrough bytes.
        let len = bytes.len();
        flush_pass(&mut self.inner, handler, len, &mut pass_start);
    }
}

impl Default for ApcScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Wraps `vte::Parser` and drives a `Handler` implementation from raw PTY bytes.
pub struct Processor {
    parser: vte::Parser,
}

impl Processor {
    pub fn new() -> Self {
        Self {
            parser: vte::Parser::new(),
        }
    }

    /// Feed raw PTY bytes into the parser, dispatching semantic operations to
    /// `handler`.
    pub fn advance<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) {
        let mut performer = Performer { handler };
        self.parser.advance(&mut performer, bytes);
    }
}

impl Default for Processor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Internal performer — holds &mut H and implements vte::Perform
// ---------------------------------------------------------------------------

struct Performer<'a, H: Handler> {
    handler: &'a mut H,
}

impl<H: Handler> vte::Perform for Performer<'_, H> {
    // Print a single Unicode character.
    fn print(&mut self, c: char) {
        self.handler.put_char(c);
    }

    // C0/C1 control codes.
    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => self.handler.bell(),
            0x08 => self.handler.backspace(),
            0x09 => self.handler.tab(),
            0x0A => self.handler.line_feed(),
            0x0D => self.handler.carriage_return(),
            _ => {} // other control codes ignored
        }
    }

    // CSI sequences.
    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if ignore {
            return;
        }

        // Collect the flat parameter list.  Each item yielded by params.iter()
        // is a &[u16] where index 0 is the primary param and the rest are
        // colon-separated sub-params (used by SGR extended colors).
        // For most CSI sequences we only need the first sub-param of each param
        // (i.e. item[0]).  For SGR we need them all flattened.
        let raw: Vec<&[u16]> = params.iter().collect();

        // Detect whether this is a DEC private mode sequence (ESC[?...h/l).
        let private = intermediates.contains(&0x3F); // '?'

        match action {
            // CUU — cursor up
            'A' => {
                let n = raw.first().and_then(|p| p.first()).copied().unwrap_or(1);
                self.handler.cursor_up(n.max(1) as usize);
            }
            // CUD — cursor down
            'B' => {
                let n = raw.first().and_then(|p| p.first()).copied().unwrap_or(1);
                self.handler.cursor_down(n.max(1) as usize);
            }
            // CUF — cursor forward
            'C' => {
                let n = raw.first().and_then(|p| p.first()).copied().unwrap_or(1);
                self.handler.cursor_forward(n.max(1) as usize);
            }
            // CUB — cursor backward
            'D' => {
                let n = raw.first().and_then(|p| p.first()).copied().unwrap_or(1);
                self.handler.cursor_backward(n.max(1) as usize);
            }
            // CUP / HVP — cursor position (1-based → 0-based)
            'H' | 'f' => {
                let row = raw
                    .first()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1)
                    .max(1) as usize
                    - 1;
                let col = raw
                    .get(1)
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1)
                    .max(1) as usize
                    - 1;
                self.handler.set_cursor_pos(row, col);
            }
            // ED — erase in display
            'J' => {
                let mode = raw.first().and_then(|p| p.first()).copied().unwrap_or(0);
                self.handler.erase_in_display(mode);
            }
            // EL — erase in line
            'K' => {
                let mode = raw.first().and_then(|p| p.first()).copied().unwrap_or(0);
                self.handler.erase_in_line(mode);
            }
            // IL — insert lines
            'L' => {
                let n = raw.first().and_then(|p| p.first()).copied().unwrap_or(1);
                self.handler.insert_lines(n.max(1) as usize);
            }
            // DL — delete lines
            'M' => {
                let n = raw.first().and_then(|p| p.first()).copied().unwrap_or(1);
                self.handler.delete_lines(n.max(1) as usize);
            }
            // SGR — select graphic rendition
            'm' => {
                // Flatten params+subparams into a single &[u16] slice.
                let flat: Vec<u16> = raw.iter().flat_map(|sub| sub.iter().copied()).collect();
                if flat.is_empty() {
                    self.handler.set_sgr(&[0]);
                } else {
                    self.handler.set_sgr(&flat);
                }
            }
            // DSR — device status report
            'n' => {
                let n = raw.first().and_then(|p| p.first()).copied().unwrap_or(0);
                self.handler.device_status_report(n);
            }
            // DA — device attributes (primary)
            'c' => {
                self.handler.device_attributes();
            }
            // SU — scroll up
            'S' => {
                let n = raw.first().and_then(|p| p.first()).copied().unwrap_or(1);
                self.handler.scroll_up(n.max(1) as usize);
            }
            // SD — scroll down
            'T' => {
                let n = raw.first().and_then(|p| p.first()).copied().unwrap_or(1);
                self.handler.scroll_down(n.max(1) as usize);
            }
            // ICH — insert character (blank spaces at cursor)
            '@' => {
                let n = raw.first().and_then(|p| p.first()).copied().unwrap_or(1);
                self.handler.insert_chars(n.max(1) as usize);
            }
            // CHA — cursor horizontal absolute (1-based)
            'G' => {
                let col = raw
                    .first()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1)
                    .max(1) as usize
                    - 1;
                self.handler.cursor_horizontal_absolute(col);
            }
            // DCH — delete character
            'P' => {
                let n = raw.first().and_then(|p| p.first()).copied().unwrap_or(1);
                self.handler.delete_chars(n.max(1) as usize);
            }
            // ECH — erase characters
            'X' => {
                let n = raw.first().and_then(|p| p.first()).copied().unwrap_or(1);
                self.handler.erase_chars(n.max(1) as usize);
            }
            // VPA — cursor vertical absolute (1-based)
            'd' => {
                let row = raw
                    .first()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1)
                    .max(1) as usize
                    - 1;
                self.handler.cursor_vertical_absolute(row);
            }
            // SM / RM — set/reset mode
            'h' => {
                for param_group in &raw {
                    let mode = param_group.first().copied().unwrap_or(0);
                    self.handler.set_mode(mode, private);
                }
            }
            'l' => {
                for param_group in &raw {
                    let mode = param_group.first().copied().unwrap_or(0);
                    self.handler.reset_mode(mode, private);
                }
            }
            // DECSTBM — set top and bottom margins (scroll region), 1-based
            'r' => {
                let top = raw
                    .first()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1)
                    .max(1) as usize
                    - 1;
                // Default bottom is the last row — but we don't know grid size
                // here; use a sentinel of u16::MAX and let the handler clamp.
                let bottom_raw = raw
                    .get(1)
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(0);
                // bottom=0 means "default" (full screen) — pass usize::MAX.
                let bottom = if bottom_raw == 0 {
                    usize::MAX
                } else {
                    (bottom_raw as usize).saturating_sub(1)
                };
                self.handler.set_scroll_region(top, bottom);
            }
            _ => {} // unhandled CSI sequence — silently ignored
        }
    }

    // OSC sequences (Operating System Command).
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // params[0] is the numeric command identifier, params[1] is the value.
        if params.len() < 2 {
            return;
        }
        match params[0] {
            b"0" | b"2" => {
                let title = std::str::from_utf8(params[1]).unwrap_or("");
                self.handler.set_title(title);
            }
            _ => {}
        }
    }

    // Remaining callbacks — no-op for Phase 1.
    fn hook(
        &mut self,
        _params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
    }
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}

    // ESC dispatch (2-byte escape sequences).
    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        // Only handle bare ESC sequences (no intermediate bytes).  Sequences
        // with intermediates — e.g. ESC ( 7 (SCS Select Character Set) — use
        // the same final byte values but have completely different semantics.
        // Dispatching on byte alone would cause silent mis-dispatch, e.g.
        // ESC ( 7 incorrectly firing save_cursor_position.
        if !intermediates.is_empty() {
            return;
        }
        match byte {
            // DECSC — save cursor position
            0x37 => self.handler.save_cursor_position(),
            // DECRC — restore cursor position
            0x38 => self.handler.restore_cursor_position(),
            // DECKPAM — set keypad application mode
            0x3D => self.handler.set_keypad_application_mode(),
            // DECKPNM — set keypad numeric mode
            0x3E => self.handler.set_keypad_numeric_mode(),
            _ => {}
        }
    }
}
