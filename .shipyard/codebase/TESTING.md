# TESTING.md

## Overview

ArcTerm's test suite is split across inline `#[cfg(test)]` modules in library crates and external integration tests in `tests/` subdirectories. The canonical test runner is `cargo-nextest`. Two test helper libraries dominate in the upstream crates: `k9` (snapshot assertions) for terminal emulation tests, and `rstest` (parameterized fixtures) for SSH integration tests. The two ArcTerm-specific crates (`arcterm-ai`, `arcterm-wasm-plugin`) each carry substantial inline `#[cfg(test)]` suites — `arcterm-ai` uses only standard `assert!`/`assert_eq!`, while `arcterm-wasm-plugin` uses `tempfile` for WASM fixture creation and tests that exercise wasmtime directly. There is no coverage tooling configured in CI; no property-based or fuzz testing was found.

---

## Findings

### Test Frameworks and Runners

- **Primary test runner**: `cargo-nextest` is the project-standard test runner. The `Makefile` `test` target calls `cargo nextest run` exclusively. CI workflows install `cargo-nextest` via `baptiste0928/cargo-install` before running tests.
  - Evidence: `Makefile` (lines 5–7), `.github/workflows/termwiz.yml` (lines 41–49), `.github/workflows/wezterm_ssh.yml` (lines 39–47)
- **Standard test attributes**: Tests use `#[test]` (sync) and occasionally `smol::block_on(async { ... })` for async test bodies. There is no `#[tokio::test]` or `#[async_std::test]` usage; the async runtime is `smol`. The ArcTerm crates use only synchronous `#[test]` — no async test bodies are needed since `arcterm-ai` uses blocking HTTP.
  - Evidence: `wezterm-ssh/tests/e2e/sftp/file.rs` (line 15), `config/src/lua.rs` (line 923)
- **`k9` assertion library**: Used in `term/` and `mux/` for rich diff output on assertion failures. The key pattern is aliasing `k9::assert_equal` as `assert_eq` to get better diff output than the standard macro. `k9::snapshot!` is used heavily for multi-line struct comparisons. Not used in the ArcTerm crates.
  - Evidence: `term/src/test/mod.rs` (line 11: `use k9::assert_equal as assert_eq`), `term/src/test/csi.rs` (line 12: `k9::snapshot!`)
- **`rstest` parameterized fixtures**: Used in `wezterm-ssh` integration tests for fixture injection (an async `session` fixture providing a live SSH session backed by a local `sshd`). Not used in the ArcTerm crates.
  - Evidence: `wezterm-ssh/tests/e2e/sftp/file.rs` (lines 4, 9)
- **`assert_fs`**: Used in SSH integration tests for temporary directory management (`TempDir`, `ChildPath` assertions).
  - Evidence: `wezterm-ssh/tests/sshd.rs` (line 1), `wezterm-ssh/tests/e2e/sftp/file.rs` (line 2)
- **`tempfile`**: Used in `arcterm-wasm-plugin` tests to write invalid WASM bytes into a `NamedTempFile` and verify that the loader returns the correct error. Declared as a `[dev-dependencies]` entry.
  - Evidence: `arcterm-wasm-plugin/Cargo.toml` (line 16: `tempfile = { workspace = true }`), `arcterm-wasm-plugin/src/loader.rs` (lines 255–267)

### Test Organization

- **Unit tests are co-located**: The dominant pattern is a `#[cfg(test)]` module at the bottom of each source file, or in a `src/test/` subdirectory for larger test suites.
  - Evidence: `wezterm-surface/src/lib.rs` (line 917), `mux/src/pane.rs` (line 543), `config/src/color.rs` (line 775)
- **ArcTerm crate tests all co-located**: Both `arcterm-ai` and `arcterm-wasm-plugin` use the co-located `#[cfg(test)]` pattern exclusively — there are no external `tests/` directories in either crate.
  - Evidence: `arcterm-ai/src/backend/ollama.rs` (line 57), `arcterm-ai/src/backend/claude.rs` (line 63), `arcterm-ai/src/config.rs` (line 37), `arcterm-ai/src/context.rs` (line 43), `arcterm-ai/src/destructive.rs` (line 60), `arcterm-ai/src/suggestions.rs` (line 130), `arcterm-ai/src/agent.rs` (line 231), `arcterm-ai/src/prompts.rs` (line 44)
  - Evidence: `arcterm-wasm-plugin/src/capability.rs` (line 163), `arcterm-wasm-plugin/src/loader.rs` (line 218), `arcterm-wasm-plugin/src/event_router.rs` (line 71), `arcterm-wasm-plugin/src/lifecycle.rs` (line 201), `arcterm-wasm-plugin/src/host_api.rs` (line 368)
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

#### AI Backend Tests (`arcterm-ai/src/backend/`)

Both LLM backend modules have inline `#[cfg(test)]` suites:

- **`ollama.rs` tests** (lines 57–80): Verify URL construction and that `is_available()` returns `false` when no Ollama server is listening. Uses a port unlikely to be in use (`127.0.0.1:19999`) to avoid flakiness.
  - `test_ollama_urls` — asserts `chat_url()` and `tags_url()` produce correct endpoint strings.
  - `test_ollama_unavailable_when_not_running` — asserts `is_available()` returns `false` against a dead port.
- **`claude.rs` tests** (lines 63–96): Verify availability flag logic and that `chat` returns a descriptive error (not a panic) when the API is unreachable.
  - `test_claude_available_with_key` — non-empty key → `is_available()` is `true`.
  - `test_claude_unavailable_without_key` — empty key → `is_available()` is `false`.
  - `test_claude_name` — asserts `name()` returns `"Claude"`.
  - `test_claude_chat_fails_without_network` — calls `chat()` with a real (but unreachable) API key, asserts `Err` and that the error string contains `"Claude API request failed"`. This test requires network absence to pass — it will fail if the test host has a route to `api.anthropic.com:443`.

Evidence: `arcterm-ai/src/backend/ollama.rs` (lines 57–80), `arcterm-ai/src/backend/claude.rs` (lines 63–96)

#### AI Config and Context Tests (`arcterm-ai/src/`)

- **`config.rs`** (lines 37–61): Tests that `AiConfig::default()` sets Ollama backend, correct endpoint/model, `None` API key, and 30 context lines. Tests that a Claude config can be built with struct update syntax.
- **`context.rs`** (lines 43–77): Tests `PaneContext::empty()` returns `has_content() == false`, that a populated context returns `true`, and that `format_for_llm()` includes the CWD, process name, and scrollback text in the formatted output.
- **`prompts.rs`** (lines 44–78): Tests `format_context_message` in full and minimal cases. Verifies that `AI_PANE_SYSTEM_PROMPT` is non-empty and contains `"DESTRUCTIVE"` and that `COMMAND_OVERLAY_SYSTEM_PROMPT` contains `"one shell command"`.
- **`destructive.rs`** (lines 60–131): The most thorough suite in `arcterm-ai`. Covers `rm -rf`, safe `rm`, SQL DROP, git force-push, git reset (safe variant), `dd`, `chmod`, fork bomb, and `maybe_warn` label/no-label cases. Case-insensitive detection is implicitly covered by the SQL tests (`DROP TABLE` vs `drop database`).

Evidence: `arcterm-ai/src/config.rs` (lines 37–61), `arcterm-ai/src/context.rs` (lines 43–77), `arcterm-ai/src/prompts.rs` (lines 44–78), `arcterm-ai/src/destructive.rs` (lines 60–131)

#### AI Suggestion Tests (`arcterm-ai/src/suggestions.rs`)

Lines 130–200 cover:
- `is_at_shell_prompt` with semantic zones (zone present and covering cursor row → true; not covering → false)
- `is_at_shell_prompt` heuristic fallback (cursor on last row + shell process name → true; non-shell process → false; cursor in middle of screen → false)
- `build_suggestion_query` returns two messages and embeds context CWD and partial command
- `clean_suggestion` strips backticks, removes markdown prefix, strips repeated partial command, takes first line of multi-line responses, handles empty input
- `SuggestionConfig::default()` values

Evidence: `arcterm-ai/src/suggestions.rs` (lines 130–200)

#### AI Agent Tests (`arcterm-ai/src/agent.rs`)

Lines 231–348 cover the `AgentSession` state machine and `parse_steps` JSON parser:
- `parse_steps` with valid JSON, markdown-wrapped JSON, and unparseable input
- Full execute→complete→next-step cycle across two steps
- Skip current step advances `current_step` and sets `StepStatus::Skipped`
- Abort sets `AgentState::Aborted` and `is_finished()` returns `true`
- Failure → `StepFailed` → retry resets to `Reviewing`
- `summary()` string counts completed/skipped/failed correctly
- Empty plan immediately transitions to `Completed`

Evidence: `arcterm-ai/src/agent.rs` (lines 231–348)

#### WASM Capability Tests (`arcterm-wasm-plugin/src/capability.rs`)

Lines 163–260 cover the full capability parsing and enforcement surface:
- `Capability::parse` for `terminal:read`, `fs:read:/home/user`, `net:connect:host:port`
- `fs:read` without path target → `Err`
- `CapabilitySet::new([])` automatically includes `terminal:read`
- `CapabilitySet` denies `fs:read` without a grant
- Filesystem path prefix enforcement: file within granted subtree → allowed; outside → denied
- **Path traversal blocking**: `../` components in a requested path are denied even if the base would otherwise match (`/home/user/../.ssh/id_rsa` is denied when grant is `fs:read:/home/user`)

Evidence: `arcterm-wasm-plugin/src/capability.rs` (lines 163–260)

#### WASM Loader Tests (`arcterm-wasm-plugin/src/loader.rs`)

Lines 218–340 cover:
- `create_engine()` succeeds
- File-not-found path produces `Err` with message containing `"failed to read WASM file"`
- Invalid WASM bytes (written via `tempfile::NamedTempFile`) produce `Err` with `"failed to compile WASM component"`
- `PluginStoreData` memory limit: 512 KB and exactly at 1 MB limit → allowed; 1 MB + 1 byte → denied
- `refuel_store` resets fuel to exact given value; verify at two different budget values

Evidence: `arcterm-wasm-plugin/src/loader.rs` (lines 218–340)

#### WASM Host API Tests (`arcterm-wasm-plugin/src/host_api.rs`)

Lines 368–740 — the largest test module in the ArcTerm crates. Two test helper functions (`store_no_caps`, `store_with_caps`) build `wasmtime::Store<PluginStoreData>` instances without needing a WASM guest binary.

Tests bypass the wasmtime linker dispatch layer and verify capability enforcement by calling `capabilities.check()` directly on the store data. This keeps tests fast (no guest compilation) while exercising the exact logic branches used in the host function closures.

Coverage:
- Log, filesystem, network, and terminal-write function registration succeeds
- Duplicate registration of the same interface returns `Err`
- `create_default_linker` registers all four interface groups without error
- `fs:read` and `fs:write` denied without capability; allowed within granted path prefix; denied outside prefix
- `net:connect` denied without capability; allowed with matching `host:port`; denied with wrong host
- `terminal:write` denied without capability; allowed when granted
- `extract_host_port` URL parsing for full URL with path, bare `host:port`, and URL without port

Evidence: `arcterm-wasm-plugin/src/host_api.rs` (lines 368–740)

#### WASM Lifecycle and EventRouter Tests

- **`lifecycle.rs`** (lines 201–460): Plugin state machine transitions (Loading → Initializing → Running → Stopping → Stopped), `Failed` state, `Display` impl for all states, `PluginManager::load_all` with empty config list, disabled plugins are skipped and not added to the plugin list, missing file produces a `Failed` plugin, multiple failing plugins are recorded independently (isolation), `shutdown_all` transitions running → stopped and leaves failed plugins untouched, mixed-state shutdown, and a fuel refuel integration test via `refuel_store`.
- **`event_router.rs`** (lines 71–107): Empty router dispatches empty list, subscribe + dispatch routes to correct subscribers, multiple subscribers for same event type are all returned.

Evidence: `arcterm-wasm-plugin/src/lifecycle.rs` (lines 201–460), `arcterm-wasm-plugin/src/event_router.rs` (lines 71–107)

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
| `arcterm-ai` | `src/backend/ollama.rs` (line 57) | URL construction, availability detection |
| `arcterm-ai` | `src/backend/claude.rs` (line 63) | Availability flag, error shape on network failure |
| `arcterm-ai` | `src/config.rs` (line 37) | Default config values |
| `arcterm-ai` | `src/context.rs` (line 43) | `PaneContext` content detection and LLM formatting |
| `arcterm-ai` | `src/destructive.rs` (line 60) | Destructive command pattern detection (12 test fns) |
| `arcterm-ai` | `src/suggestions.rs` (line 130) | Prompt detection, query building, response cleaning |
| `arcterm-ai` | `src/agent.rs` (line 231) | `AgentSession` state machine, `parse_steps` JSON parser |
| `arcterm-ai` | `src/prompts.rs` (line 44) | System prompt format, `format_context_message` |
| `arcterm-wasm-plugin` | `src/capability.rs` (line 163) | Capability parsing, path prefix enforcement, traversal blocking |
| `arcterm-wasm-plugin` | `src/loader.rs` (line 218) | Engine creation, file-not-found/invalid-WASM errors, memory limit, fuel refuel |
| `arcterm-wasm-plugin` | `src/host_api.rs` (line 368) | Linker registration, capability enforcement for all 4 host API interfaces |
| `arcterm-wasm-plugin` | `src/lifecycle.rs` (line 201) | Plugin state machine, `PluginManager::load_all`, `shutdown_all` |
| `arcterm-wasm-plugin` | `src/event_router.rs` (line 71) | EventRouter subscription and dispatch |

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
- **`tempfile::NamedTempFile` for WASM fixtures**: `arcterm-wasm-plugin` tests that need an on-disk WASM file write bytes to a `NamedTempFile`. The file is deleted when the binding goes out of scope.
  - Evidence: `arcterm-wasm-plugin/src/loader.rs` (lines 255–258)
- **`store_no_caps` / `store_with_caps` helpers**: In `arcterm-wasm-plugin/src/host_api.rs`, module-private helper functions construct `wasmtime::Store<PluginStoreData>` instances with zero or specified capabilities. This avoids repetitive `CapabilitySet`/`Store` setup in each test body.
  - Evidence: `arcterm-wasm-plugin/src/host_api.rs` (lines 378–393)
- **`default_config` / `enabled_config` / `disabled_config` helpers**: In the loader and lifecycle test modules, small helper functions construct `WasmPluginConfig` structs with canonical values. This follows the same inline helper factory pattern used upstream (e.g., `FakePane`).
  - Evidence: `arcterm-wasm-plugin/src/loader.rs` (lines 223–232), `arcterm-wasm-plugin/src/lifecycle.rs` (lines 210–231)

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

The snapshot strings are stored inline in the source file (not in separate `.snap` files), making them visible in diffs but resulting in very long test files. The ArcTerm crates do not use `k9` snapshots.
- Evidence: `term/src/test/csi.rs` (lines 12–200+), `wezterm-ssh/src/config.rs` (lines 821–832)

### Benchmarks

- **`criterion`**: Used in `termwiz/benches/cell.rs` (Cell creation benchmarks) and `wezterm-char-props/benches/wcwidth.rs` (Unicode column width benchmarks).
  - Evidence: `termwiz/benches/cell.rs` (lines 1–22)
- **`benchmarking` crate**: Used inline in `wezterm-gui/src/shapecache.rs` for a manual glyph shaping benchmark within a `#[cfg(test)]` block.
  - Evidence: `wezterm-gui/src/shapecache.rs` (lines 259–304)
- Benchmarks are not run in CI; they are development tools only.
- No benchmarks exist in the ArcTerm crates.

### Property-Based and Fuzz Testing

No `proptest`, `quickcheck`, or `cargo-fuzz` integration was found anywhere in the codebase. The conformance test approach (running all Unicode BiDi test vectors) serves a similar exhaustive-coverage purpose for that subsystem.

---

## Summary Table

| Item | Detail | Confidence |
|------|--------|------------|
| Test runner | `cargo-nextest` (installed in CI; used in `Makefile`) | Observed |
| Async test runtime | `smol::block_on()` wrapping async test bodies (upstream); none needed in arcterm crates | Observed |
| Assertion library | `k9` (`assert_equal`, `snapshot!`) in terminal/mux tests; standard `assert!`/`assert_eq!` in arcterm crates | Observed |
| Parameterized tests | `rstest` in `wezterm-ssh` integration tests; not used in arcterm crates | Observed |
| Temp file management | `assert_fs::TempDir` in SSH integration tests; `tempfile::NamedTempFile` in arcterm-wasm-plugin | Observed |
| Test organization | `#[cfg(test)]` inline modules in all arcterm crates; `tests/` for integration (SSH only) | Observed |
| Terminal emulation tests | `TestTerm` wrapper exercising real `Terminal`; issue-number-named regressions | Observed |
| SSH integration tests | Spins up real `sshd` process; skipped if not available on host | Observed |
| arcterm-ai tests | 8 co-located `#[cfg(test)]` modules; standard assert macros only; no mocking | Observed |
| arcterm-wasm-plugin tests | 5 co-located `#[cfg(test)]` modules; wasmtime engine exercised directly; `tempfile` fixtures | Observed |
| arcterm-wasm-plugin host API tests | Capability enforcement tested via store-data inspection, bypassing linker dispatch | Observed |
| arcterm-structured-output | Crate removed — no tests to account for | Observed |
| Coverage tooling | None configured | Observed |
| Fuzz/property testing | None found | Observed |
| CI test scope | `--all --no-fail-fast` on Linux and macOS; no Windows test run | Observed |
| Benchmarks | `criterion` in `termwiz` and `wezterm-char-props`; not run in CI | Observed |
| Snapshot storage | Inline in source files (k9 style), not separate `.snap` files | Observed |
| Mock LLM backends | None — `arcterm-ai` tests only check pure functions or use dead network addresses | Observed |
| WASM test fixtures | No compiled `.wasm` fixture files — invalid WASM written via `tempfile` for error-path tests only | Observed |

## Open Questions

- No coverage metrics are tracked. It is unknown what percentage of terminal escape sequence paths are exercised by the `term/src/test/` suite. Adding `cargo-llvm-cov` to CI would give this visibility.
- The `wezterm-gui` crate (the main GUI binary) has very few tests — mostly math and cache tests. The rendering pipeline, key event processing, and overlay system have no automated testing. [Inferred] These are likely tested manually or by running the binary.
- SSH integration tests are conditionally skipped when `sshd` is not present. It is unclear whether standard Ubuntu/macOS GitHub Actions runners have `sshd` available at `/usr/sbin/sshd`, which could mean these tests are silently skipped in CI.
  - Evidence: `wezterm-ssh/tests/sshd.rs` (lines 16–20)
- The `mux` crate's complex tab/pane layout logic has some tests (`mux/src/tab.rs`) but the tmux integration (`mux/src/tmux_commands.rs`, `mux/src/tmux.rs`) appears to have no test coverage.
- `test_claude_chat_fails_without_network` in `arcterm-ai/src/backend/claude.rs` (line 86) will pass only when the test host cannot reach `api.anthropic.com:443`. On a machine with that route available the test will unexpectedly fail (HTTP 401 instead of a connection error). This test should be reclassified or guarded.
- The `arcterm-ai` crate has no mock or stub for `LlmBackend`. Testing functions that call `backend.chat()` against a real backend (with actual streaming response parsing) is not currently feasible from the test suite. A `MockLlmBackend` that returns canned NDJSON strings would enable testing of response parsing in `agent.rs` and `suggestions.rs`.
- The `arcterm-wasm-plugin` crate has no compiled `.wasm` fixture components for positive-path loading tests. The happy-path load flow (valid WASM component successfully loads → `Running` state) is not exercised by any test. This is noted with a comment in `lifecycle.rs`.
  - Evidence: `arcterm-wasm-plugin/src/lifecycle.rs` (lines 339–342: comment acknowledging no fixture WASM components)
