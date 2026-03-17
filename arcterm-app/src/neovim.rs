//! Neovim process detection, socket discovery, and msgpack-RPC client.
//!
//! This module provides graceful Neovim integration:
//!
//! 1. **Detection** — `detect_neovim(pid)` checks whether a given PID is running
//!    `nvim` using platform-specific process introspection.
//! 2. **Socket discovery** — `discover_nvim_socket(pid)` scans the process's
//!    command-line arguments for a `--listen <path>` flag.
//! 3. **Cached state** — `NeovimState` wraps both results with a 2-second
//!    cache so that every keypress does not trigger syscalls.
//! 4. **RPC client** — `NvimRpcClient` sends msgpack-RPC requests over a Unix
//!    socket and decodes responses using `rmpv`.
//! 5. **Neighbour check** — `has_nvim_neighbor` queries Neovim's window layout
//!    to determine whether a Neovim split exists in a given direction.

use crate::layout::Direction;
use crate::proc::{process_args, process_comm};
use rmpv::Value;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

// ── Public API ────────────────────────────────────────────────────────────────

/// Returns `true` if the process with `pid` is running Neovim.
///
/// Checks the process name via platform-specific introspection. Returns
/// `false` on any error or unsupported platform — never panics.
pub fn detect_neovim(pid: u32) -> bool {
    process_comm(pid)
        .map(|name| name.starts_with("nvim"))
        .unwrap_or(false)
}

/// Returns the Unix socket path that Neovim is listening on, if any.
///
/// Parses the process's command-line arguments for `--listen <path>`.
/// Returns `None` if the process is not Neovim, does not have `--listen`,
/// or if introspection fails.
pub fn discover_nvim_socket(pid: u32) -> Option<String> {
    let args = process_args(pid)?;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--listen" {
            return iter.next().cloned();
        }
        // Also handle `--listen=<path>` form.
        if let Some(path) = arg.strip_prefix("--listen=") {
            return Some(path.to_string());
        }
    }
    None
}

/// Cached per-pane Neovim state.
///
/// Results are valid for `CACHE_TTL` to avoid syscall spam on every keypress.
pub struct NeovimState {
    pub is_nvim: bool,
    pub socket_path: Option<String>,
    pub last_check: Instant,
}

const CACHE_TTL: Duration = Duration::from_secs(2);

impl NeovimState {
    /// Check whether the process identified by `pid` is Neovim.
    ///
    /// If `pid` is `None`, returns a state with `is_nvim: false`.
    /// Results are cached for 2 seconds — pass `&mut Option<NeovimState>` and
    /// update via `NeovimState::refresh` to benefit from caching.
    pub fn check(pid: Option<u32>) -> Self {
        let Some(pid) = pid else {
            return NeovimState {
                is_nvim: false,
                socket_path: None,
                last_check: Instant::now(),
            };
        };

        let is_nvim = detect_neovim(pid);
        let socket_path = if is_nvim {
            discover_nvim_socket(pid)
        } else {
            None
        };

        NeovimState {
            is_nvim,
            socket_path,
            last_check: Instant::now(),
        }
    }

    /// Returns `true` if the cached data is still within the TTL window.
    pub fn is_fresh(&self) -> bool {
        self.last_check.elapsed() < CACHE_TTL
    }
}

// ── Msgpack-RPC client ────────────────────────────────────────────────────────

/// A synchronous msgpack-RPC client connected to a Neovim Unix socket.
///
/// Uses a 50 ms timeout for all I/O operations so that a stalled Neovim
/// process cannot block the arcterm event loop.
pub struct NvimRpcClient {
    stream: UnixStream,
    next_msgid: u32,
}

impl NvimRpcClient {
    const TIMEOUT: Duration = Duration::from_millis(50);

    /// Connect to the Neovim socket at `socket_path`.
    pub fn connect(socket_path: &str) -> io::Result<Self> {
        let stream = UnixStream::connect(socket_path)?;
        stream.set_read_timeout(Some(Self::TIMEOUT))?;
        stream.set_write_timeout(Some(Self::TIMEOUT))?;
        Ok(NvimRpcClient {
            stream,
            next_msgid: 1,
        })
    }

    /// Send a msgpack-RPC request and return the result value.
    ///
    /// Sends `[0, msgid, method, args]` and reads back `[1, msgid, error, result]`.
    /// Returns an error if Neovim reports a non-nil error in the response.
    pub fn call(&mut self, method: &str, args: Vec<Value>) -> io::Result<Value> {
        let msgid = self.next_msgid;
        self.next_msgid = self.next_msgid.wrapping_add(1);

        // Encode request: [type=0, msgid, method, params]
        let request = Value::Array(vec![
            Value::Integer(0.into()),
            Value::Integer(msgid.into()),
            Value::String(method.into()),
            Value::Array(args),
        ]);

        let mut buf = Vec::new();
        rmpv::encode::write_value(&mut buf, &request)
            .map_err(|e| io::Error::other(e.to_string()))?;

        self.stream.write_all(&buf)?;
        self.stream.flush()?;

        // Read response.
        let response = self.read_value()?;

        // Decode: [type=1, msgid, error, result]
        let arr = match &response {
            Value::Array(a) if a.len() == 4 => a,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected msgpack-RPC response shape",
                ));
            }
        };

        // Check error field (arr[2]).
        if !matches!(&arr[2], Value::Nil) {
            return Err(io::Error::other(format!("nvim RPC error: {:?}", arr[2])));
        }

        Ok(arr[3].clone())
    }

    /// Read a single msgpack value from the stream.
    fn read_value(&mut self) -> io::Result<Value> {
        // rmpv's decode::read_value reads directly from a Read impl.
        // We need a buffered reader to allow multiple read calls.
        let mut reader = ReadableStream {
            inner: &mut self.stream,
        };
        rmpv::decode::read_value(&mut reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    /// Call `nvim_get_current_win` and return the window handle as i64.
    pub fn get_current_win(&mut self) -> io::Result<i64> {
        let val = self.call("nvim_get_current_win", vec![])?;
        extract_integer(&val, "nvim_get_current_win")
    }

    /// Call `nvim_list_wins` and return a list of window handles.
    pub fn list_wins(&mut self) -> io::Result<Vec<i64>> {
        let val = self.call("nvim_list_wins", vec![])?;
        match val {
            Value::Array(arr) => arr
                .iter()
                .map(|v| extract_integer(v, "nvim_list_wins item"))
                .collect(),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "nvim_list_wins: expected array",
            )),
        }
    }

    /// Call `nvim_win_get_position(win_id)` and return `(row, col)`.
    pub fn win_get_position(&mut self, win_id: i64) -> io::Result<(i64, i64)> {
        let val = self.call("nvim_win_get_position", vec![Value::Integer(win_id.into())])?;
        match val {
            Value::Array(ref arr) if arr.len() == 2 => {
                let row = extract_integer(&arr[0], "win_get_position row")?;
                let col = extract_integer(&arr[1], "win_get_position col")?;
                Ok((row, col))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "nvim_win_get_position: expected [row, col]",
            )),
        }
    }
}

/// Wraps a mutable reference to `UnixStream` to implement `Read` for rmpv.
struct ReadableStream<'a> {
    inner: &'a mut UnixStream,
}

impl<'a> Read for ReadableStream<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

fn extract_integer(val: &Value, context: &str) -> io::Result<i64> {
    match val {
        Value::Integer(i) => i.as_i64().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{context}: integer overflow"),
            )
        }),
        Value::Ext(_, data) => {
            // Neovim encodes window/buffer handles as msgpack ext type.
            // The ext data is a big-endian signed integer of length 1, 2, 4, or 8.
            let id = match data.len() {
                1 => data[0] as i64,
                2 => i16::from_be_bytes([data[0], data[1]]) as i64,
                4 => i32::from_be_bytes([data[0], data[1], data[2], data[3]]) as i64,
                8 => i64::from_be_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{context}: unexpected ext data length {}", data.len()),
                    ));
                }
            };
            Ok(id)
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{context}: expected integer, got {:?}", val),
        )),
    }
}

// ── Neighbour detection ───────────────────────────────────────────────────────

/// Returns `true` if a Neovim window split exists in `direction` relative to
/// the currently focused Neovim window.
///
/// Uses geometric position comparison: a window is a neighbour in a given
/// direction if it is on the correct side of the current window's position
/// and there is column/row overlap. Neovim positions are in `(row, col)` form.
///
/// Returns `false` on any RPC error — the caller should fall through to
/// arcterm navigation.
pub fn has_nvim_neighbor(client: &mut NvimRpcClient, direction: Direction) -> io::Result<bool> {
    let current_win = client.get_current_win()?;
    let wins = client.list_wins()?;

    let (cur_row, cur_col) = client.win_get_position(current_win)?;

    for win_id in &wins {
        if *win_id == current_win {
            continue;
        }
        let (row, col) = client.win_get_position(*win_id)?;

        let is_neighbor = match direction {
            Direction::Left => col < cur_col && row == cur_row,
            Direction::Right => col > cur_col && row == cur_row,
            Direction::Up => row < cur_row && col == cur_col,
            Direction::Down => row > cur_row && col == cur_col,
        };

        if is_neighbor {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Pure-function version of neighbor detection for testing with mock positions.
///
/// `positions` is a slice of `(row, col)` for each window.
/// `current_idx` is the index of the current window in the slice.
#[cfg(test)]
pub fn has_neighbor_in_positions(
    positions: &[(i64, i64)],
    current_idx: usize,
    direction: Direction,
) -> bool {
    let (cur_row, cur_col) = positions[current_idx];

    for (i, &(row, col)) in positions.iter().enumerate() {
        if i == current_idx {
            continue;
        }
        let is_neighbor = match direction {
            Direction::Left => col < cur_col && row == cur_row,
            Direction::Right => col > cur_col && row == cur_row,
            Direction::Up => row < cur_row && col == cur_col,
            Direction::Down => row > cur_row && col == cur_col,
        };
        if is_neighbor {
            return true;
        }
    }

    false
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Task 1 tests: process detection ──────────────────────────────────────

    /// PID 1 (launchd on macOS / init on Linux) is not nvim.
    #[test]
    fn detect_neovim_false_for_pid_1() {
        assert!(!detect_neovim(1), "PID 1 must not be detected as nvim");
    }

    /// A non-nvim process must return None for socket discovery.
    #[test]
    fn discover_nvim_socket_none_for_non_nvim() {
        // PID 1 is definitely not nvim and has no --listen flag.
        let result = discover_nvim_socket(1);
        assert!(result.is_none(), "PID 1 must not have an nvim socket");
    }

    /// NeovimState::check(None) must return is_nvim=false.
    #[test]
    fn neovim_state_check_none_returns_not_nvim() {
        let state = NeovimState::check(None);
        assert!(!state.is_nvim, "check(None) must return is_nvim=false");
        assert!(state.socket_path.is_none());
    }

    /// NeovimState is fresh immediately after creation.
    #[test]
    fn neovim_state_is_fresh_after_creation() {
        let state = NeovimState::check(None);
        assert!(
            state.is_fresh(),
            "state must be fresh immediately after check()"
        );
    }

    // ── Task 2 tests: msgpack serialization ──────────────────────────────────

    /// A msgpack-RPC request [0, 1, "nvim_list_wins", []] must round-trip
    /// through encode+decode correctly.
    #[test]
    fn msgpack_request_roundtrip() {
        let request = Value::Array(vec![
            Value::Integer(0.into()),
            Value::Integer(1u32.into()),
            Value::String("nvim_list_wins".into()),
            Value::Array(vec![]),
        ]);

        let mut buf = Vec::new();
        rmpv::encode::write_value(&mut buf, &request).expect("encode must succeed");

        let mut cursor = std::io::Cursor::new(&buf);
        let decoded = rmpv::decode::read_value(&mut cursor).expect("decode must succeed");

        assert_eq!(request, decoded, "encoded request must round-trip");
    }

    /// A msgpack-RPC response [1, 1, nil, [42]] must decode with result=[42].
    #[test]
    fn msgpack_response_decode() {
        let response = Value::Array(vec![
            Value::Integer(1.into()),
            Value::Integer(1u32.into()),
            Value::Nil,
            Value::Array(vec![Value::Integer(42i64.into())]),
        ]);

        let mut buf = Vec::new();
        rmpv::encode::write_value(&mut buf, &response).expect("encode must succeed");

        let mut cursor = std::io::Cursor::new(&buf);
        let decoded = rmpv::decode::read_value(&mut cursor).expect("decode must succeed");

        if let Value::Array(arr) = &decoded {
            assert_eq!(arr.len(), 4);
            assert!(matches!(&arr[2], Value::Nil));
            if let Value::Array(result) = &arr[3] {
                assert_eq!(result.len(), 1);
                assert_eq!(result[0], Value::Integer(42i64.into()));
            } else {
                panic!("result field must be an array");
            }
        } else {
            panic!("decoded value must be an array");
        }
    }

    // ── Task 2 tests: direction logic with mock positions ─────────────────────

    /// Two windows side-by-side: window at (0,0) has a Right neighbour at (0,40).
    #[test]
    fn has_neighbor_right_found() {
        let positions = [(0i64, 0i64), (0, 40)];
        assert!(has_neighbor_in_positions(&positions, 0, Direction::Right));
    }

    /// Looking Left from (0,40): finds neighbour at (0,0).
    #[test]
    fn has_neighbor_left_found() {
        let positions = [(0i64, 0i64), (0, 40)];
        assert!(has_neighbor_in_positions(&positions, 1, Direction::Left));
    }

    /// Looking Right from the rightmost window: no neighbour.
    #[test]
    fn has_neighbor_right_edge_no_neighbor() {
        let positions = [(0i64, 0i64), (0, 40)];
        assert!(!has_neighbor_in_positions(&positions, 1, Direction::Right));
    }

    /// Vertical split: window at (0,0) has a Down neighbour at (20,0).
    #[test]
    fn has_neighbor_down_found() {
        let positions = [(0i64, 0i64), (20, 0)];
        assert!(has_neighbor_in_positions(&positions, 0, Direction::Down));
    }

    /// Vertical split: window at (20,0) has an Up neighbour at (0,0).
    #[test]
    fn has_neighbor_up_found() {
        let positions = [(0i64, 0i64), (20, 0)];
        assert!(has_neighbor_in_positions(&positions, 1, Direction::Up));
    }

    /// Looking Up from the topmost window: no neighbour.
    #[test]
    fn has_neighbor_up_edge_no_neighbor() {
        let positions = [(0i64, 0i64), (20, 0)];
        assert!(!has_neighbor_in_positions(&positions, 0, Direction::Up));
    }

    /// Single window: no neighbour in any direction.
    #[test]
    fn has_neighbor_single_window_no_neighbor() {
        let positions = [(0i64, 0i64)];
        assert!(!has_neighbor_in_positions(&positions, 0, Direction::Right));
        assert!(!has_neighbor_in_positions(&positions, 0, Direction::Left));
        assert!(!has_neighbor_in_positions(&positions, 0, Direction::Up));
        assert!(!has_neighbor_in_positions(&positions, 0, Direction::Down));
    }

    /// Diagonal window does NOT count as a neighbour (different row AND col).
    #[test]
    fn diagonal_window_not_a_neighbor() {
        // Current at (0,0), other at (10,40) — diagonal, not aligned.
        let positions = [(0i64, 0i64), (10, 40)];
        assert!(!has_neighbor_in_positions(&positions, 0, Direction::Right));
        assert!(!has_neighbor_in_positions(&positions, 0, Direction::Down));
    }
}
