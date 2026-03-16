//! VT byte-stream processor — bridges vte::Parser to the Handler trait.

use std::collections::HashMap;

use crate::{handler::ContentType, Handler};

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
// OSC 133 — shell integration (prompt/command marks)
// ---------------------------------------------------------------------------

/// Parse and dispatch an OSC 133 shell-integration sequence to the `Handler`.
///
/// The OSC params layout is:
///   params[0] = b"133"
///   params[1] = b"A" | b"B" | b"C" | b"D" [with optional params[2] for exit code]
///
/// Mappings:
///   A  → prompt start      (shell_prompt_start)
///   B  → command start     (shell_command_start)
///   C  → command executed  (no-op; the command is now running)
///   D  → command end       (shell_command_end with optional exit code)
///
/// Unknown sub-commands are silently ignored so future extensions do not break.
fn dispatch_osc133<H: Handler>(handler: &mut H, params: &[&[u8]]) {
    // Need at least params[0]=133 and params[1]=sub-command.
    if params.len() < 2 {
        return;
    }

    match params[1] {
        b"A" => {
            handler.shell_prompt_start();
        }
        b"B" => {
            handler.shell_command_start();
        }
        b"C" => {
            // Command is now executing — no-op per spec.
        }
        b"D" => {
            // Optional exit code in params[2], defaulting to 0.
            let exit_code = if params.len() >= 3 {
                std::str::from_utf8(params[2])
                    .ok()
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(0)
            } else {
                0
            };
            handler.shell_command_end(exit_code);
        }
        _ => {} // unknown sub-command — silently ignored
    }
}

// ---------------------------------------------------------------------------
// OSC 7770 — structured content block dispatch
// ---------------------------------------------------------------------------

/// Parse and dispatch an OSC 7770 sequence to the `Handler`.
///
/// The OSC params layout is:
///   params[0] = b"7770"
///   params[1] = b"start" | b"end"
///   params[2..] = b"key=value" pairs; the first must be b"type=<content_type>"
///
/// For a `start` command: parse `type=` to a `ContentType`, collect remaining
/// `key=value` pairs as attrs, and call `handler.structured_content_start()`.
///
/// For an `end` command: call `handler.structured_content_end()`.
///
/// Any malformed sequence (missing action, missing/unknown type) is silently
/// ignored.
fn dispatch_osc7770<H: Handler>(handler: &mut H, params: &[&[u8]]) {
    // Need at least params[0]=7770 and params[1]=action.
    if params.len() < 2 {
        return;
    }

    match params[1] {
        b"start" => {
            // The first param after "start" must be "type=<content_type>".
            // params[2] is the type param; params[3..] are additional attrs.
            if params.len() < 3 {
                return; // no type= param — ignore
            }

            // Parse params[2] as "type=<value>".
            let type_param = match std::str::from_utf8(params[2]) {
                Ok(s) => s,
                Err(_) => return,
            };
            let content_type_str = match type_param.strip_prefix("type=") {
                Some(t) => t,
                None => return, // first param is not type= — ignore
            };

            let content_type = match content_type_str {
                "code" => ContentType::CodeBlock,
                "diff" => ContentType::Diff,
                "plan" => ContentType::Plan,
                "markdown" => ContentType::Markdown,
                "json" => ContentType::Json,
                "error" => ContentType::Error,
                "progress" => ContentType::Progress,
                "image" => ContentType::Image,
                _ => return, // unknown type — ignore
            };

            // Parse any remaining params as key=value pairs.
            let mut attrs: HashMap<String, String> = HashMap::new();
            for raw in &params[3..] {
                if let Ok(kv) = std::str::from_utf8(raw)
                    && let Some((key, val)) = kv.split_once('=')
                {
                    attrs.insert(key.to_string(), val.to_string());
                }
            }

            handler.structured_content_start(content_type, attrs);
        }
        b"end" => {
            handler.structured_content_end();
        }

        // MCP tool discovery: AI agent queries available plugin tools.
        // ESC ] 7770 ; tools/list ST
        b"tools/list" => {
            handler.tool_list_query();
        }

        // MCP tool invocation: AI agent calls a named plugin tool.
        // ESC ] 7770 ; tools/call ; name=<tool_name> ; args=<base64_json> ST
        b"tools/call" => {
            // Parse name= and args= from params[2..].
            let mut tool_name: Option<String> = None;
            let mut args_b64: Option<&[u8]> = None;
            for raw in &params[2..] {
                if let Ok(kv) = std::str::from_utf8(raw)
                    && let Some((key, val)) = kv.split_once('=')
                {
                    match key {
                        "name" => tool_name = Some(val.to_string()),
                        "args" => args_b64 = raw.get(5..).or(Some(b"")), // skip "args=", bounds-safe
                        _ => {}
                    }
                }
            }

            let (Some(name), Some(b64)) = (tool_name, args_b64) else {
                return; // missing required params — silently ignore
            };

            // Base64-decode the args.
            use base64::Engine as _;
            let decoded = match base64::engine::general_purpose::STANDARD.decode(b64) {
                Ok(bytes) => bytes,
                Err(_) => return, // invalid base64 — silently ignore
            };
            let args_json = match String::from_utf8(decoded) {
                Ok(s) => s,
                Err(_) => return, // not valid UTF-8 — silently ignore
            };

            handler.tool_call(name, args_json);
        }

        // Cross-pane context query: AI agent requests sibling pane metadata.
        // ESC ] 7770 ; context/query ST
        b"context/query" => {
            handler.context_query();
        }

        _ => {} // unknown action — ignore
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
        // params[0] is the numeric command identifier; params[1..] are the values.
        if params.is_empty() {
            return;
        }
        match params[0] {
            b"0" | b"2" => {
                if params.len() < 2 {
                    return;
                }
                let title = std::str::from_utf8(params[1]).unwrap_or("");
                self.handler.set_title(title);
            }
            b"133" => {
                dispatch_osc133(self.handler, params);
            }
            b"7770" => {
                dispatch_osc7770(self.handler, params);
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

// ---------------------------------------------------------------------------
// Tests — Task 2 (Phase 4): OSC 7770 dispatch
// ---------------------------------------------------------------------------

#[cfg(test)]
mod phase4_task2_tests {
    use arcterm_core::{Grid, GridSize};

    use crate::{handler::ContentType, GridState, Processor};

    fn make_gs() -> GridState {
        GridState::new(Grid::new(GridSize::new(24, 80)))
    }

    fn feed(gs: &mut GridState, bytes: &[u8]) {
        let mut proc = Processor::new();
        proc.advance(gs, bytes);
    }

    // Helper: build an OSC 7770 start sequence.
    // ESC ] 7770 ; start ; type=<t> [; key=value]* ST
    fn osc7770_start(content_type: &str, extra_attrs: &[(&str, &str)]) -> Vec<u8> {
        let mut s = format!("\x1b]7770;start;type={}", content_type);
        for (k, v) in extra_attrs {
            s.push(';');
            s.push_str(k);
            s.push('=');
            s.push_str(v);
        }
        s.push('\x07'); // BEL terminator
        s.into_bytes()
    }

    // Helper: build an OSC 7770 end sequence.
    fn osc7770_end() -> Vec<u8> {
        b"\x1b]7770;end\x07".to_vec()
    }

    // --- complete code block ---

    #[test]
    fn osc7770_complete_code_block() {
        let mut gs = make_gs();
        feed(&mut gs, &osc7770_start("code", &[("lang", "rust")]));
        feed(&mut gs, b"fn main() {}");
        feed(&mut gs, &osc7770_end());

        assert_eq!(gs.completed_blocks.len(), 1);
        let block = &gs.completed_blocks[0];
        assert!(matches!(block.content_type, ContentType::CodeBlock));
        assert_eq!(block.attrs.get("lang").map(|s| s.as_str()), Some("rust"));
        assert_eq!(block.buffer, "fn main() {}");
        assert!(gs.accumulator.is_none());
    }

    // --- JSON block ---

    #[test]
    fn osc7770_json_block() {
        let mut gs = make_gs();
        feed(&mut gs, &osc7770_start("json", &[]));
        feed(&mut gs, b"{\"key\":\"value\"}");
        feed(&mut gs, &osc7770_end());

        assert_eq!(gs.completed_blocks.len(), 1);
        let block = &gs.completed_blocks[0];
        assert!(matches!(block.content_type, ContentType::Json));
        assert_eq!(block.buffer, "{\"key\":\"value\"}");
    }

    // --- all content type mappings ---

    #[test]
    fn osc7770_content_type_diff() {
        let mut gs = make_gs();
        feed(&mut gs, &osc7770_start("diff", &[]));
        feed(&mut gs, &osc7770_end());
        assert!(matches!(gs.completed_blocks[0].content_type, ContentType::Diff));
    }

    #[test]
    fn osc7770_content_type_plan() {
        let mut gs = make_gs();
        feed(&mut gs, &osc7770_start("plan", &[]));
        feed(&mut gs, &osc7770_end());
        assert!(matches!(gs.completed_blocks[0].content_type, ContentType::Plan));
    }

    #[test]
    fn osc7770_content_type_markdown() {
        let mut gs = make_gs();
        feed(&mut gs, &osc7770_start("markdown", &[]));
        feed(&mut gs, &osc7770_end());
        assert!(matches!(gs.completed_blocks[0].content_type, ContentType::Markdown));
    }

    #[test]
    fn osc7770_content_type_error() {
        let mut gs = make_gs();
        feed(&mut gs, &osc7770_start("error", &[]));
        feed(&mut gs, &osc7770_end());
        assert!(matches!(gs.completed_blocks[0].content_type, ContentType::Error));
    }

    #[test]
    fn osc7770_content_type_progress() {
        let mut gs = make_gs();
        feed(&mut gs, &osc7770_start("progress", &[]));
        feed(&mut gs, &osc7770_end());
        assert!(matches!(gs.completed_blocks[0].content_type, ContentType::Progress));
    }

    #[test]
    fn osc7770_content_type_image() {
        let mut gs = make_gs();
        feed(&mut gs, &osc7770_start("image", &[]));
        feed(&mut gs, &osc7770_end());
        assert!(matches!(gs.completed_blocks[0].content_type, ContentType::Image));
    }

    // --- end without start is a no-op ---

    #[test]
    fn osc7770_end_without_start_is_noop() {
        let mut gs = make_gs();
        feed(&mut gs, &osc7770_end());
        assert!(gs.completed_blocks.is_empty());
        assert!(gs.accumulator.is_none());
    }

    // --- regular text before and after a block is unaffected ---

    #[test]
    fn osc7770_regular_text_before_and_after() {
        let mut gs = make_gs();
        feed(&mut gs, b"before");
        feed(&mut gs, &osc7770_start("markdown", &[]));
        feed(&mut gs, b"inside");
        feed(&mut gs, &osc7770_end());
        feed(&mut gs, b"after");

        // Grid should contain "before", "inside", and "after" at appropriate positions.
        assert_eq!(gs.grid.cells[0][0].c, 'b');
        assert_eq!(gs.grid.cells[0][5].c, 'e'); // end of "before"
        // Block buffer contains only "inside".
        assert_eq!(gs.completed_blocks[0].buffer, "inside");
    }

    // --- multi-line block accumulation ---

    #[test]
    fn osc7770_multiline_block() {
        let mut gs = make_gs();
        feed(&mut gs, &osc7770_start("code", &[("lang", "python")]));
        // Simulate multi-line content via CR+LF sequences.
        feed(&mut gs, b"line1\r\nline2");
        feed(&mut gs, &osc7770_end());

        let block = &gs.completed_blocks[0];
        // Buffer should contain the printable characters (CR and LF are control codes,
        // not printed as characters — only the visible chars are appended via put_char).
        assert!(block.buffer.contains("line1"));
        assert!(block.buffer.contains("line2"));
    }

    // --- multiple key=value attrs ---

    #[test]
    fn osc7770_multiple_attrs() {
        let mut gs = make_gs();
        feed(
            &mut gs,
            &osc7770_start("code", &[("lang", "rust"), ("file", "main.rs")]),
        );
        feed(&mut gs, &osc7770_end());

        let block = &gs.completed_blocks[0];
        assert_eq!(block.attrs.get("lang").map(|s| s.as_str()), Some("rust"));
        assert_eq!(block.attrs.get("file").map(|s| s.as_str()), Some("main.rs"));
    }

    // --- start missing type= param is ignored ---

    #[test]
    fn osc7770_start_without_type_is_ignored() {
        let mut gs = make_gs();
        // OSC with no type= in params[2].
        feed(&mut gs, b"\x1b]7770;start\x07");
        assert!(gs.accumulator.is_none());
        feed(&mut gs, &osc7770_end());
        assert!(gs.completed_blocks.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Tests — Plan 7.2 Task 1: OSC 7770 tools/list and tools/call dispatch
// ---------------------------------------------------------------------------

#[cfg(test)]
mod osc7770_tools_tests {
    use arcterm_core::{Grid, GridSize};

    use crate::{GridState, Processor};

    fn make_gs() -> GridState {
        GridState::new(Grid::new(GridSize::new(24, 80)))
    }

    fn feed(gs: &mut GridState, bytes: &[u8]) {
        let mut proc = Processor::new();
        proc.advance(gs, bytes);
    }

    /// ESC ] 7770 ; tools/list BEL — should push one entry to tool_queries.
    #[test]
    fn tools_list_sets_flag() {
        let mut gs = make_gs();
        feed(&mut gs, b"\x1b]7770;tools/list\x07");
        assert_eq!(gs.tool_queries.len(), 1, "expected one tool_query entry");
    }

    /// A second tools/list accumulates.
    #[test]
    fn tools_list_accumulates() {
        let mut gs = make_gs();
        feed(&mut gs, b"\x1b]7770;tools/list\x07");
        feed(&mut gs, b"\x1b]7770;tools/list\x07");
        assert_eq!(gs.tool_queries.len(), 2);
    }

    /// take_tool_queries drains the buffer.
    #[test]
    fn take_tool_queries_drains() {
        let mut gs = make_gs();
        feed(&mut gs, b"\x1b]7770;tools/list\x07");
        let drained = gs.take_tool_queries();
        assert_eq!(drained.len(), 1);
        assert!(gs.tool_queries.is_empty(), "buffer must be empty after drain");
    }

    /// ESC ] 7770 ; tools/call ; name=get_pods ; args=<base64("{}") BEL
    #[test]
    fn tools_call_parses_name_and_args() {
        // base64("{}")  = "e30="
        let seq = b"\x1b]7770;tools/call;name=get_pods;args=e30=\x07";
        let mut gs = make_gs();
        feed(&mut gs, seq);
        assert_eq!(gs.tool_calls.len(), 1);
        let (name, args) = &gs.tool_calls[0];
        assert_eq!(name, "get_pods");
        assert_eq!(args, "{}");
    }

    /// take_tool_calls drains the buffer.
    #[test]
    fn take_tool_calls_drains() {
        let seq = b"\x1b]7770;tools/call;name=foo;args=e30=\x07";
        let mut gs = make_gs();
        feed(&mut gs, seq);
        let drained = gs.take_tool_calls();
        assert_eq!(drained.len(), 1);
        assert!(gs.tool_calls.is_empty(), "buffer must be empty after drain");
    }

    /// Malformed tools/call (missing name=) is silently ignored.
    #[test]
    fn tools_call_missing_name_is_ignored() {
        let mut gs = make_gs();
        feed(&mut gs, b"\x1b]7770;tools/call;args=e30=\x07");
        assert!(gs.tool_calls.is_empty(), "malformed call must be ignored");
    }

    /// tools/list without a name param is still valid (it has no extra params).
    #[test]
    fn tools_list_extra_params_ignored() {
        let mut gs = make_gs();
        // Extra params after tools/list should not cause a panic; query still recorded.
        feed(&mut gs, b"\x1b]7770;tools/list;extra=val\x07");
        assert_eq!(gs.tool_queries.len(), 1);
    }

    /// Security I1: tools/call with a malformed args= param shorter than 5 bytes
    /// must not panic (previously did `raw[5..]` unconditionally).
    #[test]
    fn tools_call_short_args_does_not_panic() {
        let mut gs = make_gs();
        // "args" is exactly 4 bytes — raw[5..] would panic before the fix.
        feed(&mut gs, b"\x1b]7770;tools/call;name=foo;args\x07");
        // Either silently ignored or recorded — what matters is no panic.
        // With args= missing a value, the split_once('=') finds nothing and
        // the call is dropped (missing args param).
        assert!(gs.tool_calls.is_empty(), "malformed args must be ignored");
    }
}

// ---------------------------------------------------------------------------
// Tests — Phase 9 Plan 1.2: VT regression tests (ISSUE-011, 012, 013)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod phase9_regression_tests {
    use arcterm_core::{CursorPos, Grid, GridSize};

    use crate::{GridState, Processor};

    fn make_gs() -> GridState {
        GridState::new(Grid::new(GridSize::new(24, 80)))
    }

    fn make_gs_with_size(rows: usize, cols: usize) -> GridState {
        GridState::new(Grid::new(GridSize::new(rows, cols)))
    }

    // -------------------------------------------------------------------------
    // ISSUE-011: esc_dispatch intermediates guard
    // -------------------------------------------------------------------------

    /// ESC ( 7 (SCS — Select Character Set) must NOT trigger save_cursor_position.
    /// The byte 0x37 ('7') is also DECSC's final byte, but only when there are
    /// no intermediates.  With the intermediate '(', it's SCS and must be ignored.
    #[test]
    fn esc_dispatch_with_intermediates_does_not_save_cursor() {
        let mut gs = make_gs();
        gs.grid.set_cursor(CursorPos { row: 3, col: 5 });
        let mut proc = Processor::new();
        // ESC ( 7 — SCS select character set — must NOT trigger save_cursor_position.
        proc.advance(&mut gs, b"\x1b(7");
        assert!(
            gs.saved_cursor.is_none(),
            "SCS ESC(7 must not trigger save_cursor"
        );
    }

    /// ESC 7 (bare DECSC, no intermediates) MUST save the cursor, and ESC 8
    /// (DECRC) must restore it.
    #[test]
    fn esc_dispatch_bare_esc7_saves_cursor() {
        let mut gs = make_gs();
        gs.grid.set_cursor(CursorPos { row: 3, col: 5 });
        let mut proc = Processor::new();
        proc.advance(&mut gs, b"\x1b7");
        // Move cursor elsewhere
        gs.grid.set_cursor(CursorPos { row: 0, col: 0 });
        // ESC 8 (DECRC) should restore to (3, 5)
        proc.advance(&mut gs, b"\x1b8");
        assert_eq!(gs.grid.cursor(), CursorPos { row: 3, col: 5 });
    }

    // -------------------------------------------------------------------------
    // ISSUE-012: modes 47/1047 (alt screen) and mouse modes 1000/1006
    // -------------------------------------------------------------------------

    #[test]
    fn set_mode_47_enters_alt_screen() {
        let mut gs = make_gs();
        let mut proc = Processor::new();
        proc.advance(&mut gs, b"hello");
        // CSI ? 47 h — enter alt screen
        proc.advance(&mut gs, b"\x1b[?47h");
        assert!(gs.modes.alt_screen, "mode 47 should enter alt screen");
    }

    #[test]
    fn reset_mode_47_leaves_alt_screen() {
        let mut gs = make_gs();
        let mut proc = Processor::new();
        proc.advance(&mut gs, b"\x1b[?47h");
        assert!(gs.modes.alt_screen);
        proc.advance(&mut gs, b"\x1b[?47l");
        assert!(!gs.modes.alt_screen, "mode 47 reset should leave alt screen");
    }

    #[test]
    fn set_mode_1047_enters_alt_screen() {
        let mut gs = make_gs();
        let mut proc = Processor::new();
        proc.advance(&mut gs, b"\x1b[?1047h");
        assert!(gs.modes.alt_screen, "mode 1047 should enter alt screen");
    }

    #[test]
    fn reset_mode_1047_leaves_alt_screen() {
        let mut gs = make_gs();
        let mut proc = Processor::new();
        proc.advance(&mut gs, b"\x1b[?1047h");
        assert!(gs.modes.alt_screen);
        proc.advance(&mut gs, b"\x1b[?1047l");
        assert!(!gs.modes.alt_screen, "mode 1047 reset should leave alt screen");
    }

    #[test]
    fn set_mode_1000_enables_mouse_click_report() {
        let mut gs = make_gs();
        let mut proc = Processor::new();
        proc.advance(&mut gs, b"\x1b[?1000h");
        assert!(gs.modes.mouse_report_click);
    }

    #[test]
    fn reset_mode_1000_disables_mouse_click_report() {
        let mut gs = make_gs();
        let mut proc = Processor::new();
        proc.advance(&mut gs, b"\x1b[?1000h");
        assert!(gs.modes.mouse_report_click);
        proc.advance(&mut gs, b"\x1b[?1000l");
        assert!(!gs.modes.mouse_report_click);
    }

    #[test]
    fn set_mode_1006_enables_sgr_mouse_ext() {
        let mut gs = make_gs();
        let mut proc = Processor::new();
        proc.advance(&mut gs, b"\x1b[?1006h");
        assert!(gs.modes.mouse_sgr_ext);
    }

    // -------------------------------------------------------------------------
    // ISSUE-013: newline cursor-above-scroll-region behavior
    // -------------------------------------------------------------------------

    /// A cursor above the scroll region advances row-by-row toward the region
    /// without triggering a scroll.  Once at the scroll bottom, further newlines
    /// scroll the region and keep the cursor pinned at the bottom row.
    #[test]
    fn newline_cursor_above_scroll_region_advances_into_region() {
        let mut gs = make_gs_with_size(10, 80); // 10 rows, 80 cols
        let mut proc = Processor::new();
        // Set scroll region to rows 4–8 (1-indexed) = rows 3–7 (0-indexed): CSI 4;8 r
        proc.advance(&mut gs, b"\x1b[4;8r");
        // Move cursor to row 0: CSI 1;1 H (1-indexed)
        proc.advance(&mut gs, b"\x1b[1;1H");
        assert_eq!(gs.grid.cursor().row, 0);

        // Cursor above scroll region: advance freely row-by-row
        proc.advance(&mut gs, b"\n");
        assert_eq!(gs.grid.cursor().row, 1);
        proc.advance(&mut gs, b"\n");
        assert_eq!(gs.grid.cursor().row, 2);
        proc.advance(&mut gs, b"\n");
        assert_eq!(gs.grid.cursor().row, 3); // entered scroll region

        // Continue advancing within the region
        proc.advance(&mut gs, b"\n");
        assert_eq!(gs.grid.cursor().row, 4);
        proc.advance(&mut gs, b"\n");
        assert_eq!(gs.grid.cursor().row, 5);
        proc.advance(&mut gs, b"\n");
        assert_eq!(gs.grid.cursor().row, 6);
        proc.advance(&mut gs, b"\n");
        assert_eq!(gs.grid.cursor().row, 7); // at scroll bottom

        // Next newline should scroll the region; cursor stays pinned at row 7
        proc.advance(&mut gs, b"\n");
        assert_eq!(gs.grid.cursor().row, 7, "cursor should stay pinned at scroll bottom");
    }
}

// ---------------------------------------------------------------------------
// Tests — Plan 7.1 Task 2: OSC 133 shell integration
// ---------------------------------------------------------------------------

#[cfg(test)]
mod osc133_tests {
    use arcterm_core::{Grid, GridSize};

    use crate::{GridState, Processor};

    fn make_gs() -> GridState {
        GridState::new(Grid::new(GridSize::new(24, 80)))
    }

    fn feed(gs: &mut GridState, bytes: &[u8]) {
        let mut proc = Processor::new();
        proc.advance(gs, bytes);
    }

    // Helper: build an OSC 133 sequence terminated with BEL.
    // ESC ] 133 ; <sub> [; <extra>] BEL
    fn osc133(sub: &str) -> Vec<u8> {
        format!("\x1b]133;{sub}\x07").into_bytes()
    }

    fn osc133_with_code(sub: &str, code: &str) -> Vec<u8> {
        format!("\x1b]133;{sub};{code}\x07").into_bytes()
    }

    /// OSC 133;A is a no-op on the grid — accepted without error.
    #[test]
    fn osc133_a_is_noop_on_grid() {
        let mut gs = make_gs();
        feed(&mut gs, &osc133("A"));
        // No side-effects visible on grid-level state.
        assert!(gs.shell_exit_codes.is_empty());
        assert!(!gs.pending_command_start);
    }

    /// OSC 133;B sets the pending_command_start flag.
    #[test]
    fn osc133_b_sets_pending_command_start() {
        let mut gs = make_gs();
        feed(&mut gs, &osc133("B"));
        assert!(gs.pending_command_start);
    }

    /// OSC 133;C is accepted without error (no-op per spec).
    #[test]
    fn osc133_c_is_noop() {
        let mut gs = make_gs();
        feed(&mut gs, &osc133("C"));
        assert!(gs.shell_exit_codes.is_empty());
    }

    /// OSC 133;D;1 sets exit code 1.
    #[test]
    fn osc133_d_with_code_sets_exit_code() {
        let mut gs = make_gs();
        feed(&mut gs, &osc133_with_code("D", "1"));
        assert_eq!(gs.shell_exit_codes, vec![1]);
        assert!(!gs.pending_command_start);
    }

    /// OSC 133;D without a code defaults to exit code 0.
    #[test]
    fn osc133_d_without_code_defaults_to_zero() {
        let mut gs = make_gs();
        feed(&mut gs, &osc133("D"));
        assert_eq!(gs.shell_exit_codes, vec![0]);
    }

    /// Multiple D sequences accumulate in shell_exit_codes.
    #[test]
    fn osc133_d_multiple_codes_accumulate() {
        let mut gs = make_gs();
        feed(&mut gs, &osc133_with_code("D", "0"));
        feed(&mut gs, &osc133_with_code("D", "127"));
        assert_eq!(gs.shell_exit_codes, vec![0, 127]);
    }

    /// take_exit_codes drains the buffer and leaves it empty.
    #[test]
    fn osc133_take_exit_codes_drains_buffer() {
        let mut gs = make_gs();
        feed(&mut gs, &osc133_with_code("D", "42"));
        let codes = gs.take_exit_codes();
        assert_eq!(codes, vec![42]);
        assert!(gs.shell_exit_codes.is_empty());
    }

    /// A full A → B → D sequence is accepted cleanly.
    #[test]
    fn osc133_full_sequence_a_b_d() {
        let mut gs = make_gs();
        feed(&mut gs, &osc133("A")); // prompt start
        feed(&mut gs, &osc133("B")); // command start
        feed(&mut gs, &osc133_with_code("D", "0")); // command end, success
        assert!(!gs.pending_command_start);
        assert_eq!(gs.shell_exit_codes, vec![0]);
    }
}

// ---------------------------------------------------------------------------
// Tests — Plan 7.3 Task 2: OSC 7770 context/query dispatch
// ---------------------------------------------------------------------------

#[cfg(test)]
mod osc7770_context_tests {
    use arcterm_core::{Grid, GridSize};

    use crate::{GridState, Processor};

    fn make_gs() -> GridState {
        GridState::new(Grid::new(GridSize::new(24, 80)))
    }

    fn feed(gs: &mut GridState, bytes: &[u8]) {
        let mut proc = Processor::new();
        proc.advance(gs, bytes);
    }

    /// ESC ] 7770 ; context/query BEL — should push one entry to context_queries.
    #[test]
    fn context_query_pushes_sentinel() {
        let mut gs = make_gs();
        feed(&mut gs, b"\x1b]7770;context/query\x07");
        assert_eq!(gs.context_queries.len(), 1, "expected one context_query entry");
    }

    /// Two context/query sequences accumulate.
    #[test]
    fn context_query_accumulates() {
        let mut gs = make_gs();
        feed(&mut gs, b"\x1b]7770;context/query\x07");
        feed(&mut gs, b"\x1b]7770;context/query\x07");
        assert_eq!(gs.context_queries.len(), 2);
    }

    /// take_context_queries drains the buffer.
    #[test]
    fn take_context_queries_drains() {
        let mut gs = make_gs();
        feed(&mut gs, b"\x1b]7770;context/query\x07");
        let drained = gs.take_context_queries();
        assert_eq!(drained.len(), 1);
        assert!(gs.context_queries.is_empty(), "buffer must be empty after drain");
    }

    /// context/query with extra params is still accepted (future-proof).
    #[test]
    fn context_query_extra_params_ignored() {
        let mut gs = make_gs();
        feed(&mut gs, b"\x1b]7770;context/query;scope=tab\x07");
        assert_eq!(gs.context_queries.len(), 1);
    }
}
