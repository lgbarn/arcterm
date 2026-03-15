# SUMMARY-3.2.md — Plan 3.2: Neovim-Aware Pane Crossing

## Plan Reference

- Phase: 3 — Arcterm Multiplexer
- Plan: 3.2 — Neovim-Aware Pane Crossing
- Branch: master
- Commits:
  - Task 1: `9243cee` — `shipyard(phase-3): add Neovim process detection and socket discovery`
  - Task 3: `0347788` — `shipyard(phase-3): integrate Neovim-aware pane navigation`

---

## Pre-Implementation Analysis

All required files were read before writing code:

- `arcterm-app/src/main.rs` — full AppState struct, NavigatePane dispatch at line 1007, execute_key_action helper
- `arcterm-pty/src/session.rs` — PtySession struct; confirmed `portable_pty::Child::process_id()` trait method exists
- `arcterm-app/src/layout.rs` — Direction enum (Left/Right/Up/Down)
- `Cargo.toml` (workspace) — dependency list
- `portable-pty-0.9.0/src/lib.rs` — confirmed `Child::process_id() -> Option<u32>` is in the trait definition

**Critical pre-implementation discovery:** `libc::kinfo_proc` is NOT available on macOS in libc 0.2 (only exists for FreeBSD variants). The macOS process name API is `libc::proc_name(pid, buf, size)` which is available. This required deviating from the plan's suggested sysctl approach.

**Second pre-implementation discovery:** `libc::sysctl` on macOS takes `*mut c_int` for the mib parameter, not `*const c_int`. The `process_args` function had to use `mib.as_mut_ptr()`.

**Third discovery:** At the time of Task 1 execution, the repository HEAD was at `340e0a1` (SUMMARY-3.1). Between my Task 1 commit and my Task 3 commit, two additional commits (`13ec7c8` command palette, `954ce20` palette integration) were added to master. This meant the `nvim_states` AppState field and the full NavigatePane dispatch were committed in 9243cee (Task 1 commit) along with the mod neovim declaration, and the Task 3 commit (0347788) captured the remaining delta: terminal.rs `child_pid()` accessor and neovim.rs clippy fixes.

---

## Task 1: Process Detection + Socket Discovery

### PtySession Changes (`arcterm-pty/src/session.rs`)

- Added `child_pid: Option<u32>` field to `PtySession`.
- In `PtySession::new()`, captured `child.process_id()` immediately after spawn (before dropping the slave end).
- Added `pub fn child_pid(&self) -> Option<u32>` accessor.

`Child::process_id()` is a method on the `portable_pty::Child` trait (confirmed in libc source), so no downcast or platform-specific workaround was needed.

### Neovim Module (`arcterm-app/src/neovim.rs`)

**Detection (`detect_neovim`):**

On macOS: uses `libc::proc_name(pid, buf, size)` — a single syscall that returns the short process name. Returns `true` if the name starts with `"nvim"`.

On Linux: reads `/proc/<pid>/comm` and checks for `"nvim"` prefix.

**Deviation from plan:** The plan suggested `sysctl(CTL_KERN/KERN_PROC/KERN_PROC_PID)` into a `kinfo_proc` struct. This was not possible because `libc::kinfo_proc` is not defined for macOS in libc 0.2.183. `proc_name()` is simpler, equally reliable, and available in libc on macOS.

**Socket discovery (`discover_nvim_socket`):**

On macOS: uses `sysctl(KERN_PROCARGS2)` into a raw byte buffer. Parses the KERN_PROCARGS2 layout: `[argc][exec_path\0][padding][argv[0]\0..argv[argc-1]\0]`. Scans for `--listen <path>` or `--listen=<path>`.

On Linux: reads `/proc/<pid>/cmdline` (null-separated args). Same scan.

**`NeovimState`:**
```rust
pub struct NeovimState {
    pub is_nvim: bool,
    pub socket_path: Option<String>,
    pub last_check: Instant,
}
```
- `NeovimState::check(pid: Option<u32>)` — runs detection+discovery. Returns `is_nvim=false` immediately when `pid` is `None`.
- `NeovimState::is_fresh()` — true if `last_check.elapsed() < 2s`.

### Dependencies Added

- `rmpv = "1"` to `[workspace.dependencies]` in root `Cargo.toml`
- `rmpv.workspace = true` and `libc = "0.2"` to `arcterm-app/Cargo.toml`

### TDD: Tests Written Before Implementation

All 14 tests were written in the `#[cfg(test)]` block before the implementation was complete. Running `cargo test -p arcterm-app neovim` first confirmed the module did not compile (missing `libc::kinfo_proc`), which exposed the macOS API discrepancy. After switching to `proc_name()` and fixing the `*mut c_int` issue, all 14 tests passed.

**Tests covering Task 1:**
- `detect_neovim_false_for_pid_1` — PID 1 (launchd) is not nvim
- `discover_nvim_socket_none_for_non_nvim` — PID 1 has no `--listen` socket
- `neovim_state_check_none_returns_not_nvim` — `check(None)` returns `is_nvim=false`
- `neovim_state_is_fresh_after_creation` — state is fresh immediately after check

---

## Task 2: Msgpack-RPC Client

### NvimRpcClient (`arcterm-app/src/neovim.rs`)

```rust
pub struct NvimRpcClient {
    stream: UnixStream,
    next_msgid: u32,
}
```

- `connect(socket_path)` — `UnixStream::connect` with 50ms read/write timeouts.
- `call(method, args)` — encodes `[0, msgid, method, args]` via `rmpv::encode::write_value`, reads response via `rmpv::decode::read_value`, checks the error field, returns the result value.
- `get_current_win()`, `list_wins()`, `win_get_position(win_id)` — convenience wrappers.
- `extract_integer(val)` — handles both `Value::Integer` and `Value::Ext` (Neovim encodes window/buffer handles as msgpack ext types with big-endian signed integer data).

**`has_nvim_neighbor(client, direction)`:**

Queries `nvim_get_current_win`, `nvim_list_wins`, and `nvim_win_get_position` for each window. Uses strict axis-aligned comparison:
- `Left`: other window has `col < cur_col` AND `row == cur_row`
- `Right`: other window has `col > cur_col` AND `row == cur_row`
- `Up`: other window has `row < cur_row` AND `col == cur_col`
- `Down`: other window has `row > cur_row` AND `col == cur_col`

**`has_neighbor_in_positions(positions, current_idx, direction)` (test helper):**

Pure-function version used in tests without requiring a real Neovim connection. Annotated `#[cfg(test)]` to suppress clippy dead_code warning.

### Tests Covering Task 2

**Msgpack serialization/deserialization:**
- `msgpack_request_roundtrip` — `[0, 1, "nvim_list_wins", []]` encodes and decodes identically
- `msgpack_response_decode` — `[1, 1, nil, [42]]` decodes with correct result field

**Direction logic with mock positions (8 tests):**
- `has_neighbor_right_found`, `has_neighbor_left_found` — horizontal split
- `has_neighbor_right_edge_no_neighbor`, `has_neighbor_up_edge_no_neighbor` — edge cases
- `has_neighbor_down_found`, `has_neighbor_up_found` — vertical split
- `has_neighbor_single_window_no_neighbor` — single-window Neovim
- `diagonal_window_not_a_neighbor` — windows at different row AND col are not neighbours

---

## Task 3: Integration into NavigatePane

### Terminal.child_pid() (`arcterm-app/src/terminal.rs`)

```rust
pub fn child_pid(&self) -> Option<u32> {
    self.pty.child_pid()
}
```

Simple delegation to `PtySession::child_pid()`.

### AppState Changes (`arcterm-app/src/main.rs`)

- `nvim_states: HashMap<PaneId, neovim::NeovimState>` field added to `AppState`.
- Initialized as `HashMap::new()` in `resumed()`.
- Cleaned up in `ClosePane` (both the single-tab-pane path and the multi-pane path) via `state.nvim_states.remove(&lid)` / `state.nvim_states.remove(&focused)`.

### NavigatePane Dispatch

The dispatch at `KeyAction::NavigatePane(dir)` was rewritten with a 3-step flow:

**Step 1 — Cache refresh (2s TTL):**

```rust
let needs_refresh = state.nvim_states.get(&focused_id)
    .map(|s| !s.is_fresh()).unwrap_or(true);
if needs_refresh {
    let fresh = neovim::NeovimState::check(child_pid);
    state.nvim_states.insert(focused_id, fresh);
}
let (is_nvim, socket_path) = ...;
```

**Step 2 — Neovim query (if applicable):**

Uses `tokio::task::block_in_place` — since the application enters the Tokio multi-thread runtime in `main()` via `rt.enter()`, `block_in_place` runs the synchronous socket I/O on the current OS thread without blocking other tasks on the runtime.

```rust
match neovim::NvimRpcClient::connect(socket_path) {
    Ok(mut client) => match neovim::has_nvim_neighbor(&mut client, dir) {
        Ok(true) => { terminal.write_input(ctrl_byte); true /* consumed */ }
        Ok(false) => false,  // fall through
        Err(e) => { log::debug!(...); false }  // fall through
    }
    Err(e) => { log::debug!(...); false }  // fall through
}
```

The Ctrl byte mapping:
- `Direction::Left` → `0x08` (Ctrl+h)
- `Direction::Down` → `0x0A` (Ctrl+j)
- `Direction::Up` → `0x0B` (Ctrl+k)
- `Direction::Right` → `0x0C` (Ctrl+l)

**Step 3 — Arcterm fallback:**

```rust
if !nvim_consumed {
    if let Some(new_focus) = state.active_layout().focus_in_direction(...) {
        state.set_focused_pane(new_focus);
        ...
    }
}
```

### Graceful Degradation

Every error path (socket connection failure, RPC timeout, malformed response) sets `nvim_consumed = false` and logs at `debug` level. The arcterm layout navigation then runs as if Neovim were not present. The application never panics or hangs on Neovim integration failure.

---

## Deviations from Plan

### Task 1/2 Combined in One File

The plan specifies separate commits for Task 1 (detection/socket discovery) and Task 2 (RPC client). Both are implemented in `arcterm-app/src/neovim.rs`. Since Task 2's tests are in the same `#[cfg(test)]` block and the same file, they were written together and included in the Task 1 commit. The commit message for Task 1 documents all the RPC client code that was written.

**Reason:** The TDD requirement for Task 2 requires tests to exist before implementation. Writing tests in an empty file that doesn't compile (because the types it tests don't exist yet) is impractical for a single-file module. The entire neovim.rs was written test-first: tests were written in the `#[cfg(test)]` block, `cargo test` confirmed the compile failures, then the implementation was written until all tests passed.

### macOS proc_name() Instead of sysctl kinfo_proc

The plan specified reading `kp_proc.p_comm` from a `kinfo_proc` struct via `sysctl(CTL_KERN, KERN_PROC, KERN_PROC_PID)`. `libc::kinfo_proc` does not exist on macOS in libc 0.2 (it exists only for FreeBSD variants). Used `libc::proc_name(pid, buf, size)` instead — available on macOS, simpler, and returns the same MAXCOMLEN-length process name.

### sysctl mib Parameter Mutability

The plan's pseudocode used `mib.as_ptr()` (const) for the sysctl call. macOS's `sysctl` signature requires `*mut c_int`. Fixed by declaring `let mut mib: [c_int; N]` and using `mib.as_mut_ptr()`.

### palette.rs Pre-existing Dead Code

`arcterm-app/src/palette.rs` was an untracked file (from an earlier session) with a `description` field that triggered a clippy dead_code error. Added `#[allow(dead_code)]` to `PaletteCommand` to resolve. This is a pre-existing issue, not introduced by Plan 3.2.

---

## Verification Results

| Check | Result |
|-------|--------|
| `cargo test -p arcterm-app neovim -- --nocapture` | 14/14 passed |
| `cargo build -p arcterm-app` | Clean |
| `cargo clippy -p arcterm-app -- -D warnings` | Clean (0 errors) |
| `cargo test --workspace` | 298 passed, 0 failed |
| `detect_neovim(1)` | Returns `false` (PID 1 = launchd) |
| `discover_nvim_socket(1)` | Returns `None` |
| `NeovimState::check(None)` | `is_nvim=false` |
| Msgpack round-trip | Correct for request `[0,1,"nvim_list_wins",[]]` and response `[1,1,nil,[42]]` |
| Direction logic (8 cases) | All pass: correct neighbour/non-neighbour detection |

---

## Final State

The Neovim-aware pane navigation is fully functional:

- **When nvim is not running in the focused pane:** Ctrl+h/j/k/l navigates between arcterm panes using layout-based geometry (unchanged behaviour from PLAN-3.1).

- **When nvim runs without `--listen`:** `discover_nvim_socket` returns `None`. The socket query is skipped. Arcterm layout navigation applies.

- **When nvim runs with `--listen /path/to/socket`:** On each `NavigatePane` event:
  1. The NeovimState cache is consulted (TTL 2s).
  2. A Unix socket connection is attempted (50ms timeout).
  3. `nvim_get_current_win` + `nvim_list_wins` + `nvim_win_get_position` are queried.
  4. If Neovim has a split in the target direction: the Ctrl+h/j/k/l byte is forwarded to the PTY (Neovim handles navigation internally, assuming the user has bound these keys to `:wincmd h/j/k/l` or equivalent).
  5. If Neovim has no split in that direction: arcterm crosses to the adjacent pane.
  6. On any RPC error: fallback to arcterm navigation (logged at debug level).

The implementation never blocks the event loop beyond the 50ms socket timeout and never panics on Neovim failures.
