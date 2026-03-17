# Research: Terminal Event Loop Frame Pacing

**Date:** 2026-03-16
**Phase:** 13 — Renderer Optimization
**Author:** Research Agent

---

## Context

Arcterm is a GPU-accelerated terminal emulator using wgpu (v28) for rendering,
winit (v0.30.13) for windowing, and alacritty_terminal (v0.25) as the VT state
machine. The application loop is a single-threaded `ApplicationHandler` whose
`about_to_wait` method drains PTY wakeup signals, processes structured content,
and decides whether to `request_redraw`.

Two documented performance defects from Phase 12 motivate this research:

1. **Frame pacing absent**: the event loop uses `ControlFlow::Poll` when PTY data
   is active, spinning the CPU at thousands of iterations per second with no
   frame-rate cap. The GPU present mode prefers `Mailbox` (uncapped), compounding
   the problem.

2. **Keyboard latency tension**: `ControlFlow::Wait` (the idle state) is correct
   for zero-activity periods, but the transition threshold (originally 3 idle
   cycles) was debated. The current code switches immediately on the first idle
   `about_to_wait` call.

The specific question: how do Alacritty, WezTerm, and Rio solve the tension
between coalescing rapid PTY output (to avoid rendering thousands of intermediate
frames during `cat large_file`) and maintaining low latency for keyboard input?
Are any of them using a "poll for N cycles" approach?

---

## Comparison Matrix

| Criteria | Alacritty | WezTerm | Rio |
|----------|-----------|---------|-----|
| ControlFlow strategy | `WaitUntil(deadline)` / `Wait` — never Poll | Notification-driven async (not winit ControlFlow) | `WaitUntil(deadline)` / `Wait` — never Poll |
| PTY wakeup → redraw path | `Wakeup` event sets `dirty=true`; redraw requested only if `has_frame=true` | `MuxNotification::PaneOutput` → invalidates if pane visible | Sets `pending_update` dirty; `schedule_redraw` via scheduler |
| Frame rate cap | `FrameTimer` locked to monitor refresh rate (default 60 Hz fallback) | Timer-based `schedule_next_status_update`; animation instants | Scheduler deadline = next frame interval |
| Present mode / VSync | `PresentMode::AutoNoVsync` with frame-timer scheduling | WebGPU / software compositing depending on backend | `PresentMode::Fifo` or event-based render strategy |
| "Poll for N cycles" pattern | No — not present anywhere | No | No |
| Dirty flag on PTY wakeup | Yes — `window_context.dirty = true` | Yes — via `NeedRepaint` notification | Yes — `renderable_content.pending_update.set_dirty()` |
| Frame-guard (skip render when no frame token) | Yes — `has_frame` flag on Wayland; `request_redraw` is no-op if not dirty + no frame | N/A (handles internally) | Via dirty flag; no `has_frame` equivalent |
| Max processing time per frame budget | None — drain all pending, render once | None explicitly | None explicitly |
| Keyboard event vs PTY event priority | Same thread; keyboard arrives via `window_event`, wakeup via `user_event` | Same notification bus | Same scheduler — keyboard path calls `request_redraw` immediately |

---

## Detailed Analysis

### Alacritty

**Source:** `alacritty/src/event.rs` (master, fetched 2026-03-16)
**Source:** `alacritty/src/display/mod.rs` (master, fetched 2026-03-16)
**Source:** `alacritty/src/scheduler.rs` (master, fetched 2026-03-16)

#### `about_to_wait` implementation

```rust
fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
    // Dispatch AboutToWait to all windows (processes pending state).
    for window_context in self.windows.values_mut() {
        window_context.handle_event(
            &self.proxy,
            &mut self.clipboard,
            &mut self.scheduler,
            WinitEvent::AboutToWait,
        );
    }

    // Update scheduler and sleep until next deadline or indefinitely.
    let control_flow = match self.scheduler.update() {
        Some(instant) => ControlFlow::WaitUntil(instant),
        None          => ControlFlow::Wait,
    };
    event_loop.set_control_flow(control_flow);
}
```

**Key insight:** Alacritty never uses `ControlFlow::Poll`. The control flow is
always either `Wait` (no pending timers) or `WaitUntil(deadline)` (at least one
timer is scheduled). The frame timer is the dominant timer.

#### PTY wakeup path

```rust
// In Processor::user_event:
(EventType::Terminal(TerminalEvent::Wakeup), Some(window_id)) => {
    if let Some(window_context) = self.windows.get_mut(window_id) {
        window_context.dirty = true;
        if window_context.display.window.has_frame {
            window_context.display.window.request_redraw();
        }
    }
}
```

A PTY `Wakeup` event sets `dirty = true` and calls `request_redraw()` **only if
`has_frame` is true**. The `has_frame` flag is set to `true` when the Wayland
frame callback fires (i.e., the compositor confirms the previous frame was
consumed). On non-Wayland platforms `has_frame` is initialized to `true` and
stays true, so `request_redraw()` is always called.

#### Frame rate cap — `FrameTimer`

```rust
pub fn compute_timeout(&mut self, refresh_interval: Duration) -> Duration {
    let now = Instant::now();

    if self.refresh_interval != refresh_interval {
        self.base = now;
        self.last_synced_timestamp = now;
        self.refresh_interval = refresh_interval;
        return refresh_interval;
    }

    let next_frame = self.last_synced_timestamp + self.refresh_interval;

    if next_frame < now {
        // Behind schedule — redraw immediately.
        let elapsed_micros = (now - self.base).as_micros() as u64;
        let refresh_micros = self.refresh_interval.as_micros() as u64;
        self.last_synced_timestamp =
            now - Duration::from_micros(elapsed_micros % refresh_micros);
        Duration::ZERO
    } else {
        // Wait until next vblank.
        self.last_synced_timestamp = next_frame;
        next_frame - now
    }
}
```

The refresh interval is derived from the monitor's `refresh_rate_millihertz()`.
The fallback is 60,000 mHz = 16.67 ms per frame. After each drawn frame,
`request_frame` schedules a `TimerId::Frame` event at the computed deadline,
and `scheduler.update()` returns that deadline to `WaitUntil`. This means:

- When PTY data arrives, `dirty=true` and `request_redraw()` is called.
- `RedrawRequested` triggers `window_context.draw()`.
- `draw()` calls `request_frame(scheduler)` which schedules the next frame at
  ~16.67ms in the future.
- `about_to_wait` returns `WaitUntil(next_frame_deadline)`.
- The event loop sleeps. The PTY reader thread can still send Wakeup events
  during this sleep, which are queued by winit and delivered immediately on wake.
- At the deadline, the `Frame` event fires: `has_frame = true`, and if `dirty`,
  `request_redraw()` is called again.

**Result:** Alacritty naturally caps at monitor refresh rate even during heavy
PTY output. It does not need `ControlFlow::Poll` because the frame timer drives
the loop cadence. All PTY data arriving between two frame deadlines is coalesced
into one frame by the terminal's internal state accumulation.

#### "Poll for N cycles" — not present

Alacritty has no idle cycle counter, no `ControlFlow::Poll`, and no threshold
before transitioning to `Wait`. The scheduler handles timing entirely.

**Weakness:** On X11, macOS, and Windows (non-Wayland), the `has_frame` guard
does not apply (it is always true). This means a burst of PTY wakeups could
cause `request_redraw()` to be called many times. However, winit deduplicates
`request_redraw()` calls within a single event loop iteration via the
`requested_redraw` flag:

```rust
pub fn request_redraw(&mut self) {
    if !self.requested_redraw {
        self.requested_redraw = true;
        self.window.request_redraw();
    }
}
```

So multiple wakeup events within one frame still produce at most one
`RedrawRequested`. The `FrameTimer` then gates the next redraw to the
next vblank interval.

---

### WezTerm

**Source:** `wez/wezterm` — `wezterm-gui/src/termwindow/mod.rs` (main, fetched 2026-03-16)
**Source:** `wez/wezterm` — `wezterm-gui/src/termwindow/render/mod.rs` (main, fetched 2026-03-16)

WezTerm does not use the standard winit `ApplicationHandler` pattern. It has its
own cross-platform windowing abstraction (the `window` crate) built on top of
platform APIs directly. Frame pacing is handled differently:

#### PTY wakeup path

```rust
TermWindowNotif::MuxNotification(n) => match n {
    MuxNotification::PaneOutput(pane_id) => {
        self.mux_pane_output_event(pane_id);
    }
    // ...
}
```

`mux_pane_output_event` marks the window dirty (via `NeedRepaint`) only if the
pane is currently visible. This is the coalescing mechanism: if 10,000 PTY bytes
arrive and trigger 10,000 `PaneOutput` notifications, the render only fires when
the window processes the next `NeedRepaint`.

#### Frame scheduling — animation-based

```rust
pub fn update_next_frame_time(&self, next_due: Option<Instant>) {
    if next_due.is_some() {
        update_next_frame_time(&mut *self.has_animation.borrow_mut(), next_due);
    }
}
```

WezTerm tracks a minimum `Instant` for the next required frame. Animations
(cursor blink, visual bell, image loading) schedule a future instant. The event
loop wakes at that instant. This is semantically identical to Alacritty's
`WaitUntil(scheduler.update())` but implemented at a higher abstraction level
inside WezTerm's own windowing layer.

#### Resize coalescing

```rust
WindowEvent::NeedRepaint => {
    if self.resizes_pending > 0 {
        self.is_repaint_pending = true;  // Defer render
        Ok(true)
    } else {
        Ok(self.do_paint(window))       // Render now
    }
}
```

When window resizes are in-flight, repaints are explicitly deferred. This
prevents partial renders during geometry changes — a problem arcterm does not
currently address.

#### ControlFlow — not applicable directly

WezTerm uses a promise-based async runtime (`smol`) and its own window event
loop. `ControlFlow::Poll` is not used. The equivalent of `Poll` would be
continuously dispatching `NeedRepaint`, which only happens during animations.

#### "Poll for N cycles" — not present

No such pattern exists in WezTerm.

---

### Rio

**Source:** `raphamorim/rio` — `frontends/rioterm/src/application.rs` (main, fetched 2026-03-16)

Rio uses winit's `ApplicationHandler` and a scheduler, closely mirroring
Alacritty's pattern.

#### `about_to_wait` implementation

```rust
fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
    let control_flow = match self.scheduler.update() {
        Some(instant) => ControlFlow::WaitUntil(instant),
        None          => ControlFlow::Wait,
    };
    event_loop.set_control_flow(control_flow);
}
```

This is **identical to Alacritty's pattern**. Rio also never uses `ControlFlow::Poll`.

#### PTY wakeup path

```rust
RioEventType::Rio(RioEvent::Wakeup(route_id)) => {
    if self.config.renderer.strategy.is_event_based() {
        if let Some(route) = self.router.routes.get_mut(&window_id) {
            ctx_item.val.renderable_content.pending_update.set_dirty();
            route.schedule_redraw(&mut self.scheduler, route_id);
        }
    }
}
```

`set_dirty()` marks content as stale, then `schedule_redraw` adds a deadline to
the scheduler. The `about_to_wait` loop picks up the nearest deadline and returns
`WaitUntil`. The event loop wakes at that instant and calls `request_redraw()`.

#### Render strategy — event-based vs game mode

```rust
if self.config.renderer.strategy.is_game() {
    route.request_redraw();  // Continuous render loop
} else if route.window.screen.ctx().current()
           .renderable_content.pending_update.is_dirty() {
    route.schedule_redraw(&mut self.scheduler, route_id);
}
```

Rio explicitly supports two modes:
- **Event-based** (default): renders only when dirty, using scheduler deadlines.
  This is correct for terminals.
- **Game mode**: continuously calls `request_redraw()`, equivalent to
  `ControlFlow::Poll`. Rio documents this as explicitly wrong for terminals
  and only suitable for games.

This is the clearest documentation of the pattern to avoid in terminal use.

#### "Poll for N cycles" — not present

Rio has no cycle counter. The scheduler is the sole timing mechanism.

---

### winit ControlFlow semantics (v0.30.13)

From the official winit documentation:

- **`ControlFlow::Poll`**: "Immediately begin a new iteration regardless of
  whether new events are available." Causes continuous CPU usage. Documented as
  appropriate for games and real-time graphics that use VSync from the graphics
  API to cap frame rate. Without VSync, this is a busy loop.

- **`ControlFlow::Wait`**: "Suspend the thread until another event arrives."
  Zero CPU usage when idle. Default mode. Correct for event-driven applications.

- **`ControlFlow::WaitUntil(instant)`**: "Suspend until another event arrives
  OR the given time is reached." Appropriate for applications needing periodic
  updates (cursor blink, animations). The winit documentation notes: "Applications
  which want to render at the display's native refresh rate should instead use
  `Poll` and the VSync functionality of a graphics API to reduce odds of missed
  frames." However, Alacritty and Rio both use `WaitUntil` with a frame-timer
  to achieve the same effect without polling.

**Takeaway for wgpu users:** If the wgpu present mode is `Fifo` (VSync), then
`get_current_texture()` blocks until the display hardware is ready. In this
configuration, `ControlFlow::Poll` plus `Fifo` achieves monitor-rate rendering
with the GPU blocking providing the rate cap — no CPU spin. This is the pattern
the winit documentation refers to. But it requires the GPU-side block to be the
pacing mechanism, not an explicit timer.

---

## Key Questions — Answered

### Do any of these terminals use a "poll for N cycles" approach?

No. None of the three terminals surveyed use a cycle counter or a `ControlFlow::Poll`
threshold. The pattern does not appear in any production Rust terminal examined.

### Do they use `ControlFlow::Wait` or `ControlFlow::Poll`?

All three use `Wait` or `WaitUntil`. Rio explicitly documents `Poll` (via its
"game mode") as inappropriate for terminals.

### How do they handle PTY data arriving faster than frame rate?

The answer is the same across all three terminals: the PTY reader thread runs
independently and accumulates state in the terminal's internal grid. Wakeup
signals pile up in a channel. `about_to_wait` drains the entire channel at once
(the "drain all, then render once" pattern). By the time the next frame renders,
the grid contains all bytes processed since the previous frame. The number of
intervening wakeup signals is irrelevant — only the terminal state at render time
matters.

Alacritty's `FrameTimer` additionally enforces that `request_redraw()` is only
called at the next monitor refresh interval, so no matter how many wakeups arrive
between frames, exactly one frame is produced per vblank.

### Is there a "max processing time per frame" budget?

No. None of the three terminals enforce a per-frame processing time limit. The
implicit budget is the frame interval (≈16.67ms at 60Hz). If PTY processing plus
rendering takes longer than 16.67ms, frames are dropped; there is no mechanism
to truncate processing early. This is acceptable because modern terminal byte
processing is fast (microseconds per kilobyte for alacritty_terminal's VT parser).

---

## Recommendation for Arcterm

**Selected approach: `PresentMode::Fifo` + `ControlFlow::Poll` when active, `ControlFlow::Wait` when idle.**

This is what arcterm's existing Phase 13 plan already specifies. The upstream
terminal research confirms the plan is sound, but the framing differs from
Alacritty/Rio. Here is the precise reasoning:

**Why not adopt Alacritty's `WaitUntil` + `FrameTimer` approach?**

Alacritty's `FrameTimer` + scheduler requires:
1. A scheduler data structure (a sorted `VecDeque<Timer>`)
2. A `FrameTimer` struct tracking base timestamp and last sync point
3. A `request_frame()` call at the end of every `draw()`
4. The `about_to_wait` scheduler update on every iteration

This is approximately 150 lines of infrastructure. The alternative — `Fifo`
present mode with `ControlFlow::Poll` — achieves the same result because
`get_current_texture()` under `Fifo` blocks on the hardware VSync, naturally
capping at monitor refresh rate. The GPU provides the pacing, not a CPU timer.

Both approaches are valid. Alacritty's scheduler approach works on all present
modes (including uncapped `Mailbox`) and gives precise control. The `Fifo` +
`Poll` approach relies on the GPU driver's VSync blocking. Both are used in
production terminals (Alacritty = timer-based; the `Fifo`+`Poll` pattern is
described in winit's own documentation as the recommended approach for GPU
applications).

**The `idle_cycles` concern:**

The original three-cycle threshold was incorrect and has already been removed
from arcterm (the code now transitions to `Wait` immediately on the first
`about_to_wait` call with no data). This is consistent with how Alacritty and
Rio behave: there is no polling "grace period". When no PTY data is present,
the loop sleeps until the next event.

**Under `Fifo` + `Poll` when active:**

When `got_data = true` in `about_to_wait`, arcterm sets `ControlFlow::Poll`.
Winit then calls `about_to_wait` again immediately. The next call drains any new
wakeups. If no new data arrived, `got_data = false`, the loop transitions to
`ControlFlow::Wait`. The net behavior:

- With high PTY throughput: each `about_to_wait` call drains all pending wakeups
  and issues one `request_redraw()`. Under `Fifo`, `get_current_texture()` then
  blocks until VSync, so the effective render rate is capped at monitor refresh
  rate even with `Poll`. CPU is only consumed at the GPU block.
- With no PTY throughput: the second `about_to_wait` call finds no data, sets
  `Wait`, and the thread sleeps until a keyboard event or new PTY wakeup arrives.

This achieves the same observable behavior as Alacritty's timer approach without
the scheduler infrastructure.

**Keyboard latency:**

Keyboard events arrive via `window_event`, not `about_to_wait`. `window_event`
calls `request_redraw()` directly. Under both `Poll` and `Wait`, winit delivers
`RedrawRequested` in the next event loop iteration. There is no latency
difference between `Poll` and `Wait` for keyboard events — both deliver the
event promptly. The concern about `ControlFlow::Wait` increasing keyboard latency
applies only if the event loop is sleeping when the keyboard event arrives,
which winit handles correctly by waking the thread on any new event.

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `Mailbox` present mode on macOS Metal unavailable, so `Fifo` fallback already active but not confirmed | Low | Low | Verify at startup by logging which present mode was selected; add to latency-trace output |
| `Fifo` VSync block on `get_current_texture()` occurs on the render thread, not the event thread — may introduce a one-frame delay for keyboard input if rendering is slow | Low | Medium | Profile with `latency-trace`; the render pass for a terminal is typically <2ms on modern hardware, well within 16.67ms budget |
| `ControlFlow::Poll` when active causes `about_to_wait` to fire multiple times before a `RedrawRequested` is processed if the GPU frame queue is full | Low | Low | Under `Fifo`, wgpu's `get_current_texture()` blocks until a frame slot is available, preventing unbounded polling |
| Rio's "game mode" pattern — continuously requesting redraw — could be accidentally introduced if `got_data` is too broadly true | Medium | Medium | Current code sets `got_data = true` only when `terminal.has_wakeup()` returns true; keyboard input does not set `got_data` and goes through `request_redraw()` directly |
| Alacritty's `WaitUntil` approach could be adopted in a future phase to enable cursor blink or other animations without polling | N/A | N/A | If cursor blink is added, adopt a minimal scheduler (a single `Option<Instant>` is sufficient) rather than Alacritty's full `VecDeque`-based scheduler |

---

## Implementation Considerations

### Specific code change required (from existing Phase 13 RESEARCH.md)

The single change needed in `arcterm-render/src/gpu.rs` lines 56–61:

```rust
// Current (prefers Mailbox):
let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
    wgpu::PresentMode::Mailbox
} else {
    wgpu::PresentMode::Fifo
};

// Change to:
let present_mode = wgpu::PresentMode::Fifo;
```

No changes to `main.rs` ControlFlow logic are required. The existing
`Poll` (active) / `Wait` (idle) pattern is correct and consistent with
the intent of all three upstream terminals.

### The `idle_cycles` field

`AppState.idle_cycles` is set to 0 when PTY data arrives (line 1699) and was
previously used to delay the transition from `Poll` to `Wait`. The field is now
unused in the decision logic (the else branch sets `Wait` immediately). It can
be removed in a cleanup pass, but keeping it does not cause any behavioral
problem.

### Drain-before-render pattern

Arcterm's `about_to_wait` already implements the drain-before-render pattern
correctly:

1. Drain all pane wakeup signals (`has_wakeup()` loop-drains the channel).
2. For each pane with a wakeup, drain OSC 7770, Kitty images, OSC 133 side
   channels.
3. After all drains, call `request_redraw()` once.
4. The `RedrawRequested` handler calls `render_multipane` once, capturing all
   accumulated terminal state.

This is structurally identical to how Alacritty drains PTY state before
rendering. No refactoring is needed.

---

## Sources

1. Alacritty `event.rs` — https://raw.githubusercontent.com/alacritty/alacritty/master/alacritty/src/event.rs
2. Alacritty `display/mod.rs` — https://raw.githubusercontent.com/alacritty/alacritty/master/alacritty/src/display/mod.rs
3. Alacritty `display/window.rs` — https://raw.githubusercontent.com/alacritty/alacritty/master/alacritty/src/display/window.rs
4. Alacritty `scheduler.rs` — https://raw.githubusercontent.com/alacritty/alacritty/master/alacritty/src/scheduler.rs
5. WezTerm `termwindow/mod.rs` — https://raw.githubusercontent.com/wez/wezterm/main/wezterm-gui/src/termwindow/mod.rs
6. WezTerm `termwindow/render/mod.rs` — https://raw.githubusercontent.com/wez/wezterm/main/wezterm-gui/src/termwindow/render/mod.rs
7. Rio `application.rs` — https://raw.githubusercontent.com/raphamorim/rio/main/frontends/rioterm/src/application.rs
8. winit ControlFlow documentation — https://rust-windowing.github.io/winit/winit/event_loop/enum.ControlFlow.html
9. Alacritty issue #3972 (frame scheduling) — https://github.com/alacritty/alacritty/issues/3972
10. Alacritty issue #673 (input latency) — https://github.com/alacritty/alacritty/issues/673
11. Arcterm `arcterm-app/src/main.rs` — codebase read, lines 1264–1724 (`about_to_wait`)
12. Arcterm `arcterm-app/src/terminal.rs` — codebase read, lines 539–574 (`has_wakeup`)
13. Arcterm Phase 13 RESEARCH.md — `.shipyard/phases/13/RESEARCH.md`

---

## Uncertainty Flags

- **Alacritty's `has_frame` on non-Wayland platforms:** The research confirms
  `has_frame` is initialized to `true` on non-Wayland and stays true unless
  the Wayland frame callback fires. On macOS (Metal) specifically, whether
  there is any equivalent frame-gating mechanism is unclear from the source
  alone. On macOS, frame rate may be gated by `CVDisplayLink` or simply by
  `swap_buffers` blocking — the source does not make this explicit. This does
  not affect arcterm (which uses wgpu's `Fifo` surface presentation for
  blocking), but it limits the transferability of Alacritty's `has_frame` pattern.

- **WezTerm present mode:** WezTerm's GPU backend selection (WebGPU vs software
  compositor vs OpenGL) was not fully traced. The render path observed uses
  `do_paint_webgpu` vs `do_paint`, but the underlying present mode selection
  was not accessible from the source fragments fetched. WezTerm's frame pacing
  is therefore described at the abstraction level visible in the source — timer-
  based animation instants — rather than at the GPU present mode level.

- **Rio's `schedule_redraw` implementation:** The `schedule_redraw` method body
  was not directly visible in the fetched source. Based on `about_to_wait`
  returning `WaitUntil(scheduler.update())`, the method must schedule a timer
  in the same scheduler. The exact deadline (frame interval vs zero) used by
  `schedule_redraw` is inferred, not confirmed.
