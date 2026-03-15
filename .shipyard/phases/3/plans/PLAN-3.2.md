---
phase: multiplexer
plan: "3.2"
wave: 3
dependencies: ["1.1", "1.2", "2.1", "2.2"]
must_haves:
  - Neovim process detection via process name on macOS
  - Neovim socket discovery via --listen arg or fd scan
  - Msgpack-RPC client for nvim_get_current_win, nvim_list_wins, nvim_win_get_position
  - Neovim-aware directional navigation (query Neovim splits before crossing to arcterm pane)
  - Graceful fallback when Neovim is not detected or socket unavailable
files_touched:
  - arcterm-app/src/neovim.rs
  - arcterm-app/src/main.rs (mod declaration + NavigatePane dispatch modification)
  - arcterm-pty/src/session.rs (child PID accessor)
  - Cargo.toml (workspace dependency: rmpv)
  - arcterm-app/Cargo.toml (dependency: rmpv, libc)
tdd: true
---

# PLAN-3.2 -- Neovim-Aware Pane Crossing

## Goal

Implement Neovim detection and socket communication so that Ctrl+h/j/k/l first traverses Neovim's own splits before crossing to an arcterm pane boundary. Uses a hand-rolled msgpack-RPC client over Unix socket (avoiding the LGPL nvim-rs dependency) with the `rmpv` crate for msgpack serialization.

## Why Wave 3

Depends on the keymap module (2.2) for `NavigatePane` action dispatch, and on the layout engine (1.1) for fallback to arcterm navigation. The neovim module is an enhancement layer on top of the basic pane navigation flow wired in PLAN-3.1.

## Tasks

<task id="1" files="arcterm-pty/src/session.rs, arcterm-app/src/neovim.rs, Cargo.toml, arcterm-app/Cargo.toml" tdd="true">
  <action>Add child PID access and Neovim process detection:

**PtySession changes** (`arcterm-pty/src/session.rs`):
1. Add `child_pid: Option<u32>` field to `PtySession`.
2. In `PtySession::new()`, after spawning the child process, attempt to retrieve the PID. The `portable_pty::Child` trait does not expose `pid()` directly. Use platform-specific logic: on Unix, the concrete type behind `Box<dyn Child>` in portable-pty v0.9 is `std::process::Child` wrapper. If `process_id()` method exists on the concrete type, use it. Otherwise, store `None` and fall back to process enumeration. Add `pub fn child_pid(&self) -> Option<u32>` accessor.

**Neovim detection** (`arcterm-app/src/neovim.rs`):
1. Add `rmpv = "1"` to `[workspace.dependencies]` in root `Cargo.toml` and to `arcterm-app/Cargo.toml`. Add `libc = "0.2"` to `arcterm-app/Cargo.toml`.
2. `pub fn detect_neovim(pid: u32) -> bool` -- on macOS, use `libc::sysctl` with `CTL_KERN / KERN_PROC / KERN_PROC_PID` to read `kinfo_proc.kp_proc.p_comm`. Return true if the process name starts with "nvim". On Linux, read `/proc/<pid>/comm`. Return false on any error.
3. `pub fn discover_nvim_socket(pid: u32) -> Option<String>` -- read the process's command line args. On macOS: use `sysctl` with `KERN_PROCARGS2`. On Linux: read `/proc/<pid>/cmdline`. Parse for `--listen` followed by a path. Return the path if found. If `--listen` is absent, return None (user must launch nvim with `--listen` for socket integration).
4. `pub struct NeovimState` -- cached per-pane state:
   ```rust
   pub struct NeovimState {
       pub is_nvim: bool,
       pub socket_path: Option<String>,
       pub last_check: Instant,
   }
   ```
5. `NeovimState::check(pid: Option<u32>) -> Self` -- runs detection and discovery. Cache results for 2 seconds to avoid syscall spam on every keypress.

Write tests:
- `detect_neovim` returns false for PID 1 (launchd/init, not nvim)
- `discover_nvim_socket` returns None for a non-nvim process
- `NeovimState::check(None)` returns `is_nvim: false`</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app neovim -- --nocapture</verify>
  <done>Neovim detection tests pass. `detect_neovim` correctly identifies non-nvim processes. Socket discovery parses `--listen` from process args.</done>
</task>

<task id="2" files="arcterm-app/src/neovim.rs" tdd="true">
  <action>Implement the hand-rolled msgpack-RPC client:

1. `pub struct NvimRpcClient` -- wraps a `tokio::net::UnixStream`:
   ```rust
   pub struct NvimRpcClient {
       stream: tokio::net::UnixStream,
       next_msgid: u32,
   }
   ```

2. `NvimRpcClient::connect(socket_path: &str) -> io::Result<Self>` -- connects to the Unix socket asynchronously.

3. `NvimRpcClient::call(&mut self, method: &str, args: Vec<rmpv::Value>) -> io::Result<rmpv::Value>` -- sends a msgpack-rpc request `[0, msgid, method, args]` and reads the response `[1, msgid, error, result]`. Uses `rmpv::encode::write_value` to serialize and `rmpv::decode::read_value` to deserialize. Increments `next_msgid` after each call. Returns the `result` value on success, or an error if the response contains a non-nil error field.

4. Convenience methods:
   - `get_current_win(&mut self) -> io::Result<i64>` -- calls `nvim_get_current_win`, returns window ID as i64.
   - `list_wins(&mut self) -> io::Result<Vec<i64>>` -- calls `nvim_list_wins`, returns vec of window IDs.
   - `win_get_position(&mut self, win_id: i64) -> io::Result<(i64, i64)>` -- calls `nvim_win_get_position(win_id)`, returns `(row, col)`.

5. `pub fn has_nvim_neighbor(client: &mut NvimRpcClient, direction: Direction) -> io::Result<bool>` -- queries current window position and all window positions, determines if a Neovim split exists in the specified direction using geometric overlap logic (same algorithm as described in RESEARCH.md section 5).

Write tests using a mock approach:
- Test msgpack request serialization produces correct byte format
- Test msgpack response deserialization extracts result correctly
- Test `has_nvim_neighbor` direction logic with mock window positions (test the geometric logic as a pure function, separate from the RPC client)</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app neovim -- --nocapture</verify>
  <done>RPC client tests pass. Msgpack serialization/deserialization is correct. Direction logic correctly determines Neovim neighbor presence from mock window positions.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs, arcterm-app/src/neovim.rs" tdd="false">
  <action>Integrate Neovim-aware navigation into the KeyAction::NavigatePane dispatch:

1. Add `mod neovim;` to `main.rs`.
2. Add `nvim_states: HashMap<PaneId, neovim::NeovimState>` to `AppState`.
3. In the `NavigatePane(dir)` dispatch (from PLAN-3.1 Task 2):
   a. Get the focused pane's child PID from `self.panes.get(&focus).and_then(|t| t.child_pid())`.
   b. Check/update `nvim_states` for this pane (using `NeovimState::check` with 2-second cache).
   c. If `is_nvim` and `socket_path.is_some()`:
      - Spawn a `tokio::task::spawn_blocking` (or use the tokio runtime handle) to synchronously connect and query `has_nvim_neighbor`.
      - If Neovim has a neighbor in the target direction: forward the original Ctrl+h/j/k/l byte to the PTY (so Neovim handles the navigation internally). Return without changing arcterm focus.
      - If Neovim does NOT have a neighbor: fall through to arcterm's `focus_in_direction`.
      - If the RPC call fails (timeout, connection error): log at debug level and fall through to arcterm navigation.
   d. If not nvim: use arcterm's `focus_in_direction` as before.
4. Add `pub fn child_pid(&self) -> Option<u32>` to `Terminal` in `terminal.rs`, delegating to `self.pty.child_pid()`.

Note on async: The Neovim RPC query must not block the event loop. Use `tokio::task::block_in_place` (since we are already within the tokio runtime context from `rt.enter()`) or cache the last-known Neovim layout and query asynchronously, applying the result on the next frame. For Phase 3, the synchronous-with-timeout approach is acceptable: set a 50ms connect+read timeout on the Unix socket. If Neovim does not respond in 50ms, fall through.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app && cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -10</verify>
  <done>`cargo build` and `cargo clippy` succeed. When focused pane runs Neovim with `--listen`, Ctrl+h/j/k/l traverses Neovim splits first. When Neovim has no split in the target direction, focus crosses to the adjacent arcterm pane. When Neovim is not detected, standard arcterm navigation applies.</done>
</task>
