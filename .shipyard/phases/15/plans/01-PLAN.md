---
phase: event-handling-exit-hardening
plan: 01
wave: 1
dependencies: []
must_haves:
  - Reader thread EOF sends final wakeup signal before breaking
  - Last pane exit calls event_loop.exit() immediately — no banner, no extra cycle
  - PresentMode logged at startup with Fifo availability check
files_touched:
  - arcterm-app/src/terminal.rs
  - arcterm-app/src/main.rs
  - arcterm-render/src/gpu.rs
tdd: false
---

# PLAN-01 — EOF Wakeup, Auto-Close Exit, Fifo Logging

Three independent fixes that touch different code paths and share no logical dependencies.

## Tasks

<task id="1" files="arcterm-app/src/terminal.rs" tdd="false">
  <action>
    Clone `wakeup_tx` for the PTY reader thread BEFORE `wakeup_tx` is moved into the `ArcTermEventListener` (~line 284). The listener takes ownership of `wakeup_tx`, so the clone must happen earlier.

    Specifically:
    1. At ~line 284, BEFORE creating the `ArcTermEventListener`, add: `let wakeup_tx_for_reader = wakeup_tx.clone();`
    2. Move `wakeup_tx_for_reader` into the reader thread closure (~line 362).
    3. In the `Ok(0)` arm (line 374), before `break`, add: `let _ = wakeup_tx_for_reader.send(());`
    4. Also send on the `Err(e)` break path (line 388) for symmetry.

    IMPORTANT: `wakeup_tx` is moved into `ArcTermEventListener` at ~line 289. The clone MUST happen before that move.
  </action>
  <verify>cargo build -p arcterm-app 2>&1 | tail -5</verify>
  <done>Reader thread sends wakeup on both EOF and error break paths. `cargo build` succeeds with no warnings on the changed file.</done>
</task>

<task id="2" files="arcterm-app/src/main.rs" tdd="false">
  <action>
    Remove the exit banner and make last-pane exit immediate. Three changes in `main.rs`:

    1. **Pane removal loop (~line 1682-1687):** When `state.panes.is_empty()` is detected, call `event_loop.exit(); return;` directly instead of setting `shell_exited = true` and requesting a redraw. This exits in the same `about_to_wait` cycle.

    2. **Remove banner rendering (~line 2079-2131):** Delete the entire `if state.shell_exited` block in the `RedrawRequested` handler that constructs the "Shell exited — press any key to close" banner overlay. This code is dead once step 1 is done.

    3. **Remove keyboard exit handler (~line 2394-2399):** Delete the `if state.shell_exited { event_loop.exit(); return; }` block in the `KeyboardInput` handler — no banner means no "press any key" flow.

    4. **Remove `shell_exited` field:** Remove the `shell_exited: bool` field from `AppState` (~line 560), its initialization to `false` (~lines 975, 1225), and the early return at `about_to_wait` line 1269-1272.

    Note: `about_to_wait` needs access to `event_loop` for the `exit()` call. The method signature already receives `event_loop: &ActiveEventLoop`, and the pane removal loop is inside `about_to_wait`, so `event_loop.exit()` is callable there.
  </action>
  <verify>cargo build -p arcterm-app 2>&1 | tail -5 && grep -c "shell_exited" arcterm-app/src/main.rs | grep "^0$"</verify>
  <done>`shell_exited` field fully removed. `exit` in a single-pane session calls `event_loop.exit()` in the same `about_to_wait` iteration. No banner code remains. Build succeeds.</done>
</task>

<task id="3" files="arcterm-render/src/gpu.rs" tdd="false">
  <action>
    Upgrade PresentMode logging from `log::debug!` to `log::info!` and add Fifo availability check. At line ~53-58 in `GpuState::new()`:

    1. After `let caps = surface.get_capabilities(&adapter);`, check if Fifo is supported: `let fifo_available = caps.present_modes.contains(&wgpu::PresentMode::Fifo);`
    2. If not available, log a warning: `log::warn!("PresentMode::Fifo not in supported modes {:?}; frame pacing may be degraded", caps.present_modes);`
    3. Change existing `log::debug!("wgpu present mode: {:?}", present_mode);` to `log::info!("wgpu present mode: {:?} (fifo supported: {})", present_mode, fifo_available);`
  </action>
  <verify>cargo build -p arcterm-render 2>&1 | tail -5</verify>
  <done>Startup logs the actual PresentMode at `info` level and warns if Fifo is unavailable. Build succeeds.</done>
</task>
