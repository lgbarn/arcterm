---
description: "Task list for WASM Plugin System"
---

# Tasks: WASM Plugin System

**Input**: Design documents from `/specs/002-wasm-plugin-system/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/plugin-host-api.wit, quickstart.md

**Tests**: Not explicitly requested in the spec. Verification is via integration test fixtures (compiled WASM plugins) and `cargo test --all`.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Project Initialization)

**Purpose**: Create the `arcterm-wasm-plugin` crate, add wasmtime dependency, and establish the project structure.

- [x] T001 Create `arcterm-wasm-plugin/` directory with `Cargo.toml` declaring dependencies on `wasmtime` (LTS), `config`, `mux`, `log`, `anyhow` — and add it to workspace `members` in root `Cargo.toml`
- [x] T002 Create `arcterm-wasm-plugin/src/lib.rs` with module declarations for `capability`, `config`, `host_api`, `lifecycle`, `loader`, and `event_router`
- [x] T003 Copy WIT contract from `specs/002-wasm-plugin-system/contracts/plugin-host-api.wit` to `arcterm-wasm-plugin/wit/plugin-host-api.wit`
- [x] T004 Add `wasmtime::component::bindgen!` macro invocation in `arcterm-wasm-plugin/src/loader.rs` to generate Rust bindings from the WIT file
- [x] T005 Verify the new crate compiles with `cargo check --package arcterm-wasm-plugin`

**Checkpoint**: New crate exists, compiles, and wasmtime bindings are generated from WIT.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Implement the core types (Plugin, Capability, PluginConfig) and the Lua config integration that all user stories depend on.

- [x] T006 Implement `Capability` type in `arcterm-wasm-plugin/src/capability.rs` — parse capability strings (`fs:read:/path`, `net:connect:host:port`, `terminal:read`, etc.), validate format, implement deny-by-default enforcement checks
- [x] T007 Implement `PluginState` enum and `Plugin` struct in `arcterm-wasm-plugin/src/lifecycle.rs` — state machine (Loading → Initializing → Running → Stopping → Stopped, with Failed from any state), store name/path/capabilities/state
- [x] T008 Implement `WasmPluginConfig` struct in `arcterm-wasm-plugin/src/config.rs` — fields for name, path, capabilities, memory_limit_mb, fuel_per_callback, enabled; implement `FromDynamic` for Lua deserialization
- [x] T009 Add `wezterm.plugin.register()` Lua API function in `arcterm-wasm-plugin/src/config.rs` — register it via `add_context_setup_func()` so it's available in the Lua config; store registrations in a global `Vec<WasmPluginConfig>`
- [ ] T010 Wire `arcterm-wasm-plugin` into `env-bootstrap/src/lib.rs` — add the Lua registration function call alongside the existing 14 lua-api-crate registrations
- [ ] T011 Verify `cargo check --package arcterm-wasm-plugin --package env-bootstrap` compiles

**Checkpoint**: Core types exist. `wezterm.plugin.register()` is callable from Lua config. Config declarations are parsed and stored.

---

## Phase 3: User Story 1 — Load and Run a WASM Plugin (Priority: P1)

**Goal**: Load a WASM file, create a sandboxed wasmtime instance, call `init()`, and run the plugin with terminal read access.

**Independent Test**: Build a minimal "hello world" WASM plugin, configure it in `arcterm.lua`, start ArcTerm, and verify the plugin initializes (log message appears).

- [ ] T012 [US1] Implement WASM loader in `arcterm-wasm-plugin/src/loader.rs` — create `wasmtime::Engine` with fuel consumption enabled, `wasmtime::component::Component` from file, `wasmtime::Store` with memory limits and fuel budget per plugin config
- [ ] T013 [US1] Implement host API stub for `log` interface in `arcterm-wasm-plugin/src/host_api.rs` — expose `info()`, `warn()`, `error()` functions that delegate to the `log` crate; link them into the wasmtime `Linker`
- [ ] T014 [US1] Implement host API stub for `terminal-read` interface in `arcterm-wasm-plugin/src/host_api.rs` — expose `get-visible-text()`, `get-cursor-position()`, `get-working-directory()`, `get-pane-dimensions()`, `get-last-exit-code()`, `get-lines()` backed by the `Pane` trait from `mux`
- [ ] T015 [US1] Implement plugin initialization in `arcterm-wasm-plugin/src/lifecycle.rs` — instantiate the WASM component, call the `lifecycle.init()` export, transition state from Loading → Initializing → Running (or Failed on error)
- [ ] T016 [US1] Implement error containment — catch panics and traps from WASM execution, log the error, mark plugin as Failed, ensure terminal continues normally
- [ ] T017 [US1] Wire plugin loading into `wezterm-gui/src/main.rs` — after `Mux::new()` and config load, iterate `WasmPluginConfig` entries, call the loader for each, log results
- [ ] T018 [US1] Create test fixture: compile a minimal "hello world" WASM plugin to `arcterm-wasm-plugin/tests/fixtures/hello.wasm` that calls `log::info("Hello from WASM plugin!")` in `init()` and returns Ok
- [ ] T019 [US1] Create test fixture: compile a "crasher" WASM plugin to `arcterm-wasm-plugin/tests/fixtures/crasher.wasm` that panics in `init()`
- [ ] T020 [US1] Write integration test in `arcterm-wasm-plugin/tests/integration_tests.rs` — test loading hello.wasm succeeds, test loading crasher.wasm fails gracefully, test loading nonexistent file fails gracefully
- [ ] T021 [US1] Verify `cargo test --package arcterm-wasm-plugin` passes

**Checkpoint**: WASM plugins load, initialize, and run in sandbox. Crashes are contained. Terminal starts normally even with bad plugins.

---

## Phase 4: User Story 2 — Capability-Based Permission Grants (Priority: P1)

**Goal**: Enforce capability checks on every host API call. Deny access when capability is not granted.

**Independent Test**: Load a plugin with only `terminal:read` capability; verify filesystem/network/write calls are denied. Add capabilities and verify they work.

- [ ] T022 [US2] Implement capability enforcement layer in `arcterm-wasm-plugin/src/capability.rs` — add `check_capability(&self, required: &Capability) -> Result<(), CapabilityDenied>` method that validates a requested operation against the plugin's granted capabilities, including path prefix matching for `fs:*` capabilities
- [ ] T023 [US2] Implement host API for `filesystem` interface in `arcterm-wasm-plugin/src/host_api.rs` — `read-file()` and `write-file()` with capability checks before each operation; path validation against granted `fs:read:<path>` / `fs:write:<path>`
- [ ] T024 [US2] Implement host API for `network` interface in `arcterm-wasm-plugin/src/host_api.rs` — `http-get()` and `http-post()` with capability checks against granted `net:connect:<host>:<port>`
- [ ] T025 [US2] Implement host API for `terminal-write` interface in `arcterm-wasm-plugin/src/host_api.rs` — `send-text()` and `inject-output()` with capability check for `terminal:write`
- [ ] T026 [US2] Wire all capability checks into the wasmtime `Linker` — each host function call checks the plugin's capability set before executing; denied calls return an error result (not a trap)
- [ ] T027 [US2] Create test fixture: compile `fs_reader.wasm` to `arcterm-wasm-plugin/tests/fixtures/fs_reader.wasm` that attempts to read a file in `init()`
- [ ] T028 [US2] Write integration tests in `arcterm-wasm-plugin/tests/integration_tests.rs` — test fs_reader.wasm denied without `fs:read` capability, test fs_reader.wasm succeeds with `fs:read:.` capability, test path prefix enforcement (deny reads outside granted path)
- [ ] T029 [US2] Verify `cargo test --package arcterm-wasm-plugin` passes

**Checkpoint**: All host API calls enforce capabilities. Denied operations return clean errors. Path prefix matching works for filesystem.

---

## Phase 5: User Story 3 — Plugin Coexistence with Lua (Priority: P2)

**Goal**: WASM plugins work alongside Lua config and plugins without interference.

**Independent Test**: Start ArcTerm with Lua keybinding customizations AND a WASM plugin. Both work correctly.

- [ ] T030 [US3] Ensure WASM plugin loading is independent of Lua plugin loading order in `wezterm-gui/src/main.rs` — WASM plugins load after Lua config is fully evaluated (so `wezterm.plugin.register()` calls have been processed)
- [ ] T031 [US3] Add error isolation between Lua and WASM in `wezterm-gui/src/main.rs` — a WASM plugin failure during loading must not prevent Lua config from being applied; a Lua config error must not prevent WASM plugins from loading
- [ ] T032 [US3] Write integration test verifying Lua config + WASM plugin coexistence — create a test config that sets font_size via Lua AND registers a hello.wasm plugin; verify both take effect

**Checkpoint**: Lua and WASM systems are independent. Failures in one don't affect the other.

---

## Phase 6: User Story 4 — Plugin Lifecycle Management (Priority: P2)

**Goal**: Plugins have clean lifecycle (load → init → run → destroy), with crash containment, timeout enforcement, and graceful shutdown.

**Independent Test**: Start ArcTerm with a crashing plugin and a working plugin. Working plugin runs, crashing one is marked failed. Shut down ArcTerm and verify destroy callbacks fire.

- [ ] T033 [US4] Implement plugin destroy lifecycle in `arcterm-wasm-plugin/src/lifecycle.rs` — call the `lifecycle.destroy()` export when ArcTerm shuts down; transition state Running → Stopping → Stopped
- [ ] T034 [US4] Implement fuel-based execution timeout in `arcterm-wasm-plugin/src/loader.rs` — configure `store.add_fuel(fuel_per_callback)` before each callback invocation; handle `OutOfFuel` trap by marking the plugin as Failed with a timeout message
- [ ] T035 [US4] Implement memory limit enforcement in `arcterm-wasm-plugin/src/loader.rs` — configure `StoreLimitsBuilder::memory_size(memory_limit_mb * 1024 * 1024)` on the wasmtime Store
- [ ] T036 [US4] Wire plugin shutdown into ArcTerm's exit path in `wezterm-gui/src/main.rs` — on application quit, iterate all running plugins and call destroy
- [ ] T037 [US4] Write integration test for lifecycle — verify init/destroy sequence, verify fuel exhaustion terminates plugin gracefully, verify memory limit terminates plugin gracefully

**Checkpoint**: Full lifecycle works. Crash/timeout/OOM all handled gracefully. Destroy fires on shutdown.

---

## Phase 7: User Story 5 — Terminal State API for Plugins (Priority: P3)

**Goal**: Plugins can subscribe to terminal events (output changes, bell, focus) and register custom key bindings.

**Independent Test**: Load a plugin that watches for output changes and counts them. Verify the callback fires when terminal output changes.

- [ ] T038 [US5] Implement event router in `arcterm-wasm-plugin/src/event_router.rs` — subscribe to `MuxNotification` from the mux layer; filter and debounce `PaneOutput` events; dispatch to plugins that export `events.on-output`
- [ ] T039 [US5] Wire event router into `wezterm-gui/src/main.rs` — start event routing after plugin initialization; ensure events are dispatched on a background thread (not blocking GUI)
- [ ] T040 [US5] Implement bell and focus event routing in `arcterm-wasm-plugin/src/event_router.rs` — handle `Alert::Bell` and focus change notifications; dispatch to `events.on-bell` and `events.on-focus` plugin exports
- [ ] T041 [US5] Implement `keybindings` host API in `arcterm-wasm-plugin/src/host_api.rs` — `register-key-binding()` that creates a `KeyAssignment` and wires it into the existing keybinding system; invoke `events.on-key-binding(id)` when triggered
- [ ] T042 [US5] Create test fixture: compile `output_watcher.wasm` to `arcterm-wasm-plugin/tests/fixtures/output_watcher.wasm` that subscribes to on-output and logs each event
- [ ] T043 [US5] Write integration tests for event routing — verify on-output fires when terminal state changes, verify fuel is consumed per callback, verify debouncing reduces callback frequency

**Checkpoint**: Plugins receive terminal events. Key bindings integrate with existing system. Event dispatch doesn't block GUI.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Final verification, documentation, and cleanup.

- [ ] T044 Run full `cargo test --all` to verify all existing tests pass alongside new WASM tests
- [ ] T045 Run `cargo fmt --all` to ensure formatting is clean
- [ ] T046 Run `cargo clippy --package arcterm-wasm-plugin` to check for lint issues
- [ ] T047 Verify `cargo build --release` succeeds with the new crate
- [ ] T048 Update `specs/002-wasm-plugin-system/spec.md` status from "Draft" to "Complete"

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup — core types and Lua integration
- **User Story 1 (Phase 3)**: Depends on Foundational — needs config + types
- **User Story 2 (Phase 4)**: Depends on US1 — needs working loader to test capabilities
- **User Story 3 (Phase 5)**: Depends on US1 — needs working plugins to test coexistence
- **User Story 4 (Phase 6)**: Depends on US1 — needs working lifecycle to test management
- **User Story 5 (Phase 7)**: Depends on US1 + US4 — needs running plugins and lifecycle
- **Polish (Phase 8)**: Depends on ALL user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Depends on Foundational. Core MVP — everything else builds on this.
- **User Story 2 (P1)**: Depends on US1 (needs loader). Can run after US1 checkpoint.
- **User Story 3 (P2)**: Depends on US1. Independent of US2/US4/US5.
- **User Story 4 (P2)**: Depends on US1. Independent of US2/US3. Can run parallel with US3.
- **User Story 5 (P3)**: Depends on US1 + US4 (needs lifecycle for callback management).

### Parallel Opportunities

- **Phase 2**: T006, T007, T008 are parallelizable (different files)
- **Phase 3**: T012-T014 are parallelizable (loader, log API, terminal-read API are different files)
- **Phase 3**: T018, T019 are parallelizable (different test fixtures)
- **Phase 4**: T023, T024, T025 are parallelizable (different host API interfaces)
- **Phase 5 + 6**: US3 and US4 can run in parallel after US1 completes
- **Phase 7**: T038, T041 are parallelizable (event router vs keybinding API)

---

## Parallel Example: Phase 3 (User Story 1)

```bash
# Launch loader + API stubs in parallel (different files):
Task: "T012 [US1] Implement WASM loader in loader.rs"
Task: "T013 [US1] Implement log host API in host_api.rs"
Task: "T014 [US1] Implement terminal-read host API in host_api.rs"

# Then sequential (depends on loader + API):
Task: "T015 [US1] Implement plugin initialization in lifecycle.rs"
Task: "T016 [US1] Implement error containment"
Task: "T017 [US1] Wire plugin loading into main.rs"

# Launch test fixtures in parallel:
Task: "T018 [US1] Compile hello.wasm fixture"
Task: "T019 [US1] Compile crasher.wasm fixture"

# Final verification:
Task: "T020 [US1] Write integration tests"
Task: "T021 [US1] Verify tests pass"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (crate, deps, WIT bindings)
2. Complete Phase 2: Foundational (types, config, Lua API)
3. Complete Phase 3: User Story 1 (load, init, run, crash containment)
4. **STOP and VALIDATE**: `cargo test --all` green + hello.wasm loads
5. Demo: a WASM plugin that reads terminal text and logs it

### Incremental Delivery

1. US1 → Plugins load and run in sandbox (MVP)
2. Add US2 → Capability enforcement (security complete)
3. Add US3 + US4 in parallel → Lua coexistence + lifecycle management
4. Add US5 → Event routing and key bindings (full feature)
5. Each story adds value without breaking previous stories

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Test fixtures need a WASM build toolchain (cargo-component or similar)
- Wasmtime LTS version must be verified against minimum Rust version — if incompatible, bump Rust minimum
- The `deny.toml` license allowlist may need Apache 2.0 added for wasmtime
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
