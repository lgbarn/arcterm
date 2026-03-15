---
phase: multiplexer
plan: "3.4"
wave: 3
dependencies: ["3.1"]
must_haves:
  - No measurable latency regression from Phase 2 single-pane performance
  - All 195+ existing tests still pass
  - Clippy clean across entire workspace
  - Manual verification of all Phase 3 success criteria
files_touched:
  - arcterm-app/src/main.rs (latency-trace instrumentation only)
tdd: false
---

# PLAN-3.4 -- Performance Verification and Phase 3 Acceptance

## Goal

Verify that Phase 3's multiplexer adds no measurable latency regression over Phase 2's single-pane performance, and confirm all Phase 3 success criteria from ROADMAP.md are met. This is the final plan in the phase.

## Why Wave 3 (after 3.1)

PLAN-3.1 must be complete (the integrated multi-pane app must be running) before performance can be measured. This plan runs after PLAN-3.1; it may run in parallel with PLAN-3.2 and PLAN-3.3 since it tests core functionality, not Neovim or palette features.

## Tasks

<task id="1" files="arcterm-app/src/main.rs" tdd="false">
  <action>Add multi-pane latency instrumentation and run performance measurements:

1. In `about_to_wait`, under the `#[cfg(feature = "latency-trace")]` guard, add timing around the multi-pane PTY poll loop: log the total time to poll all N pane channels. Log the pane count.
2. In `RedrawRequested`, under the latency-trace guard, log the time for `render_multipane` and the pane count. Compare to the single-pane `render_frame` time logged in Phase 2.
3. Run `RUST_LOG=debug cargo run --features latency-trace -p arcterm-app` with:
   a. Single pane: measure key-to-screen latency (type a character, measure from KeyboardInput to frame present). Record baseline.
   b. 4 panes (2x2 split): measure the same latency. Verify it is within 1ms of the single-pane baseline.
   c. Fast output test: in one pane, run `cat /dev/urandom | head -c 1M | base64`. Verify no frame drops or hangs. Other panes should remain responsive.
4. Record results in the plan execution summary.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --features latency-trace -p arcterm-app 2>&1 | tail -5</verify>
  <done>Latency instrumentation compiles. Performance measurements show no measurable regression: single-pane and 4-pane key-to-screen latency are both under 16ms. Fast output in one pane does not affect responsiveness of other panes.</done>
</task>

<task id="2" files="" tdd="false">
  <action>Run the full test suite and clippy across the entire workspace:

1. `cargo test --workspace` -- all 195+ existing tests must pass, plus new tests from Phase 3 plans (layout, tab, keymap, palette, neovim).
2. `cargo clippy --workspace -- -D warnings` -- zero warnings.
3. `cargo build --release -p arcterm-app` -- release build succeeds (catches any debug-only code paths).</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --workspace 2>&1 | tail -20 && cargo clippy --workspace -- -D warnings 2>&1 | tail -10 && cargo build --release -p arcterm-app 2>&1 | tail -5</verify>
  <done>All tests pass (195+ existing + new Phase 3 tests). Clippy reports zero warnings. Release build succeeds.</done>
</task>

<task id="3" files="" tdd="false">
  <action>Manual acceptance testing against all Phase 3 success criteria from ROADMAP.md:

1. **Horizontal and vertical splits create independent PTY-backed panes**: Press Leader+n (Ctrl+a, n) to create horizontal split. Press Leader+v to create vertical split. Verify each pane has its own independent shell session. Type different commands in each pane.

2. **Ctrl+h/j/k/l navigates between panes**: With 4 panes (2x2), verify Ctrl+h moves left, Ctrl+l moves right, Ctrl+k moves up, Ctrl+j moves down. Verify focus indicator (border color) changes.

3. **Neovim-aware pane crossing**: Launch Neovim with `nvim --listen /tmp/arcterm-test.sock` in one pane. Create splits inside Neovim (`:vsplit`, `:split`). Verify Ctrl+h/j/k/l traverses Neovim splits first. Verify that when at the edge of Neovim's splits, the key crosses to the adjacent arcterm pane.

4. **Tabs group pane layouts**: Press Leader+t to create a new tab. Verify the tab bar shows two tabs. Switch tabs with Leader+1 and Leader+2. Verify each tab has independent pane layouts.

5. **Leader+n/q/z keybindings**: Leader+n splits, Leader+q closes (verify pane is removed and sibling expands), Leader+z zooms (verify focused pane fills the window, press again to unzoom).

6. **Pane resize**: Leader+arrow resizes panes. Mouse drag on border resizes panes.

7. **No latency regression**: Subjective assessment during all tests above -- typing responsiveness should feel identical to Phase 2.

Record pass/fail for each criterion.</action>
  <verify>echo "Manual acceptance testing -- no automated verification command. Results recorded in execution summary."</verify>
  <done>All 7 Phase 3 success criteria pass manual verification. The multiplexer is fully functional with splits, tabs, navigation, Neovim integration, zoom, resize, and no perceptible latency regression.</done>
</task>
