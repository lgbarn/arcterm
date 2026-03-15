---
phase: terminal-fidelity
plan: "3.2"
wave: 3
dependencies: ["2.1", "2.3"]
must_haves:
  - 120+ FPS during fast cat output (measured)
  - PresentMode::Mailbox for uncapped frame rate
  - No visual artifacts in vim, tmux, htop, SSH sessions
  - App cursor keys mode changes arrow key sequences
  - Bracketed paste wraps pasted text correctly
files_touched:
  - arcterm-render/src/gpu.rs
  - arcterm-app/src/main.rs
  - arcterm-app/src/input.rs
  - arcterm-app/src/terminal.rs
tdd: false
---

# PLAN-3.2 -- Performance Optimization and Integration Verification

## Goal

Optimize rendering for 120+ FPS during fast output, fix the present mode for
uncapped frame rate, wire up application cursor key mode, and verify end-to-end
correctness with vim, tmux, htop, and SSH.

## Why Wave 3

Depends on PLAN-2.1 (quad pipeline and dirty-row optimization must exist to
optimize) and PLAN-2.3 (mouse events, scroll viewport, clipboard must work for
full integration testing). This is the final polish wave.

## Tasks

<task id="1" files="arcterm-render/src/gpu.rs, arcterm-app/src/main.rs" tdd="false">
  <action>
  Performance optimizations for 120+ FPS target.

  1. In `gpu.rs`, change `present_mode: wgpu::PresentMode::Fifo` to
     `wgpu::PresentMode::Mailbox`. Fall back to Fifo if Mailbox is not in
     `caps.present_modes`. Mailbox allows frame rates above vsync.

  2. In `main.rs` `about_to_wait()`, batch all pending PTY data before
     requesting a redraw. Currently the loop calls try_recv in a loop and sets
     got_data -- this is already correct. Verify that only ONE redraw is
     requested after draining all available data (already the case).

  3. In `main.rs`, add continuous redraw when the terminal is actively receiving
     data: after `got_data = true` processing, call
     `event_loop.set_control_flow(ControlFlow::Poll)` to keep the event loop
     spinning. When no data arrives for N consecutive about_to_wait calls
     (e.g., 3), switch to `ControlFlow::Wait` to avoid burning CPU when idle.
     This ensures maximum throughput during fast output.

  4. Add a frame counter: track frames-per-second using a simple counter and
     timestamp. Log FPS every 5 seconds when `RUST_LOG=debug` is set. This
     provides the measurement for the 120 FPS acceptance criterion.

  5. In the PTY reader thread (arcterm-pty/src/session.rs), increase the read
     buffer from 4096 to 16384 bytes to reduce syscall overhead during fast
     output. (This is a one-line change in session.rs that does not conflict
     with other plans.)
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app && RUST_LOG=debug cargo run --package arcterm-app 2>&1 | head -1</verify>
  <done>arcterm-app builds and runs. PresentMode is Mailbox (or Fifo fallback). FPS counter logs to debug output. ControlFlow switches between Poll (active) and Wait (idle). PTY reader uses 16KB buffer.</done>
</task>

<task id="2" files="arcterm-app/src/input.rs" tdd="true">
  <action>
  Wire application cursor key mode and bracketed paste into input handling.

  1. Modify `translate_key_event()` to accept a `modes: &TermModes` parameter
     (or individual booleans for the relevant modes).

  2. When `modes.app_cursor_keys` is true, change arrow key sequences:
     - ArrowUp: ESC O A (instead of ESC [ A)
     - ArrowDown: ESC O B
     - ArrowRight: ESC O C
     - ArrowLeft: ESC O D
     - Home: ESC O H (instead of ESC [ H)
     - End: ESC O F (instead of ESC [ F)

  3. When `modes.app_keypad` is true, change keypad sequences (if numpad keys
     are separately identifiable -- this may require checking the physical key).
     For Phase 2, at minimum handle the common case where vim sends DECKPAM.

  4. In the paste handler (Cmd+V from PLAN-2.3), when `modes.bracketed_paste`
     is true, wrap the pasted text: send `ESC[200~` before and `ESC[201~` after
     the pasted content.

  5. Update the call site in main.rs to pass the grid's modes to
     translate_key_event.

  6. Write tests:
     - Arrow up with app_cursor_keys=false => ESC [ A
     - Arrow up with app_cursor_keys=true => ESC O A
     - All four arrows tested in both modes
     - Home/End in both modes
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-app -- input</verify>
  <done>All input tests pass. Arrow keys produce ESC O sequences when app_cursor_keys is true. Bracketed paste wraps content correctly. translate_key_event accepts modes parameter.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs, arcterm-app/src/terminal.rs" tdd="false">
  <action>
  Integration verification and remaining wiring.

  1. Ensure the DSR (Device Status Report) response path works: when the terminal
     receives CSI 6 n (cursor position query), it needs to write back
     `ESC [ row ; col R` to the PTY. Add a mechanism:
     - Add a `pending_replies: Vec<Vec<u8>>` field to Grid or Terminal.
     - In the Handler impl for device_status_report(6), push the response bytes.
     - In `about_to_wait()` or after process_pty_output, drain pending_replies
       and write each to the PTY via terminal.write_input().
     This is required for vim (it queries cursor position on startup).

  2. Ensure DA (Device Attributes) response works: when CSI c is received,
     respond with `ESC [ ? 1 ; 2 c` (VT100 with advanced video option).
     Same mechanism as DSR.

  3. Add ISSUE-006 fix: the cursor is now rendered as a solid quad (from PLAN-2.1),
     but verify that when `modes.cursor_visible` is false, no cursor quad is
     generated. The render_frame path should check this flag.

  4. Wire up window title from grid: in RedrawRequested, if `grid.title()` is
     Some and differs from the current window title, call
     `window.set_title(title)`.

  5. Document manual test checklist as a comment in main.rs:
     - `echo -e "\033[41m red bg \033[0m"` -- red background renders
     - `vim` -- opens in alt screen, status bar stays fixed, cursor visible
     - `tmux` -- splits render, scroll regions work
     - `htop` -- full-screen rendering, bars update
     - `ssh remote_host` then run vim -- no artifacts
     - Mouse wheel scrolls through scrollback history
     - Cmd+C copies selected text, Cmd+V pastes
     - Change color_scheme in config.toml -- colors update live
     - `cat /dev/urandom | head -c 10M | base64` -- smooth, no freeze
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app && cargo test --workspace</verify>
  <done>Full workspace builds and all tests pass. DSR responds with cursor position. DA responds with VT100 identifier. Cursor visibility respects DECTCEM mode. Window title updates from OSC sequences. All manual test scenarios documented.</done>
</task>
