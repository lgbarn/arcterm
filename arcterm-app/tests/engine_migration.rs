//! Integration tests verifying that the Phase-12 alacritty_terminal migration
//! preserved functional parity with the previous arcterm-core/vt/pty stack.
//!
//! ## Running locally
//!
//! All five tests can be run locally (a real PTY is required):
//! ```
//! cargo test -p arcterm-app --test engine_migration
//! ```
//!
//! Tests marked `#[ignore]` require a real TTY/PTY and will not run in
//! headless CI environments (e.g. `cargo test` inside a Docker container
//! without a controlling terminal).  To run them:
//! ```
//! cargo test -p arcterm-app --test engine_migration -- --ignored
//! ```

// ---------------------------------------------------------------------------
// Test 1: Terminal creation — PTY opens and child PID is populated
// ---------------------------------------------------------------------------

/// Verify that `Terminal::new` opens a real PTY and reports a child PID.
///
/// This test requires a real PTY (TTY required).  It is marked `#[ignore]`
/// for headless CI environments.
#[test]
#[ignore = "requires real PTY (run locally with -- --ignored)"]
fn terminal_creates_pty_and_reports_pid() {
    use arcterm_app::Terminal;

    let (terminal, _image_rx) = Terminal::new(80, 24, 9, 18, None, None)
        .expect("Terminal::new must succeed with a real PTY");

    let pid = terminal.child_pid();
    assert!(pid.is_some(), "child_pid() must return Some after creation");
    assert!(pid.unwrap() > 0, "child PID must be a positive integer");
}

// ---------------------------------------------------------------------------
// Test 2: PreFilter round-trip — intercepts OSC 7770, APC, passes plain text
// ---------------------------------------------------------------------------

/// Feed a byte buffer containing OSC 7770, APC, and plain text through the
/// `PreFilter`.  Verify that:
/// - passthrough bytes contain only the plain-text portion
/// - OSC 7770 params are extracted correctly
/// - APC payloads are extracted correctly
#[test]
fn prefilter_round_trip_separates_intercepted_and_passthrough() {
    use arcterm_app::PreFilter;

    let mut pf = PreFilter::new();

    // Plain text "hello"
    let hello = b"hello";
    // OSC 7770 with BEL terminator: ESC ] 7770 ; start ; type=test BEL
    let osc7770 = b"\x1b]7770;start;type=test\x07";
    // APC sequence: ESC _ payload ESC \
    let apc = b"\x1b_apc-payload\x1b\\";
    // Plain text "world"
    let world = b"world";

    let mut combined = Vec::new();
    combined.extend_from_slice(hello);
    combined.extend_from_slice(osc7770);
    combined.extend_from_slice(apc);
    combined.extend_from_slice(world);

    let out = pf.advance(&combined);

    // Passthrough must contain only "hello" and "world".
    assert_eq!(
        out.passthrough,
        b"helloworld",
        "passthrough must contain only plain text, got: {:?}",
        String::from_utf8_lossy(&out.passthrough)
    );

    // OSC 7770 params extracted.
    assert_eq!(out.osc7770_params.len(), 1, "expected 1 OSC 7770 param");
    assert_eq!(
        out.osc7770_params[0], "start;type=test",
        "OSC 7770 param mismatch"
    );

    // APC payload extracted.
    assert_eq!(out.apc_payloads.len(), 1, "expected 1 APC payload");
    assert_eq!(out.apc_payloads[0], b"apc-payload", "APC payload mismatch");
}

/// Verify that `PreFilter` handles sequences split across multiple `advance`
/// calls (simulates PTY read boundary splits).
#[test]
fn prefilter_handles_split_sequences() {
    use arcterm_app::PreFilter;

    let mut pf = PreFilter::new();

    // Split the OSC 7770 sequence across two advance calls.
    let part1 = b"\x1b]7770;start";
    let part2 = b";type=split\x07";

    let out1 = pf.advance(part1);
    // No complete OSC yet — params should be empty.
    assert!(
        out1.osc7770_params.is_empty(),
        "no complete OSC after first half"
    );

    let out2 = pf.advance(part2);
    // The sequence completes on the second call.
    assert_eq!(
        out2.osc7770_params.len(),
        1,
        "OSC must complete on second advance"
    );
    assert_eq!(out2.osc7770_params[0], "start;type=split");
}

// ---------------------------------------------------------------------------
// Test 3: Write-input round-trip — "echo hello" appears in the grid
// ---------------------------------------------------------------------------

/// Write `echo hello\n` to the PTY, wait for wakeup, lock the `Term`, and
/// verify that "hello" appears somewhere in the renderable content.
///
/// Requires a real PTY.  Marked `#[ignore]` for headless CI.
#[test]
#[ignore = "requires real PTY (run locally with -- --ignored)"]
fn write_input_echo_hello_appears_in_grid() {
    use arcterm_app::Terminal;
    use arcterm_render::snapshot_from_term;
    use std::time::{Duration, Instant};

    let (mut terminal, _image_rx) =
        Terminal::new(80, 24, 9, 18, None, None).expect("Terminal::new must succeed");

    // Write the echo command.
    terminal.write_input(b"echo hello\n");

    // Poll for wakeup for up to 3 seconds.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut got_output = false;
    while Instant::now() < deadline {
        if terminal.has_wakeup() {
            got_output = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        got_output,
        "no wakeup received within 3 s after writing echo command"
    );

    // Snapshot the terminal and look for "hello".
    let snapshot = snapshot_from_term(&*terminal.lock_term());
    let found = (0..snapshot.rows).any(|r| {
        let row_text: String = snapshot.row(r).iter().map(|c| c.c).collect();
        row_text.contains("hello")
    });
    assert!(
        found,
        "expected 'hello' in terminal grid after echo command"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Resize — terminal dimensions update after resize
// ---------------------------------------------------------------------------

/// Create a terminal at 80×24 and resize it to 120×40.  Verify the stored
/// dimensions update immediately.
///
/// Requires a real PTY.  Marked `#[ignore]` for headless CI.
#[test]
#[ignore = "requires real PTY (run locally with -- --ignored)"]
fn resize_updates_terminal_dimensions() {
    use arcterm_app::Terminal;

    let (mut terminal, _image_rx) =
        Terminal::new(80, 24, 9, 18, None, None).expect("Terminal::new must succeed");

    assert_eq!(terminal.cols(), 80);
    assert_eq!(terminal.rows(), 24);

    terminal.resize(120, 40, 9, 18);

    assert_eq!(terminal.cols(), 120, "cols must update to 120 after resize");
    assert_eq!(terminal.rows(), 40, "rows must update to 40 after resize");
}

// ---------------------------------------------------------------------------
// Test 5: OSC 7770 structured content round-trip
// ---------------------------------------------------------------------------

/// Feed a complete OSC 7770 start/content/end sequence to the PreFilter and
/// verify the params are extracted correctly.  This tests the structured
/// content pipeline without requiring a real PTY.
#[test]
fn prefilter_osc7770_start_content_end_sequence() {
    use arcterm_app::PreFilter;

    let mut pf = PreFilter::new();

    // Build a complete OSC 7770 structured block sequence.
    let start = b"\x1b]7770;start;type=code;lang=rust\x07";
    let end_seq = b"\x1b]7770;end\x07";
    let content = b"fn main() {}";

    let mut input = Vec::new();
    input.extend_from_slice(start);
    input.extend_from_slice(content);
    input.extend_from_slice(end_seq);

    let out = pf.advance(&input);

    // Two OSC 7770 completions: start and end.
    assert_eq!(
        out.osc7770_params.len(),
        2,
        "expected 2 OSC 7770 params (start + end), got: {:?}",
        out.osc7770_params
    );
    assert_eq!(out.osc7770_params[0], "start;type=code;lang=rust");
    assert_eq!(out.osc7770_params[1], "end");

    // The content text passes through unchanged.
    assert_eq!(
        out.passthrough, b"fn main() {}",
        "content between OSC sequences must pass through"
    );
}
