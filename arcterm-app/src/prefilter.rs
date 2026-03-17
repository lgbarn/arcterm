//! Pre-filter byte-stream scanner for arcterm.
//!
//! `PreFilter` sits in front of the alacritty `EventLoop` and intercepts
//! escape sequences that alacritty would silently drop:
//!
//! - **APC** (`ESC _ <payload> ESC \`) — Kitty Graphics Protocol
//! - **OSC 7770** (`ESC ] 7770 ; <params> BEL|ST`) — arcterm custom sequences
//! - **OSC 133** (`ESC ] 133 ; <type> BEL|ST`) — shell integration marks
//!
//! All other bytes, including non-intercepted OSC sequences such as OSC 0
//! (window title), are emitted unchanged into `PreFilterOutput::passthrough`.
//!
//! The state machine is fully stateful so it handles sequences that span
//! multiple `advance` calls (e.g. split reads from a PTY).

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Shell-integration event decoded from an OSC 133 sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Osc133Event {
    /// OSC 133 ; A — prompt start.
    PromptStart,
    /// OSC 133 ; B — command start (user begins typing).
    CommandStart,
    /// OSC 133 ; C — command executed (Enter pressed, command now running).
    CommandExecuted,
    /// OSC 133 ; D [; exit-code] — command finished.
    ///
    /// The exit code is `Some(n)` when the sequence includes it, `None`
    /// when omitted.
    CommandFinished(Option<i32>),
}

/// Output produced by a single `PreFilter::advance` call.
///
/// Multiple intercepted sequences may be emitted in one call when the input
/// buffer contains more than one complete sequence.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreFilterOutput {
    /// Bytes that should be forwarded to the terminal engine unchanged.
    pub passthrough: Vec<u8>,
    /// Completed APC payloads (the bytes between `ESC _` and `ESC \`).
    pub apc_payloads: Vec<Vec<u8>>,
    /// Completed OSC 7770 parameter strings (UTF-8; the part after `7770;`).
    pub osc7770_params: Vec<String>,
    /// Completed OSC 133 events.
    pub osc133_events: Vec<Osc133Event>,
}

impl PreFilterOutput {
    fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Internal state-machine states
// ---------------------------------------------------------------------------

/// The set of states the `PreFilter` state machine can occupy.
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    /// Normal passthrough — not inside any escape sequence.
    Normal,
    /// Just saw `ESC` (0x1b); waiting for the introducer byte.
    PendingEsc,

    // --- APC ---
    /// Inside an APC sequence (`ESC _`); collecting payload bytes.
    InApc,
    /// Inside an APC sequence and just saw `ESC`; waiting for `\` (ST).
    InApcPendingEsc,

    // --- OSC ---
    /// Saw `ESC ]`; collecting the raw OSC parameter string.
    ///
    /// We buffer everything until a terminator (BEL or ST) so we can route
    /// the sequence to the correct handler.
    InOsc,
    /// Inside an OSC sequence and just saw `ESC`; waiting for `\` (ST).
    InOscPendingEsc,
}

// ---------------------------------------------------------------------------
// PreFilter
// ---------------------------------------------------------------------------

/// Stateful byte-stream pre-filter.
///
/// Feed raw PTY bytes via `advance`; the returned `PreFilterOutput` separates
/// intercepted sequences from bytes that should be forwarded to the terminal.
#[derive(Debug, Clone)]
pub struct PreFilter {
    state: State,
    /// Accumulation buffer shared between APC and OSC collection states.
    buf: Vec<u8>,
    /// The terminator byte seen at the end of the current OSC sequence.
    ///
    /// Set to `0x07` (BEL) or `b'\\'` (the `\` byte of ST `ESC \`) when an
    /// OSC completes, so `reconstruct_osc_passthrough` can replay the original
    /// terminator instead of always emitting BEL.
    osc_terminator: u8,
}

impl PreFilter {
    /// Create a new, idle `PreFilter`.
    pub fn new() -> Self {
        Self {
            state: State::Normal,
            buf: Vec::new(),
            osc_terminator: 0x07,
        }
    }

    /// Process `input` bytes and return classified output.
    ///
    /// The state machine retains its current state across calls, so partial
    /// sequences that are split across read boundaries are handled correctly.
    pub fn advance(&mut self, input: &[u8]) -> PreFilterOutput {
        let mut out = PreFilterOutput::new();

        for &byte in input {
            match &self.state {
                // -----------------------------------------------------------------
                State::Normal => {
                    if byte == 0x1b {
                        self.state = State::PendingEsc;
                    } else {
                        out.passthrough.push(byte);
                    }
                }

                // -----------------------------------------------------------------
                State::PendingEsc => {
                    match byte {
                        b'_' => {
                            // ESC _ → APC start
                            self.buf.clear();
                            self.state = State::InApc;
                        }
                        b']' => {
                            // ESC ] → OSC start
                            self.buf.clear();
                            self.state = State::InOsc;
                        }
                        _ => {
                            // ESC followed by something else — pass both bytes through.
                            out.passthrough.push(0x1b);
                            out.passthrough.push(byte);
                            self.state = State::Normal;
                        }
                    }
                }

                // -----------------------------------------------------------------
                State::InApc => {
                    if byte == 0x1b {
                        self.state = State::InApcPendingEsc;
                    } else {
                        self.buf.push(byte);
                    }
                }

                // -----------------------------------------------------------------
                State::InApcPendingEsc => {
                    if byte == b'\\' {
                        // ESC \ (ST) — end of APC.
                        out.apc_payloads.push(self.buf.clone());
                        self.buf.clear();
                        self.state = State::Normal;
                    } else {
                        // ESC followed by non-'\' inside APC body — include both
                        // bytes as payload and stay in APC.
                        self.buf.push(0x1b);
                        self.buf.push(byte);
                        self.state = State::InApc;
                    }
                }

                // -----------------------------------------------------------------
                State::InOsc => {
                    match byte {
                        0x07 => {
                            // BEL terminator
                            self.osc_terminator = 0x07;
                            self.dispatch_osc(&mut out);
                            self.state = State::Normal;
                        }
                        0x1b => {
                            self.state = State::InOscPendingEsc;
                        }
                        _ => {
                            self.buf.push(byte);
                        }
                    }
                }

                // -----------------------------------------------------------------
                State::InOscPendingEsc => {
                    if byte == b'\\' {
                        // ST terminator (ESC \)
                        self.osc_terminator = b'\\';
                        self.dispatch_osc(&mut out);
                        self.state = State::Normal;
                    } else {
                        // ESC followed by non-'\' inside OSC body — include both.
                        self.buf.push(0x1b);
                        self.buf.push(byte);
                        self.state = State::InOsc;
                    }
                }
            }
        }

        out
    }

    /// Dispatch the buffered OSC sequence to the correct output bucket.
    ///
    /// Sequences that are not intercepted (i.e. not OSC 7770 or OSC 133) are
    /// reconstructed and appended to `passthrough` so the terminal engine can
    /// handle them normally.
    fn dispatch_osc(&mut self, out: &mut PreFilterOutput) {
        // The buffer contains everything after `ESC ]` and before the terminator.
        // Split on `;` to get the numeric prefix.
        let raw = self.buf.as_slice();

        // Find the first `;` to extract the OSC number.
        let semi_pos = raw.iter().position(|&b| b == b';');

        let osc_num = match semi_pos {
            Some(pos) => &raw[..pos],
            None => raw, // no `;` — treat entire buffer as the number
        };

        match osc_num {
            b"7770" => {
                // OSC 7770: emit the part after `7770;` as a UTF-8 string.
                let params_bytes = match semi_pos {
                    Some(pos) => &raw[pos + 1..],
                    None => b"",
                };
                let params = String::from_utf8_lossy(params_bytes).into_owned();
                out.osc7770_params.push(params);
            }

            b"133" => {
                // OSC 133: parse shell-integration event.
                if let Some(event) = parse_osc133(raw) {
                    out.osc133_events.push(event);
                } else {
                    // Malformed 133 — pass through.
                    self.reconstruct_osc_passthrough(out);
                }
            }

            _ => {
                // All other OSC sequences: pass through unchanged so the
                // terminal engine handles them (e.g. OSC 0 window title).
                self.reconstruct_osc_passthrough(out);
            }
        }

        self.buf.clear();
    }

    /// Reconstruct a non-intercepted OSC sequence and push it to passthrough.
    ///
    /// The original terminator is replayed: BEL (`0x07`) stays as BEL; ST
    /// (`ESC \`) is re-emitted as the two-byte sequence `0x1b 0x5c`.  This
    /// preserves the wire format so that tmux passthrough and other
    /// terminator-sensitive consumers receive exactly what the application sent.
    fn reconstruct_osc_passthrough(&self, out: &mut PreFilterOutput) {
        // ESC ]
        out.passthrough.push(0x1b);
        out.passthrough.push(b']');
        out.passthrough.extend_from_slice(&self.buf);
        // Replay original terminator.
        if self.osc_terminator == 0x07 {
            out.passthrough.push(0x07);
        } else {
            // ST: ESC \
            out.passthrough.push(0x1b);
            out.passthrough.push(b'\\');
        }
    }
}

impl Default for PreFilter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// OSC 133 parser helper
// ---------------------------------------------------------------------------

/// Parse the raw OSC 133 parameter buffer into an `Osc133Event`.
///
/// `raw` is the content between `ESC ]` and the terminator (BEL or ST).
/// Layout: `133;A`, `133;B`, `133;C`, or `133;D` (with optional `;exit-code`).
fn parse_osc133(raw: &[u8]) -> Option<Osc133Event> {
    // Split on `;`.
    let parts: Vec<&[u8]> = raw.splitn(3, |&b| b == b';').collect();

    // parts[0] must be "133", parts[1] is the sub-command letter.
    if parts.len() < 2 {
        return None;
    }

    match parts[1] {
        b"A" => Some(Osc133Event::PromptStart),
        b"B" => Some(Osc133Event::CommandStart),
        b"C" => Some(Osc133Event::CommandExecuted),
        b"D" => {
            // Optional exit code in parts[2].
            let code = parts.get(2).and_then(|s| {
                let s = std::str::from_utf8(s).ok()?;
                s.trim().parse::<i32>().ok()
            });
            Some(Osc133Event::CommandFinished(code))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: feed all bytes in one call.
    fn run(input: &[u8]) -> PreFilterOutput {
        PreFilter::new().advance(input)
    }

    // Helper: feed bytes in two separate calls; return the merged output.
    fn run_split(first: &[u8], second: &[u8]) -> PreFilterOutput {
        let mut pf = PreFilter::new();
        let mut a = pf.advance(first);
        let b = pf.advance(second);
        a.passthrough.extend_from_slice(&b.passthrough);
        a.apc_payloads.extend(b.apc_payloads);
        a.osc7770_params.extend(b.osc7770_params);
        a.osc133_events.extend(b.osc133_events);
        a
    }

    // -------------------------------------------------------------------------
    // 1. Passthrough
    // -------------------------------------------------------------------------

    #[test]
    fn test_plain_ascii_passthrough() {
        let out = run(b"hello world");
        assert_eq!(out.passthrough, b"hello world");
        assert!(out.apc_payloads.is_empty());
        assert!(out.osc7770_params.is_empty());
        assert!(out.osc133_events.is_empty());
    }

    // -------------------------------------------------------------------------
    // 2. APC complete
    // -------------------------------------------------------------------------

    #[test]
    fn test_apc_complete() {
        // ESC _ payload ESC \
        let input = b"\x1b_PAYLOAD\x1b\\";
        let out = run(input);
        assert_eq!(out.apc_payloads, vec![b"PAYLOAD".to_vec()]);
        assert!(out.passthrough.is_empty());
    }

    // -------------------------------------------------------------------------
    // 3. APC split across calls
    // -------------------------------------------------------------------------

    #[test]
    fn test_apc_split() {
        let out = run_split(b"\x1b_PAY", b"LOAD\x1b\\");
        assert_eq!(out.apc_payloads, vec![b"PAYLOAD".to_vec()]);
        assert!(out.passthrough.is_empty());
    }

    // -------------------------------------------------------------------------
    // 4. OSC 7770 complete — BEL terminated
    // -------------------------------------------------------------------------

    #[test]
    fn test_osc7770_bel() {
        // ESC ] 7770 ; type=code ; lang=rs BEL
        let input = b"\x1b]7770;type=code;lang=rs\x07";
        let out = run(input);
        assert_eq!(out.osc7770_params, vec!["type=code;lang=rs".to_string()]);
        assert!(out.passthrough.is_empty());
    }

    // -------------------------------------------------------------------------
    // 5. OSC 7770 complete — ST terminated
    // -------------------------------------------------------------------------

    #[test]
    fn test_osc7770_st() {
        // ESC ] 7770 ; start ; type=code ESC \
        let input = b"\x1b]7770;start;type=code\x1b\\";
        let out = run(input);
        assert_eq!(out.osc7770_params, vec!["start;type=code".to_string()]);
        assert!(out.passthrough.is_empty());
    }

    // -------------------------------------------------------------------------
    // 6. OSC 7770 split across calls
    // -------------------------------------------------------------------------

    #[test]
    fn test_osc7770_split() {
        let out = run_split(b"\x1b]7770;star", b"t;type=code\x07");
        assert_eq!(out.osc7770_params, vec!["start;type=code".to_string()]);
        assert!(out.passthrough.is_empty());
    }

    // -------------------------------------------------------------------------
    // 7. OSC 133 D with exit code
    // -------------------------------------------------------------------------

    #[test]
    fn test_osc133_d_with_exit_code() {
        let input = b"\x1b]133;D;0\x07";
        let out = run(input);
        assert_eq!(
            out.osc133_events,
            vec![Osc133Event::CommandFinished(Some(0))]
        );
        assert!(out.passthrough.is_empty());
    }

    // -------------------------------------------------------------------------
    // 8. OSC 133 A / B / C variants
    // -------------------------------------------------------------------------

    #[test]
    fn test_osc133_a() {
        let out = run(b"\x1b]133;A\x07");
        assert_eq!(out.osc133_events, vec![Osc133Event::PromptStart]);
    }

    #[test]
    fn test_osc133_b() {
        let out = run(b"\x1b]133;B\x07");
        assert_eq!(out.osc133_events, vec![Osc133Event::CommandStart]);
    }

    #[test]
    fn test_osc133_c() {
        let out = run(b"\x1b]133;C\x07");
        assert_eq!(out.osc133_events, vec![Osc133Event::CommandExecuted]);
    }

    #[test]
    fn test_osc133_d_no_exit_code() {
        let out = run(b"\x1b]133;D\x07");
        assert_eq!(out.osc133_events, vec![Osc133Event::CommandFinished(None)]);
    }

    // -------------------------------------------------------------------------
    // 9. Mixed sequence: text + APC + text + OSC 7770
    // -------------------------------------------------------------------------

    #[test]
    fn test_mixed_sequence() {
        let mut input = Vec::new();
        input.extend_from_slice(b"before");
        input.extend_from_slice(b"\x1b_kitty\x1b\\");
        input.extend_from_slice(b"middle");
        input.extend_from_slice(b"\x1b]7770;event\x07");
        input.extend_from_slice(b"after");

        let out = run(&input);

        assert_eq!(out.passthrough, b"beforemiddleafter");
        assert_eq!(out.apc_payloads, vec![b"kitty".to_vec()]);
        assert_eq!(out.osc7770_params, vec!["event".to_string()]);
        assert!(out.osc133_events.is_empty());
    }

    // -------------------------------------------------------------------------
    // 10. Non-intercepted OSC: OSC 0 (window title) passes through
    // -------------------------------------------------------------------------

    #[test]
    fn test_non_intercepted_osc_passthrough() {
        // OSC 0 ; My Title BEL
        let input = b"\x1b]0;My Title\x07";
        let out = run(input);

        // The sequence must appear in passthrough (reconstructed).
        // We expect ESC ] 0 ; My Title BEL.
        assert_eq!(out.passthrough, b"\x1b]0;My Title\x07");
        assert!(out.osc7770_params.is_empty());
        assert!(out.osc133_events.is_empty());
        assert!(out.apc_payloads.is_empty());
    }

    // -------------------------------------------------------------------------
    // 11. ESC followed by non-special byte (CSI) passes through unchanged
    // -------------------------------------------------------------------------

    #[test]
    fn test_csi_passthrough() {
        // ESC [ 0 m — SGR reset
        let input = b"\x1b[0m";
        let out = run(input);
        assert_eq!(out.passthrough, b"\x1b[0m");
        assert!(out.apc_payloads.is_empty());
        assert!(out.osc7770_params.is_empty());
        assert!(out.osc133_events.is_empty());
    }

    // -------------------------------------------------------------------------
    // 12. Empty input produces empty output (ISSUE-021)
    // -------------------------------------------------------------------------

    #[test]
    fn test_empty_input() {
        let out = PreFilter::new().advance(&[]);
        assert!(out.passthrough.is_empty());
        assert!(out.apc_payloads.is_empty());
        assert!(out.osc7770_params.is_empty());
        assert!(out.osc133_events.is_empty());
    }

    // -------------------------------------------------------------------------
    // 13. Non-intercepted OSC with ST terminator passes through with ST
    //     (ISSUE-020: reconstruct_osc_passthrough must replay original terminator)
    // -------------------------------------------------------------------------

    #[test]
    fn test_non_intercepted_osc_st_passthrough() {
        // OSC 0 ; My Title ESC \ — window title with ST terminator
        let input = b"\x1b]0;My Title\x1b\\";
        let out = run(input);
        // Must be reconstructed with ST, not BEL.
        assert_eq!(out.passthrough, b"\x1b]0;My Title\x1b\\");
        assert!(out.osc7770_params.is_empty());
        assert!(out.osc133_events.is_empty());
        assert!(out.apc_payloads.is_empty());
    }

    // -------------------------------------------------------------------------
    // 14. ESC as final byte (PendingEsc split boundary) (ISSUE-023)
    // -------------------------------------------------------------------------

    #[test]
    fn test_pending_esc_split_boundary() {
        let mut pf = PreFilter::new();

        // First call: plain text followed by a bare ESC (no second byte yet).
        let first = pf.advance(b"hello\x1b");
        // The ESC is held in PendingEsc state — only the plain text passes through.
        assert_eq!(first.passthrough, b"hello");
        assert!(first.apc_payloads.is_empty());
        assert!(first.osc7770_params.is_empty());
        assert!(first.osc133_events.is_empty());

        // Second call: complete the CSI sequence ESC [ 3 1 m.
        let second = pf.advance(b"[31m");
        // The buffered ESC plus the CSI suffix must all pass through.
        assert_eq!(second.passthrough, b"\x1b[31m");
        assert!(second.apc_payloads.is_empty());
        assert!(second.osc7770_params.is_empty());
        assert!(second.osc133_events.is_empty());
    }
}
