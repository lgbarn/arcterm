# TESTING.md

## Overview
Arcterm has 558 `#[test]` functions spread across 29 source files. Tests are predominantly inline `#[cfg(test)]` modules co-located with the implementation. The single exception is `arcterm-plugin/tests/runtime_test.rs`, which is an external integration test. There is no mocking framework — all tests use real types and real filesystem isolation via `tempfile`. No coverage tooling is configured.

---

## Findings

### Test Framework and Runner

- **Framework**: Rust's built-in `#[test]` + `#[tokio::test]` for async tests. No external test framework (no `nextest.toml`, no `criterion` benchmarks, no `proptest`).
  - Evidence: `arcterm-core/src/cell.rs:61` — `#[test] fn cell_default_is_space()`
  - Evidence: `arcterm-plugin/src/manager.rs:639` — `#[tokio::test] async fn event_broadcast_reaches_subscribers()`

- **Async test runtime**: `tokio` via `#[tokio::test]` attribute — used only where async behavior is tested
  - Evidence: `arcterm-plugin/src/manager.rs:639,687` — two async tests verify broadcast channel behavior

- **Cargo test aliases**:
  - `cargo xt` — runs `arcterm-core`, `arcterm-vt`, `arcterm-pty` (the non-GPU crates)
  - Evidence: `.cargo/config.toml` — `xt = "test --package arcterm-core --package arcterm-vt --package arcterm-pty"`
  - [Inferred] GPU-dependent crates (`arcterm-render`, `arcterm-app`) are excluded from the shortcut alias because they require a display/GPU and cannot run headlessly in CI

### Test File Organization

- **Inline modules** (primary pattern): `#[cfg(test)] mod tests { ... }` at the bottom of every implementation file
  - Evidence: `arcterm-core/src/cell.rs:57-113` — 6 unit tests for `Cell` and `Color`
  - Evidence: `arcterm-core/src/grid.rs:668-1112` — 38 tests for `Grid` operations
  - Evidence: `arcterm-app/src/config.rs:418-706` — 20 tests for `ArctermConfig`
  - Evidence: `arcterm-plugin/src/manager.rs:542-746` — 4 unit + 2 async tests (broadcast, lifecycle)
  - Evidence: `arcterm-plugin/src/manifest.rs:198-366` — 11 tests for manifest parsing and security

- **External integration tests** (one file only): `arcterm-plugin/tests/runtime_test.rs`
  - 3 tests: `test_runtime_creation`, `test_component_compiles`, `test_load_timing`
  - Tests the wasmtime `PluginRuntime` end-to-end with a hand-written WAT component
  - Includes a performance assertion: component compile must complete in < 50ms
  - Evidence: `arcterm-plugin/tests/runtime_test.rs:72-107`

- **Tests co-located with lib.rs for vt crate**: `arcterm-vt/src/lib.rs` contains `mod handler_tests` (79 tests) testing `Handler` trait implementation against a real `Grid`
  - Evidence: `arcterm-vt/src/lib.rs:13` — `#[cfg(test)] mod handler_tests { ... }`

### Test Naming

- **Pattern**: `snake_case` verb-object-condition describing the expected behavior
  - `cell_default_is_space` — what the default state is
  - `cell_set_char_marks_dirty` — what side-effect occurs
  - `scroll_up_pushes_to_scrollback` — what the operation does
  - `validate_rejects_name_with_forward_slash` — what input is rejected and why
  - `toml_overrides_fields` — TOML parsing behavior
  - `event_broadcast_reaches_subscribers` — async behavior
  - Evidence: `arcterm-core/src/cell.rs:62,70,78,90,96,103`
  - Evidence: `arcterm-plugin/src/manifest.rs:337-365`

- **Assertion messages**: `assert!(..., "message describing invariant")` — tests provide failure messages explaining *what* should have happened
  - Evidence: `arcterm-core/src/cell.rs:67` — `assert!(cell.dirty, "default cell must be marked dirty")`
  - Evidence: `arcterm-core/src/grid.rs:890` — `assert_eq!(g.scrollback_len(), 1, "one row must be in scrollback after scroll_up")`

### Test Grouping Within Files

- **Section comments** group related tests by feature:
  - Evidence: `arcterm-core/src/grid.rs:881-883`:
    ```rust
    // -------------------------------------------------------------------------
    // Task 1: Scrollback buffer and scroll regions
    // -------------------------------------------------------------------------
    ```
  - Evidence: `arcterm-core/src/grid.rs:968-970`:
    ```rust
    // -------------------------------------------------------------------------
    // Task 2: TermModes, cursor save/restore, alt screen, viewport
    // -------------------------------------------------------------------------
    ```
  - Evidence: `arcterm-plugin/src/manager.rs:548-570` — each test group labeled `// ── (a) ...`, `// ── (b) ...` etc.

### Fixtures and Helpers

- **Factory helpers**: `make_grid(rows, cols)` — avoids repetition in vt tests
  - Evidence: `arcterm-vt/src/lib.rs:18-20`:
    ```rust
    fn make_grid(rows: usize, cols: usize) -> Grid {
        Grid::new(GridSize::new(rows, cols))
    }
    ```

- **Helper: `write_plugin_toml`**: Sets up a minimal `plugin.toml` + placeholder wasm for file-copy tests
  - Evidence: `arcterm-plugin/src/manager.rs:552-565`

- **`tempfile::tempdir()`**: Used for filesystem isolation in plugin manager tests — ensures no test pollutes the real config directory
  - Evidence: `arcterm-plugin/src/manager.rs:574,604,644,691,716`

- **Inline WAT component**: A minimal hand-written WebAssembly Text component is embedded as a `const &str` for runtime integration tests — avoids external binary dependency
  - Evidence: `arcterm-plugin/tests/runtime_test.rs:29-66` — `const TEST_COMPONENT_WAT: &str = r#"(component ...)"`

### What Is Tested

| Crate | Tests | What's Covered |
|-------|-------|----------------|
| `arcterm-core` | 55 | `Cell`, `Color`, `CellAttrs`, `Grid` (cursor, scroll, resize, SGR, alt screen, scrollback, viewport) |
| `arcterm-vt` | 79 + 33 + 19 = 131 | VT handler sequences, APC/Kitty scanner, processor passthrough |
| `arcterm-plugin` | 11 + 4 + 3 = 18 | Manifest parsing, security validation, runtime creation, broadcast events, file install |
| `arcterm-app` | ~286 | Config loading/TOML parsing, palette, keymap, tab layout, selection, search, overlay, detection, workspace, colors, neovim, AI detection, proc |
| `arcterm-render` | 32 + 6 = 38 | Structured blocks, text rendering helpers |
| `arcterm-pty` | 2 | PTY session smoke tests |

### Coverage Tooling

- **No `tarpaulin` or `llvm-cov` configured** — no `.cargo/tarpaulin.toml`, no `codecov.yml`, no coverage CI step
  - Evidence: No coverage config files found in project root or `.cargo/`
  - [Inferred] Coverage is not tracked; there is no baseline metric

### What Is Not Tested

- **`arcterm-render`**: GPU pipeline tests (`GpuState`, `QuadRenderer`, `TextRenderer`) are absent — these require a wgpu-capable display and cannot run headlessly. Only pure-logic helpers (`StructuredBlock`, `ansi_color_to_glyphon`) have tests.
  - Evidence: `arcterm-render/src/renderer.rs` — no `#[cfg(test)]` module; `arcterm-render/src/gpu.rs` — no tests
- **`arcterm-app/src/main.rs`**: The `winit` event loop is untested — not possible to test without a window.
- **Hot-reload**: `watch_config()` in `arcterm-app/src/config.rs` spawns an OS thread with a file watcher — no test for file-change events; would require temporary file manipulation and timing.
  - Evidence: `arcterm-app/src/config.rs:337-412` — no `#[cfg(test)]` section after `watch_config`
- **`arcterm-pty`**: Only 2 tests (`arcterm-app/src/proc.rs`) — PTY session spawning and resize are not exercised by automated tests (require a real TTY).

### `#[tokio::test]` Usage

- Async tests used selectively where tokio channels/timeouts are required
- Evidence: two async tests in `arcterm-plugin/src/manager.rs:639,687` — both use `tokio::time::timeout` to verify broadcast delivery without blocking indefinitely
- All other tests are synchronous even within async contexts (blocking calls tested via `tokio::task::block_in_place` pattern where needed)

---

## Summary Table

| Item | Detail | Confidence |
|------|--------|------------|
| Test framework | Built-in `#[test]` + `#[tokio::test]` | Observed |
| Total test count | 558 `#[test]` across 29 files | Observed |
| Organization | Inline `#[cfg(test)]` modules + one external `tests/` file | Observed |
| Integration tests | `arcterm-plugin/tests/runtime_test.rs` (3 tests, real wasmtime) | Observed |
| Mocking | None — all tests use real types | Observed |
| Filesystem isolation | `tempfile::tempdir()` | Observed |
| Async tests | `#[tokio::test]` in plugin manager only | Observed |
| Test naming | `snake_case` verb-object-condition | Observed |
| Section grouping | Task-labeled section comments within test modules | Observed |
| Coverage tooling | None configured | Observed |
| GPU test coverage | Not tested (headless constraint) | Observed |
| PTY session tests | Minimal (2 tests) | Observed |
| Performance tests | 1 timing assertion (`< 50ms` compile) in runtime_test | Observed |

---

## Open Questions

- Is there a CI pipeline (GitHub Actions / other) that runs `cargo xt` or `cargo test`? No `.github/workflows/` directory found.
- Should `watch_config()` be covered by a test that writes a temp config file and verifies the reload channel receives the update?
- The `arcterm-render` structured block tests (`arcterm-render/src/structured.rs:32` tests) pass without GPU — worth confirming this assumption holds on all platforms.
- Coverage baseline: with 558 tests and no coverage tool, it is unknown how much of `arcterm-app/src/main.rs` (the largest file) is exercised.
