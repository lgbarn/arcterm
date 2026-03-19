# Implementation Plan: WASM Plugin System

**Branch**: `002-wasm-plugin-system` | **Date**: 2026-03-19 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/002-wasm-plugin-system/spec.md`

## Summary

Add a WASM plugin system to ArcTerm using wasmtime with the Component Model.
Plugins run in isolated sandboxes with capability-based permissions. The system
coexists with the existing Lua plugin system. A new `arcterm-wasm-plugin` crate
provides plugin loading, a WIT-defined host API, capability enforcement, and
lifecycle management. Plugins are configured via the existing Lua config.

## Technical Context

**Language/Version**: Rust (edition 2021, min Rust version may need bump for wasmtime)
**Primary Dependencies**: wasmtime (LTS v36.x), wasmtime-wasi (for WASI subset support)
**Storage**: N/A — plugins loaded from user's filesystem
**Testing**: `cargo test --all` + integration tests with compiled WASM test fixtures
**Target Platform**: macOS, Linux, Windows (same as ArcTerm)
**Project Type**: Desktop application — new subsystem (plugin runtime)
**Performance Goals**: Plugin load < 500ms, terminal stays at 60fps with 5 plugins
**Constraints**: Must not break Lua plugins. Must not block GUI thread. Memory-isolated per plugin.
**Scale/Scope**: 1 new crate, ~2000-3000 lines of Rust, WIT contract, test fixtures

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Upstream Compatibility | PASS | New `arcterm-wasm-plugin` crate — no upstream file modifications except wiring in `env-bootstrap` and `wezterm-gui/src/main.rs` (minimal, surgical). |
| II. Security by Default | PASS | Core feature is capability-based sandbox. Deny-by-default. No capabilities auto-granted except `terminal:read`. Filesystem/network/write require explicit user grants. |
| III. Local-First AI | N/A | No AI features in this change. |
| IV. Extension Isolation | PASS | WASM plugins isolated via separate wasmtime Stores. Lua and WASM coexist with independent loading paths and error boundaries. Crashing WASM plugin cannot affect Lua or terminal. |
| V. Test Preservation | PASS | All existing tests must pass. SC-007 explicitly requires `cargo test --all` green. |
| VI. Minimal Surface Area | PASS | Host API starts with core subset (read/write/fs/net/keybinding) matching existing Lua API. No speculative abstractions. Config uses existing Lua system. |
| Fork: CI/CD isolation | PASS | New crate only, no CI changes needed. |
| Fork: Config file | PASS | Plugins configured via existing `arcterm.lua` / `wezterm.lua` config system. |

**Gate result: PASS** — no violations.

## Project Structure

### Documentation (this feature)

```text
specs/002-wasm-plugin-system/
├── plan.md              # This file
├── research.md          # Phase 0: wasmtime research, codebase analysis
├── data-model.md        # Phase 1: Plugin, Capability, HostAPI entities
├── quickstart.md        # Phase 1: verification and usage guide
├── contracts/
│   └── plugin-host-api.wit  # WIT interface definition
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
arcterm-wasm-plugin/                    # New crate
├── Cargo.toml
├── src/
│   ├── lib.rs                          # Crate root, public API
│   ├── capability.rs                   # Capability parsing and enforcement
│   ├── config.rs                       # WasmPluginConfig, Lua registration
│   ├── host_api.rs                     # Host function implementations
│   ├── lifecycle.rs                    # Plugin state machine
│   ├── loader.rs                       # WASM file loading, wasmtime setup
│   └── event_router.rs                 # MuxNotification → plugin dispatch
├── wit/
│   └── plugin-host-api.wit             # WIT interface (copied from contracts/)
└── tests/
    ├── fixtures/                       # Compiled WASM test plugins
    │   ├── hello.wasm                  # Minimal init/destroy
    │   ├── crasher.wasm                # Deliberate panic
    │   ├── fs_reader.wasm              # Reads a file (tests capability)
    │   └── output_watcher.wasm         # Subscribes to on-output
    └── integration_tests.rs            # Load, capability, lifecycle tests

# Modified existing files (minimal wiring)
env-bootstrap/src/lib.rs                # Register WASM plugin Lua API
wezterm-gui/src/main.rs                 # Initialize plugin runtime, route events
config/src/lib.rs                       # Add WasmPluginConfig to ConfigHandle
Cargo.toml                              # Add arcterm-wasm-plugin to workspace
```

**Structure Decision**: New `arcterm-wasm-plugin` crate at workspace root,
following the `arcterm-` naming convention per constitution. Minimal changes
to 3 existing files for wiring.

## Complexity Tracking

No constitution violations — this section is empty.
