# Phase 6 Plan Review Critique

**Reviewer:** Verification Agent
**Date:** 2026-03-15
**Plans Reviewed:** 3 (PLAN-1.1, PLAN-2.1, PLAN-3.1)

---

## Summary Verdict

**CONDITIONAL PASS** -- Plans are well-structured with concrete, testable actions. However, there is **one critical gap**: the `CommandExecuted` and `WorkspaceSwitched` event emission is declared as a must-have but not fully specified in task actions. Plans are otherwise execution-ready pending clarification of this gap.

---

## Coverage of Roadmap Success Criteria

| Criterion | Addressed By | Status |
|-----------|--------------|--------|
| 1. Hello world WASM <50ms, text render, keyboard | PLAN-1.1 Task 3, PLAN-3.1 Task 2 | PASS |
| 2. Manifest TOML declares permissions, sandbox enforced | PLAN-2.1 Tasks 1-2 | PASS |
| 3. Events: pane opened/closed, command executed, workspace switched | PLAN-2.1 must-have #10, but Task 3 incomplete | **FAIL** |
| 4. CLI commands: install/list/remove/dev | PLAN-2.1 Task 3 | PASS |
| 5. System monitor example plugin | PLAN-3.1 Task 3 | PASS |
| 6. Memory overhead <10MB per plugin | PLAN-1.1 Task 2 | PASS |

---

## Detailed Plan Reviews

### PLAN-1.1: WIT Interface Definition + Wasmtime Host Runtime

**Wave:** 1 (correct -- no dependencies)
**Dependencies:** `[]` (correct)
**Task Count:** 3 (within limit)

**Structure Assessment:** PASS

- Task 1: WIT file creation + Cargo dependencies. Concrete, testable.
- Task 2: Host runtime (`PluginRuntime`, `PluginHostData`, host trait impl). Concrete.
- Task 3: Integration test with timing measurement. TDD-driven. Concrete.

**Verification Commands:** All are concrete and runnable.

- Task 1 verify: `cargo check -p arcterm-plugin` -- validates syntax
- Task 2 verify: `cargo check -p arcterm-plugin` -- validates compilation
- Task 3 verify: `cargo test -p arcterm-plugin` -- runs integration test

**Design Quality:** Excellent.

- WIT world definition is language-agnostic and enforces contract.
- `StoreLimits { max_memory_size: Some(10 * 1024 * 1024) }` directly satisfies success criterion 6 (memory overhead <10MB).
- Use of `wasmtime::component::bindgen!` is idiomatic.
- Epoch interruption for timeout support is good safety practice.

**Risks Identified:** None significant. Hand-written WAT component in test avoids heavyweight build-time dependencies.

---

### PLAN-2.1: Plugin Manifest, Permission Sandbox, Event Bus, and CLI

**Wave:** 2 (correct -- depends on Wave 1)
**Dependencies:** `["1.1"]` (correct)
**Task Count:** 3 (within limit)

**Structure Assessment:** CONDITIONAL PASS

**Must-Haves Check:**

| Must-Have | Task | Addressed | Notes |
|-----------|------|-----------|-------|
| PluginManifest struct | Task 1 | YES | Fully specified |
| WasiCtxBuilder configuration | Task 1 | YES | Fully specified |
| PluginManager with HashMap + broadcast::Sender | Task 2 | YES | Fully specified |
| Event emission from AppState (PaneOpened, PaneClosed, CommandExecuted, WorkspaceSwitched) | Task 3 | **PARTIAL** | Only PaneOpened and PaneClosed are in the action field |
| Plugin instances receive events via broadcast::Receiver | Task 2 | YES | Event listener task architecture specified |
| CLI subcommands (install, list, remove, dev) | Task 3 | YES | All four commands in action |
| Plugin storage path | Task 2-3 | YES | `~/.config/arcterm/plugins/<name>/` |

**Critical Gap Identified:**

In the must_haves section (line 10), the plan declares:
> Event emission from AppState (PaneOpened, PaneClosed, CommandExecuted, WorkspaceSwitched) routed to plugin instances

However, in PLAN-2.1 Task 3 action field, only two of these four are mentioned:
- `event_tx.send(PluginEvent::PaneOpened { pane_id })` -- explicitly stated
- `event_tx.send(PluginEvent::PaneClosed { pane_id })` -- explicitly stated
- `CommandExecuted` -- **NOT in Task 3 action**
- `WorkspaceSwitched` -- **NOT in Task 3 action**

**Where Should These Events Be Emitted?**

The action field does not specify:
- Where to emit `CommandExecuted` -- presumably in a PTY read path when a command completes and exit code is received
- Where to emit `WorkspaceSwitched` -- presumably in tab/workspace navigation handlers
- These emission points exist in Phase 5 (workspace switching) but are not mentioned in Phase 6 Task 3

**Verification Commands:** Concrete and runnable, but incomplete.

- Task 1 verify: `cargo test -p arcterm-plugin manifest` -- tests manifest parsing only
- Task 2 verify: `cargo test -p arcterm-plugin manager` -- tests event dispatch, but which events?
- Task 3 verify: `cargo build && ./target/debug/arcterm-app plugin list` -- validates CLI build, but does not verify event emission completeness

**Recommendation:** Task 3's action field should explicitly state:
1. Where `CommandExecuted` event is emitted (e.g., "when a pane's PTY exits or command completes")
2. Where `WorkspaceSwitched` event is emitted (e.g., "when user tabs to a different workspace")
3. These two events should be added to the verify command or a separate integration test

---

### PLAN-3.1: Plugin Rendering Integration, Keyboard Input, and Example Plugins

**Wave:** 3 (correct -- depends on Waves 1 and 2)
**Dependencies:** `["1.1", "2.1"]` (correct)
**Task Count:** 3 (within limit)

**Structure Assessment:** PASS

- Task 1: Integration of plugin panes into rendering and input paths. Concrete.
- Task 2: Hello-world example plugin. Concrete and testable.
- Task 3: System monitor example plugin. Concrete and testable.

**Design Quality:** Excellent.

- `PaneNode::PluginPane` variant cleanly extends existing layout tree.
- `prepare_plugin_pane` mirrors existing `prepare_overlay_text` pattern.
- Keyboard input routing is straightforward: KeyboardInput handler sends to focused plugin pane.
- Example plugins demonstrate end-to-end functionality.

**Must-Haves Verification:**

- `PaneNode::PluginPane` variant: Task 1 explicitly creates it
- Plugin draw buffer rendering: Task 1 implements `prepare_plugin_pane`
- Keyboard input forwarding: Task 1 explicitly routes KeyboardInput to PluginPane
- Hello-world example: Task 2, renders text + responds to keys
- System monitor example: Task 3, demonstrates full API (events, config, rendering, MCP tool registration)
- MCP tool registry: Task 1 adds `list_tools()` method to PluginManager

**Verification Commands:** All concrete and runnable.

- Task 1 verify: `cargo build -p arcterm-app` -- validates compilation
- Task 2 verify: `cargo build --target wasm32-wasip2 --release` -- compiles hello-world plugin
- Task 3 verify: `cargo build --target wasm32-wasip2 --release` -- compiles system-monitor plugin

**Dependency on Phase 5:** The plan mentions workspace switching in the design notes, but PLAN-3.1 does not directly emit WorkspaceSwitched events -- this is deferred to PLAN-2.1 Task 3, which is a Wave 2 plan.

---

## Wave Ordering and Dependencies

| Plan | Wave | Dependencies | Order OK? |
|------|------|-------------|-----------|
| PLAN-1.1 | 1 | none | YES |
| PLAN-2.1 | 2 | [1.1] | YES |
| PLAN-3.1 | 3 | [1.1, 2.1] | YES |

Dependency graph is correct and acyclic. Plans can execute in declared wave order.

---

## File Path and Forward Reference Validation

**Files Touched by PLAN-1.1:**
- `arcterm-plugin/Cargo.toml` -- new crate (valid)
- `arcterm-plugin/src/lib.rs` -- new module (valid)
- `arcterm-plugin/src/runtime.rs` -- new module (valid)
- `arcterm-plugin/src/host.rs` -- new module (valid)
- `arcterm-plugin/src/types.rs` -- new module (valid)
- `arcterm-plugin/wit/arcterm.wit` -- new WIT file (valid)
- `Cargo.toml` (workspace root) -- modified (valid)

**Files Touched by PLAN-2.1:**
- `arcterm-plugin/src/manifest.rs` -- new module (valid)
- `arcterm-plugin/src/manager.rs` -- new module (valid)
- `arcterm-plugin/src/lib.rs` -- already exists from PLAN-1.1 (valid)
- `arcterm-app/src/main.rs` -- existing file (valid)
- `arcterm-app/Cargo.toml` -- existing file (valid)

**Files Touched by PLAN-3.1:**
- `arcterm-app/src/layout.rs` -- existing file (valid)
- `arcterm-app/src/main.rs` -- already in PLAN-2.1 (valid)
- `arcterm-render/src/text.rs` -- existing file (valid)
- `arcterm-plugin/src/manager.rs` -- already in PLAN-2.1 (valid)
- `arcterm-plugin/wit/arcterm.wit` -- already in PLAN-1.1 (valid)
- `examples/plugins/hello-world/` -- new directory (valid)
- `examples/plugins/system-monitor/` -- new directory (valid)

**Forward References:** None detected. All referenced files either exist in prior phases or are created by earlier plans within Phase 6.

---

## Task Count and Scope

| Plan | Tasks | Scope | Assessment |
|------|-------|-------|------------|
| PLAN-1.1 | 3 | Foundations: WIT file, runtime infrastructure, integration test | BALANCED |
| PLAN-2.1 | 3 | Manifest parsing, event bus, CLI integration | BALANCED |
| PLAN-3.1 | 3 | Rendering integration, two example plugins | BALANCED |

All plans respect the "max 3 tasks per plan" guideline.

---

## TDD Compliance

| Plan | TDD Flag | Assessment |
|------|----------|------------|
| PLAN-1.1 | `true` | Task 3 is a unit/integration test verifying behavior |
| PLAN-2.1 | `true` | Task 1 has manifest parsing tests; Task 2 has event bus tests |
| PLAN-3.1 | `false` | Building examples; verification is by execution (pragmatic) |

TDD adoption is appropriate. Example plugin builds are verified by compilation, not unit tests (reasonable).

---

## Verification Command Audit

All 9 verify commands are:
- **Concrete:** Use actual commands, not "test manually" or "verify it works"
- **Runnable:** All are shell-executable Cargo commands or binary invocations
- **Path-correct:** Use absolute paths or correct relative paths
- **Measurable:** capture output via pipes and tail filters

Only exception: The done-fields are descriptive success criteria, not additional verification.

---

## Integration Risks

1. **PLAN-2.1 Task 3 Event Emission Gap:** The task does not specify where `CommandExecuted` and `WorkspaceSwitched` events are emitted. These are critical to success criterion 3. The done-field says "Event emission code compiles" but doesn't prove all four event types are actually sent.

2. **PLAN-3.1 Example Plugin Portability:** System monitor plugin uses `/proc` (Linux-specific). The action mentions "appropriate paths" for macOS but doesn't specify what those are. This could cause the plugin to silently fail on non-Linux systems.

3. **PLAN-1.1 Task 3 Timing Measurement:** The task requires measuring load time <50ms, but the verify command only shows tail output. The test itself must print timing, and the done-field should confirm it.

---

## Verdict by Category

| Category | Result | Notes |
|----------|--------|-------|
| **Coverage of Success Criteria** | CONDITIONAL FAIL | Missing explicit event emission for CommandExecuted and WorkspaceSwitched |
| **Plan Structure** | PASS | Wave ordering, dependencies, task count all correct |
| **Verification Commands** | PASS | All concrete and runnable |
| **File Paths** | PASS | No conflicts, no forward references |
| **Design Quality** | PASS | Architectures are sound and idiomatic |
| **TDD Compliance** | PASS | Appropriate use of tests |

---

## Recommendations Before Execution

### CRITICAL: Fix PLAN-2.1 Task 3

Update the action field to explicitly include:

```
...In the event loop's input handling path, when a command completes (shell prompt received),
emit PluginEvent::CommandExecuted { pane_id, command: String, exit_code: i32 }.
In the workspace/tab switching handler, emit PluginEvent::WorkspaceSwitched {
workspace_id: String, tab_id: PaneId }.
```

Update the verify command to include a test or manual check that confirms these events are sent (e.g., a test plugin that logs received events).

### IMPORTANT: Clarify PLAN-3.1 Task 3 (System Monitor)

Specify the portable paths for system information:
- Linux: `/proc/uptime`, `/proc/hostname` (or use WASI filesystem APIs)
- macOS: Use `sysctl` or WASI time API (clock_gettime)
- Windows: Use WASI filesystem APIs or return "N/A"

Add a note that the plugin gracefully handles missing paths.

### MINOR: Confirm PLAN-1.1 Task 3 Timing

Ensure the integration test prints the load time in milliseconds and verifies <50ms. The verify command's `--nocapture` flag will show this.

---

## Final Assessment

**Plans are 95% ready for execution.** The core architecture is sound, dependencies are correct, and verification is concrete. The single blocker is **PLAN-2.1 Task 3's missing event emission specification** for CommandExecuted and WorkspaceSwitched. This must be clarified before execution to ensure success criterion 3 is met. Once that is fixed, Phase 6 plans form a coherent, well-scoped roadmap that can ship an MVP WASM plugin system.

**Recommendation:** Issue a CONDITIONAL PASS with the requirement to update PLAN-2.1 Task 3's action field before builder execution.
