# TESTING.md

## Overview

ArcTerm's test suite is split across inline `#[cfg(test)]` modules in library crates and external integration tests in `tests/` subdirectories. The canonical test runner is `cargo-nextest`. Two test helper libraries dominate: `k9` (snapshot assertions) for terminal emulation tests, and `rstest` (parameterized fixtures) for SSH integration tests. There is no coverage tooling configured in CI; no property-based or fuzz testing was found.

---

## Findings

### Test Frameworks and Runners

- **Primary test runner**: `cargo-nextest` is the project-standard test runner. The `Makefile` `test` target calls `cargo nextest run` exclusively. CI workflows install `cargo-nextest` via `baptiste0928/cargo-install` before running tests.
  - Evidence: `Makefile` (lines 5–7), `.github/workflows/termwiz.yml` (lines 41–49), `.github/workflows/wezterm_ssh.yml` (lines 39–47)
- **Standard test attributes**: Tests use `#[test]` (sync) and occasionally `smol::block_on(async { ... })` for async test bodies. There is no `#[tokio::test]` or `#[async_std::test]` usage; the async runtime is `smol`.
  - Evidence: `wezterm-ssh/tests/e2e/sftp/file.rs` (line 15), `config/src/lua.rs` (line 923)
- **`k9` assertion library**: Used in `term/` and `mux/` for rich diff output on assertion failures. The key pattern is aliasing `k9::assert_equal` as `assert_eq` to get better diff output than the standard macro. `k9::snapshot!` is used heavily for multi-line struct comparisons.
  - Evidence: `term/src/test/mod.rs` (line 11: `use k9::assert_equal as assert_eq`), `term/src/test/csi.rs` (line 12: `k9::snapshot!`)
- **`rstest` parameterized fixtures**: Used in `wezterm-ssh` integration tests for fixture injection (an async `session` fixture providing a live SSH session backed by a local `sshd`).
  - Evidence: `wezterm-ssh/tests/e2e/sftp/file.rs` (lines 4, 9)
- **`assert_fs`**: Used in SSH integration tests for temporary directory management (`TempDir`, `ChildPath` assertions).
  - Evidence: `wezterm-ssh/tests/sshd.rs` (line 1), `wezterm-ssh/tests/e2e/sftp/file.rs` (line 2)

### Test Organization

- **Unit tests are co-located**: The dominant pattern is a `#[cfg(test)]` module at the bottom of each source file, or in a `src/test/` subdirectory for larger test suites.
  - Evidence: `wezterm-surface/src/lib.rs` (line 917), `mux/src/pane.rs` (line 543), `config/src/color.rs` (line 775)
- **Separate `tests/` for integration**: External integration tests (those that require a real running service) live in `<crate>/tests/`. Only `wezterm-ssh` has a populated `tests/` directory.
  - Evidence: `wezterm-ssh/tests/` (contains `lib.rs`, `sshd.rs`, `e2e/`)
- **Terminal emulation tests in dedicated `src/test/`**: The `term` crate has its terminal model tests in `term/src/test/`, split by escape sequence category:
  - `c0.rs` — C0 control characters (BS, LF, CR, TAB)
  - `c1.rs` — C1 control sequences
  - `csi.rs` — CSI sequences (cursor, erase, insert/delete)
  - `selection.rs` — text selection
  - Evidence: `term/src/test/` directory listing, `term/src/test/mod.rs` (lines 5–9)

### Key Test Files and Coverage

#### Terminal Emulation (`term/src/test/`)

The `term` crate's test suite is the most structured. All tests operate through a `TestTerm` wrapper around a real `Terminal` instance, exercising the complete VT parser and screen model:

- `TestTerm::new(height, width, scrollback)` creates a fully functional terminal
- `term.print(bytes)` drives `terminal.advance_bytes()` — the same path used in production
- `term.assert_cursor_pos(x, y, reason, seqno)` — cursor position assertion with optional diagnostic string
- `k9::snapshot!` captures full `Line`/`Cell` structures with all attributes for regression protection

Example from `term/src/test/csi.rs`:
```rust
fn test_789() {
    let mut term = TestTerm::new(1, 8, 0);
    term.print("\x1b[40m\x1b[Kfoo\x1b[2P");
    k9::snapshot!(term.screen().visible_lines(), r#"..."#);
}
```
Test names correlate to GitHub issue numbers (e.g., `test_789`) — a pattern of regression-driven tests named after the issues that prompted them.
- Evidence: `term/src/test/csi.rs` (lines 7–8), `term/src/test/c0.rs`

#### SSH Integration Tests (`wezterm-ssh/tests/`)

`wezterm-ssh` has real end-to-end integration tests that spin up a local `sshd` process:

- `tests/sshd.rs` — test infrastructure: `SshKeygen::generate_rsa()`, port allocation, `SshAgent` process management, `Sshd` struct with `Drop` cleanup
- `tests/e2e/sftp/file.rs` — SFTP file operations: metadata, read, write, seek, truncate, remove
- `tests/e2e/sftp.rs` — SFTP directory operations: open/read/create/remove directories
- `tests/e2e/agent_forward.rs` — SSH agent forwarding
- Tests are `#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]` — conditionally skipped on Windows
- Tests guard with `if !sshd_available() { return; }` since `sshd` may not be installed on all CI runners
- Evidence: `wezterm-ssh/tests/sshd.rs`, `wezterm-ssh/tests/e2e/sftp/file.rs`

#### Lua Event System (`config/src/lua.rs`)

`config/src/lua.rs` contains a `#[cfg(test)]` module with a test that verifies the event registration and emission system end-to-end, mixing Rust-registered handlers with Lua-registered handlers, and confirming that a handler returning `false` stops propagation:
- Evidence: `config/src/lua.rs` (lines 865–960)

#### BiDi Conformance (`bidi/tests/conformance.rs`)

The `bidi` crate runs against the official Unicode Consortium BiDi conformance test data files (`BidiCharacterTest.txt`, `BidiTest.txt`) embedded via `include_str!`. Results are summarized as pass/fail counts rather than failing on first error.
- Evidence: `bidi/tests/conformance.rs` (lines 36–50)

#### Other Inline Tests

| Crate | Location | What is tested |
|-------|----------|---------------|
| `wezterm-surface` | `src/lib.rs` (line 917) | Surface rendering, line storage |
| `wezterm-surface` | `src/hyperlink.rs` (line 218) | Hyperlink detection |
| `wezterm-surface` | `src/line/storage.rs` (line 30) | Line storage edge cases |
| `mux` | `src/pane.rs` (line 543) | `get_lines_with_hyperlinks_applied` via `FakePane` |
| `mux` | `src/tab.rs` (line 2198) | Tab layout logic |
| `wezterm-ssh` | `src/config.rs` (line 801) | SSH config file parsing |
| `config` | `src/color.rs` (line 775) | Color parsing |
| `config` | `src/font.rs` (line 703) | Font config parsing |
| `config` | `src/units.rs` (line 253) | CSS unit parsing |
| `config` | `src/wsl.rs` (line 166) | WSL domain config |
| `codec` | `src/lib.rs` (line 1145) | Codec serialization roundtrips |
| `wezterm-cell` | `src/lib.rs` (line 1000) | Cell attribute packing |
| `pty` | `src/cmdbuilder.rs` (line 773) | Command builder |
| `wezterm-gui` | `src/quad.rs` (line 398) | Quad geometry math |
| `wezterm-gui` | `src/shapecache.rs` (line 115) | Shaping cache |
| `wezterm-gui` | `src/overlay/quickselect.rs` (line 128) | QuickSelect regex |
| `lua-api-crates/plugin` | `src/lib.rs` (line 257) | Plugin loader |
| `lua-api-crates/serde-funcs` | `src/lib.rs` (line 292) | JSON/TOML Lua helpers |

### Fixture and Mock Patterns

- **`TestTerm` wrapper**: The canonical approach for testing terminal behavior. Wraps a real `Terminal` with helper methods. Not a mock — it is a full production instance.
  - Evidence: `term/src/test/mod.rs` (lines 41–87)
- **`FakePane` struct**: For tests that need a `Pane` but only care about line data, a local `FakePane` struct is defined inline in the test module implementing `Pane` with `unimplemented!()` for all methods except the one under test.
  - Evidence: `mux/src/pane.rs` (lines 551–610)
- **`LocalClip` for clipboard**: A simple `Mutex<Option<String>>` wrapper implementing the `Clipboard` trait, defined inline in the test module.
  - Evidence: `term/src/test/mod.rs` (lines 17–38)
- **`TestTermConfig`**: A minimal `TerminalConfiguration` impl with configurable scrollback, used to control terminal behavior in tests without loading the full config system.
  - Evidence: `term/src/test/mod.rs` (lines 45–57)
- **`rstest` fixtures for SSH sessions**: The `wezterm-ssh` integration tests use an async `session` fixture (provided by rstest) that creates a full `SessionWithSshd` — an SSH session connected to a locally spawned `sshd` process.
  - Evidence: `wezterm-ssh/tests/e2e/sftp/file.rs` (line 9: `#[rstest]`, `#[future] session: SessionWithSshd`)
- **Embedded data files**: Conformance test data is embedded via `include_str!` so tests are fully self-contained without runtime file I/O.
  - Evidence: `bidi/tests/conformance.rs` (line 40)

### CI Testing Configuration

- **Per-platform test workflows**: CI has platform-specific workflows (`gen_macos.yml`, `gen_ubuntu24.04_continuous.yml`, etc.) that run `cargo nextest run --all --no-fail-fast`. The `--no-fail-fast` flag ensures all test failures are reported in a single run.
  - Evidence: `.github/workflows/gen_macos.yml` (line 90), `.github/workflows/gen_ubuntu24.04_continuous.yml` (line 105)
- **Focused library workflows**: `termwiz.yml` and `wezterm_ssh.yml` run tests only for those libraries on push/PR when their paths change. This provides faster feedback for library-only changes.
  - Evidence: `.github/workflows/termwiz.yml`, `.github/workflows/wezterm_ssh.yml`
- **SSH tests with dual backend matrix**: `wezterm_ssh.yml` tests the SSH crate twice — once with `libssh-rs` and once with `ssh2` features, exercising both SSH client backends.
  - Evidence: `.github/workflows/wezterm_ssh.yml` (lines 26–69)
- **Format check as a separate job**: Formatting is checked independently from tests via `fmt.yml`, which runs `cargo +nightly fmt --all -- --check`.
  - Evidence: `.github/workflows/fmt.yml`
- **macOS CI runs Intel target only** for tests (`--target=x86_64-apple-darwin`), even though ARM is built in release. [Inferred] This is likely a CI host limitation since macOS GitHub Actions runners are Intel.
  - Evidence: `.github/workflows/gen_macos.yml` (line 90)
- **No coverage tooling**: No `cargo-tarpaulin`, `cargo-llvm-cov`, or `codecov` configuration found anywhere in CI workflows.
- **No Windows test runs**: The `gen_windows*.yml` workflows build but do not appear to run `cargo nextest`. [Inferred] Running `sshd`-dependent tests and some platform tests on Windows CI is impractical.

### Snapshot Testing

`k9::snapshot!` is the primary regression tool for complex output. On first run it writes the expected output to the test source file as a string literal. On subsequent runs it diffs against the stored expectation. This is most heavily used in:
- `term/src/test/csi.rs` — capturing full `Line`/`Cell` struct trees
- `term/src/test/mod.rs` — cursor position and screen content checks
- `wezterm-ssh/src/config.rs` — SSH config parse output

The snapshot strings are stored inline in the source file (not in separate `.snap` files), making them visible in diffs but resulting in very long test files.
- Evidence: `term/src/test/csi.rs` (lines 12–200+), `wezterm-ssh/src/config.rs` (lines 821–832)

### Benchmarks

- **`criterion`**: Used in `termwiz/benches/cell.rs` (Cell creation benchmarks) and `wezterm-char-props/benches/wcwidth.rs` (Unicode column width benchmarks).
  - Evidence: `termwiz/benches/cell.rs` (lines 1–22)
- **`benchmarking` crate**: Used inline in `wezterm-gui/src/shapecache.rs` for a manual glyph shaping benchmark within a `#[cfg(test)]` block.
  - Evidence: `wezterm-gui/src/shapecache.rs` (lines 259–304)
- Benchmarks are not run in CI; they are development tools only.

### Property-Based and Fuzz Testing

No `proptest`, `quickcheck`, or `cargo-fuzz` integration was found anywhere in the codebase. The conformance test approach (running all Unicode BiDi test vectors) serves a similar exhaustive-coverage purpose for that subsystem.

---

## Summary Table

| Item | Detail | Confidence |
|------|--------|------------|
| Test runner | `cargo-nextest` (installed in CI; used in `Makefile`) | Observed |
| Async test runtime | `smol::block_on()` wrapping async test bodies | Observed |
| Assertion library | `k9` (`assert_equal`, `snapshot!`) in terminal/mux tests | Observed |
| Parameterized tests | `rstest` in `wezterm-ssh` integration tests | Observed |
| Temp file management | `assert_fs::TempDir` in SSH integration tests | Observed |
| Test organization | `#[cfg(test)]` inline modules; `tests/` for integration (SSH only) | Observed |
| Terminal emulation tests | `TestTerm` wrapper exercising real `Terminal`; issue-number-named regressions | Observed |
| SSH integration tests | Spins up real `sshd` process; skipped if not available on host | Observed |
| Coverage tooling | None configured | Observed |
| Fuzz/property testing | None found | Observed |
| CI test scope | `--all --no-fail-fast` on Linux and macOS; no Windows test run | Observed |
| Benchmarks | `criterion` in `termwiz` and `wezterm-char-props`; not run in CI | Observed |
| Snapshot storage | Inline in source files (k9 style), not separate `.snap` files | Observed |

## Open Questions

- No coverage metrics are tracked. It is unknown what percentage of terminal escape sequence paths are exercised by the `term/src/test/` suite. Adding `cargo-llvm-cov` to CI would give this visibility.
- The `wezterm-gui` crate (the main GUI binary) has very few tests — mostly math and cache tests. The rendering pipeline, key event processing, and overlay system have no automated testing. [Inferred] These are likely tested manually or by running the binary.
- SSH integration tests are conditionally skipped when `sshd` is not present. It is unclear whether standard Ubuntu/macOS GitHub Actions runners have `sshd` available at `/usr/sbin/sshd`, which could mean these tests are silently skipped in CI.
  - Evidence: `wezterm-ssh/tests/sshd.rs` (lines 16–20)
- The `mux` crate's complex tab/pane layout logic has some tests (`mux/src/tab.rs`) but the tmux integration (`mux/src/tmux_commands.rs`, `mux/src/tmux.rs`) appears to have no test coverage.
