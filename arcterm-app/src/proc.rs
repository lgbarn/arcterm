//! Shared process introspection helpers.
//!
//! Provides platform-specific functions for reading the process name (comm)
//! and command-line arguments for a given PID. Extracted from `neovim.rs`
//! so that both `neovim.rs` and `ai_detect.rs` can share the same implementations
//! without duplication.

// ── Platform-specific detection helpers ──────────────────────────────────────

/// Return the process name (comm) for `pid`, or `None` on error.
#[allow(dead_code)]
#[cfg(target_os = "macos")]
pub fn process_comm(pid: u32) -> Option<String> {
    // libc::proc_name writes the short process name (up to MAXCOMLEN bytes)
    // into a caller-supplied buffer. Returns the number of bytes written.
    const PROC_NAME_MAX: usize = 64;
    let mut buf = [0u8; PROC_NAME_MAX];
    let ret = unsafe {
        libc::proc_name(
            pid as libc::c_int,
            buf.as_mut_ptr() as *mut libc::c_void,
            buf.len() as u32,
        )
    };
    if ret <= 0 {
        return None;
    }
    let len = (ret as usize).min(buf.len());
    // Find null terminator if present.
    let end = buf[..len].iter().position(|&b| b == 0).unwrap_or(len);
    String::from_utf8(buf[..end].to_vec()).ok()
}

#[cfg(target_os = "linux")]
pub fn process_comm(pid: u32) -> Option<String> {
    let path = format!("/proc/{pid}/comm");
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn process_comm(_pid: u32) -> Option<String> {
    None
}

// ── Process args helpers ──────────────────────────────────────────────────────

/// Return the command-line arguments for `pid` as a vector of strings, or
/// `None` on error / unsupported platform.
#[allow(dead_code)]
#[cfg(target_os = "macos")]
pub fn process_args(pid: u32) -> Option<Vec<String>> {
    use libc::{c_int, c_void, CTL_KERN, KERN_PROCARGS2};

    // First call: determine required buffer size.
    let mut mib: [c_int; 3] = [CTL_KERN, KERN_PROCARGS2, pid as c_int];
    let mut size: libc::size_t = 0;

    let ret = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret != 0 {
        return None;
    }

    let mut buf: Vec<u8> = vec![0u8; size];

    let ret = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            buf.as_mut_ptr() as *mut c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret != 0 {
        return None;
    }
    buf.truncate(size);

    // KERN_PROCARGS2 layout:
    //   [0..4]  argc (i32, little-endian)
    //   [4..]   exec_path (null-terminated), padding, then argv[0..argc] null-terminated strings
    if buf.len() < 4 {
        return None;
    }
    let argc = i32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;

    // Skip past exec_path (first null-terminated string after argc).
    let mut pos = 4;
    while pos < buf.len() && buf[pos] != 0 {
        pos += 1;
    }
    // Skip null bytes / padding.
    while pos < buf.len() && buf[pos] == 0 {
        pos += 1;
    }

    // Now parse argc null-terminated strings.
    let mut args = Vec::with_capacity(argc);
    let mut count = 0;
    while pos < buf.len() && count < argc {
        let start = pos;
        while pos < buf.len() && buf[pos] != 0 {
            pos += 1;
        }
        if let Ok(s) = std::str::from_utf8(&buf[start..pos]) {
            args.push(s.to_string());
        }
        pos += 1; // skip null
        count += 1;
    }

    Some(args)
}

#[cfg(target_os = "linux")]
pub fn process_args(pid: u32) -> Option<Vec<String>> {
    let path = format!("/proc/{pid}/cmdline");
    let data = std::fs::read(&path).ok()?;
    Some(
        data.split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .filter_map(|s| std::str::from_utf8(s).ok().map(String::from))
            .collect(),
    )
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn process_args(_pid: u32) -> Option<Vec<String>> {
    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// process_comm for PID 1 returns Some (it is init/launchd) or None — but never panics.
    #[test]
    fn proc_comm_does_not_panic() {
        let _ = process_comm(1);
    }

    /// process_args for PID 1 returns Some or None — but never panics.
    #[test]
    fn proc_args_does_not_panic() {
        let _ = process_args(1);
    }
}
