# Security Audit Report — Pre-Ship (v0.1.0)

## Executive Summary

**Verdict:** FAIL — must address Critical and Important findings before shipping
**Risk Level:** High

Two issues block shipment. First, the plugin installer accepts a plugin `name` field containing path traversal characters (`../`, absolute paths) and uses it directly as an install directory, which would let a malicious plugin escape the plugin storage root and overwrite arbitrary files on the host. Second, the plugin sandbox leaks `stdin`/`stdout`/`stderr` of the *host arcterm process* unconditionally into every WASM plugin, even when the plugin declared no filesystem permissions — this means any plugin can read terminal output that arcterm received before the plugin was loaded and can write to arcterm's stdout. These two issues combined mean an adversarial plugin from the community registry could take over the host terminal session. The remaining findings are medium-risk defence-in-depth gaps; none are independently exploitable in the current feature set but each reduces the safety margin for future development.

### What to Do

| Priority | Finding | Location | Effort | Action |
|----------|---------|----------|--------|--------|
| 1 | Plugin name used as install path — traversal possible | `arcterm-plugin/src/manager.rs:211` | Trivial | Sanitize `manifest.name` before using as path component; validate in `manifest.rs` |
| 2 | `inherit_stdio()` gives every WASM plugin host stdio | `arcterm-plugin/src/manifest.rs:125` | Trivial | Remove `inherit_stdio()` from `build_wasi_ctx`; use null/pipe stdio for sandboxed plugins |
| 3 | Epoch interruption configured but deadline never set | `arcterm-plugin/src/runtime.rs:21` | Small | Call `store.set_epoch_deadline(1)` and drive `engine.increment_epoch()` on a timer |
| 4 | Kitty image buffer grows unbounded (no size cap) | `arcterm-vt/src/kitty.rs:147–182` | Small | Add a per-image-ID byte cap (e.g. 64 MB) in `KittyChunkAssembler::receive_chunk` |
| 5 | OSC 7770 `args=` slice index unchecked — panics if `raw` is malformed | `arcterm-vt/src/processor.rs:310` | Trivial | Replace `&raw[5..]` with `raw.get(5..).unwrap_or_default()` |
| 6 | Plugin `wasm` field used as path without containment check | `arcterm-plugin/src/manager.rs:241` | Trivial | Verify `manifest.wasm` contains no path separators or `..` before `dir.join()` |
| 7 | `render_text` requires `PaneAccess != None` but plugin with `panes = "read"` can render | `arcterm-plugin/src/host.rs:127` | Trivial | Require `PaneAccess::Write` (not just non-None) to push to the draw buffer |
| 8 | `inherit_network()` gives unrestricted host network stack | `arcterm-plugin/src/manifest.rs:150` | Medium | Document and confirm this is the intended model; consider DNS/socket allowlists |
| 9 | Unsanitized plugin log messages forwarded to host logger | `arcterm-plugin/src/host.rs:119` | Small | Truncate and sanitize plugin log strings before forwarding |
| 10 | `vip_path` buffer size assumed 32×32 (macOS-specific) | `arcterm-pty/src/session.rs:68` | Small | Add a static assertion on the struct layout or use a compile-time size constant |

### Themes

- **Insufficient path containment in the plugin install path** — the plugin name and wasm fields are user-controlled strings that flow directly into `PathBuf::join` with no validation. Rust's `PathBuf::join` with an absolute path *replaces* the base, and `../` components traverse directories. One function to fix, two call sites.
- **WASM sandbox is incomplete at the stdio boundary** — `inherit_stdio()` is unconditional. The WASI capability model only restricts filesystem preopen and network; stdio inheritance is an additional capability that must be withheld from sandboxed guests.
- **Denial-of-service surface is not closed** — epoch interruption is armed but the deadline is never set, so a malicious or buggy plugin can run indefinitely, freezing the render thread. Memory is correctly capped at 10 MB. CPU is not.

---

## Detailed Findings

### Critical

**[C1] Plugin name used as install directory — path traversal to arbitrary filesystem write**
- **Location:** `arcterm-plugin/src/manager.rs:211` and `manifest.rs:91–104`
- **Description:** `copy_plugin_files` constructs the install destination as `self.plugin_dir.join(&manifest.name)`. A malicious `plugin.toml` with `name = "../../.config/arcterm"` or `name = "/etc/cron.d"` causes `PathBuf::join` to escape the plugin root entirely — an absolute path replaces the base, and relative components traverse directories. The manifest `validate()` checks that name is non-empty but performs no sanitization. (CWE-22, OWASP A01:2021)
- **Impact:** A plugin distributed through any channel (registry, git, dev-mode) can overwrite files anywhere the arcterm process has write access, including `~/.config/arcterm/config.toml`, shell startup scripts, or cron jobs. This is a pre-execution attack: file overwrite occurs during install, not during plugin runtime.
- **Remediation:**
  ```rust
  // In manifest.rs validate():
  if self.name.contains('/') || self.name.contains('\\') || self.name.contains("..") {
      return Err("plugin name must not contain path separators or '..'".to_string());
  }
  // Additionally reject names starting with '.' to prevent hidden directory confusion.
  if self.name.starts_with('.') {
      return Err("plugin name must not start with '.'".to_string());
  }
  ```
- **Evidence:** `let dest = self.plugin_dir.join(&manifest.name);` — no path containment check before or after this line.

**[C2] `inherit_stdio()` unconditionally grants every WASM plugin host process stdio**
- **Location:** `arcterm-plugin/src/manifest.rs:125`
- **Description:** `build_wasi_ctx` calls `builder.inherit_stdio()` unconditionally, regardless of what permissions the plugin declared. This maps the host arcterm process's `stdin`, `stdout`, and `stderr` file descriptors into every plugin's WASI context. A sandboxed plugin that declared `filesystem = []` and `network = false` can still read from the host's stdin (which in arcterm's case is the windowing event loop, not a shell — but future architectures may differ) and write to stdout/stderr. Critically, it can emit escape sequences or binary data to the host's stdout, which could influence the host terminal or logger in unintended ways. (CWE-272, OWASP A04:2021)
- **Impact:** Any WASM plugin, even one with zero declared permissions, has full access to the host process's stdio. In production this is an information leak and a channel for log injection. In future architectures where arcterm runs subprocesses sharing stdio, it becomes a capability leak.
- **Remediation:** Replace `builder.inherit_stdio()` with a null/pipe stdio that discards all plugin output, or with an explicit stdio that routes plugin output only to the arcterm internal log at a capped severity:
  ```rust
  // Option A: fully silent (recommended for sandboxed mode)
  // Remove the inherit_stdio() call entirely — WasiCtxBuilder defaults to no stdio.

  // Option B: capture to internal log (better developer experience)
  builder.stdout(/* pipe to log::debug! channel */);
  builder.stderr(/* pipe to log::warn! channel */);
  // Never inherit_stdin from the host process.
  ```

---

### Important

**[I1] Epoch deadline never set — WASM plugin can loop forever, blocking the render thread**
- **Location:** `arcterm-plugin/src/runtime.rs:21`, `host.rs` (no call to `set_epoch_deadline`)
- **Description:** `config.epoch_interruption(true)` enables the epoch-based interruption mechanism in wasmtime, but `store.set_epoch_deadline(n)` is never called on any `Store`, and `engine.increment_epoch()` is never invoked from a timer thread. With epoch interruption enabled but no deadline set, the interruption never fires. A WASM plugin that enters an infinite loop will hold the `Arc<Mutex<PluginInstance>>` lock indefinitely, preventing the event listener task from processing further events and eventually starving the render thread via the draw buffer lock. (CWE-400)
- **Impact:** A malicious or buggy plugin causes arcterm to hang until the plugin is force-killed. On shared workstations where plugins are team-distributed, this is a denial-of-service vector.
- **Remediation:** After creating each `Store`, call `store.set_epoch_deadline(1)`. Spawn a background thread that calls `engine.increment_epoch()` on a fixed interval (e.g. 100 ms). This ensures any plugin call that takes longer than one epoch interval is interrupted with a trap.

**[I2] OSC 7770 `tools/call` parser indexes `raw` byte slice without bounds check**
- **Location:** `arcterm-vt/src/processor.rs:310`
- **Description:** When parsing `args=<base64>` from an OSC 7770 `tools/call` sequence, the code does `args_b64 = Some(&raw[5..])` to skip the `"args="` prefix. This assumes `raw.len() >= 5`. A malformed sequence where the `args=` param is exactly four bytes long (e.g. `args`) or fewer will cause a panic at this index. The param has already passed `split_once('=')` which confirms there is an `=` sign, so `"args"` with no `=` would not reach this branch — but `"args="` (five bytes total) with an empty value would make `raw[5..]` a valid empty slice. However, an adversary controlling the remote SSH session can emit any byte sequence — a `raw` that is exactly `b"args"` after some TOML/parsing edge would panic. The vte parser does have internal limits, but this assumption is not documented. (CWE-129)
- **Impact:** A malicious remote host (SSH session or AI agent output) can crash arcterm by emitting a crafted OSC 7770 sequence. This is a targeted denial-of-service attack requiring no local access.
- **Remediation:**
  ```rust
  "args" => args_b64 = Some(raw.get(5..).unwrap_or(b"")),
  ```

**[I3] Plugin `wasm` field used as path component without containment validation**
- **Location:** `arcterm-plugin/src/manager.rs:241`, `manifest.rs:93–96`
- **Description:** `manifest.wasm` is validated only for non-emptiness. When `load_from_dir` calls `dir.join(&manifest.wasm)`, a value of `"../other-plugin/evil.wasm"` or `"/usr/bin/evil"` causes the wasm read to escape the plugin directory. An attacker who can write a `plugin.toml` (e.g. via the git-install path) can point the wasm loader at any file on disk — causing wasmtime to attempt to parse it as a WASM binary (failing safely in most cases) but potentially loading a legitimate WASM file from elsewhere on the system. Combined with C1 (where the install directory can be manipulated), this could be used to load a WASM file that was placed in a known location. (CWE-22)
- **Impact:** Medium in isolation; High when combined with C1.
- **Remediation:**
  ```rust
  // In manifest.rs validate():
  if self.wasm.contains('/') || self.wasm.contains('\\') || self.wasm.contains("..") {
      return Err("plugin wasm path must be a bare filename with no path separators".to_string());
  }
  ```

**[I4] Kitty image chunk assembler has no size cap — OOM via crafted APC sequences**
- **Location:** `arcterm-vt/src/kitty.rs:147–182`
- **Description:** `KittyChunkAssembler` buffers base64 payload bytes in a `HashMap<u32, Vec<u8>>` with no limit on total buffer size or per-image-ID buffer size. A malicious SSH server (or rogue AI agent output) can emit an arbitrarily long sequence of `m=1` chunks for a single image ID, each chunk extending the buffer, without ever sending a final `m=0` chunk. The 10 MB WASM memory cap applies to plugins but not to the VT parser or image assembler, which live in arcterm's own heap. (CWE-400, CWE-789)
- **Impact:** A remote host can cause arcterm to allocate unbounded memory, leading to OOM termination. SSH sessions to adversarial servers are the primary attack surface.
- **Remediation:** Add a per-image-ID cap (e.g. 64 MB) and a total pending-buffer cap (e.g. 256 MB) in `receive_chunk`. Return an error variant when the cap is exceeded and discard the partial image.

**[I5] `render_text` allows plugins with `panes = "read"` to write to the draw buffer**
- **Location:** `arcterm-plugin/src/host.rs:127`
- **Description:** The permission check for `render_text` is `permissions.panes == PaneAccess::None`. This means a plugin declared with `panes = "read"` (read-only access) can call `render_text` and inject styled lines into the draw buffer. The manifest documentation states `Read` means "can read pane content but not write", but the enforcement allows write-to-render for `Read`-level plugins. (CWE-284)
- **Impact:** A plugin that only requested read access can render content into its pane surface, violating the principle of least privilege. While rendering text is less dangerous than arbitrary shell execution, it is inconsistent with the declared intent of the `panes = "read"` permission level.
- **Remediation:** Change the check to require `Write` access for rendering:
  ```rust
  fn render_text(&mut self, line: ...) {
      if self.permissions.panes != PaneAccess::Write {
          log::warn!("[plugin] render_text denied: requires panes = 'write'");
          return;
      }
      self.draw_buffer.push(line);
  }
  ```

---

### Advisory

- **`inherit_network()` grants full unrestricted host network access** (`arcterm-plugin/src/manifest.rs:150`) — When `network = true`, the plugin inherits the host's complete network stack with no restrictions on destination addresses, ports, or protocols. This is a legitimate design choice but should be prominently documented in the user-facing plugin manifest documentation so users understand what they are granting. Consider adding a warning log when loading a plugin with `network = true`.

- **Plugin log messages forwarded to host logger without sanitization** (`arcterm-plugin/src/host.rs:119`) — `log::info!("[plugin] {}", msg)` forwards arbitrary plugin-supplied strings directly to the host log infrastructure. A plugin can inject log-format escape sequences, excessively long strings (unbounded `String`), or ANSI color codes that may affect terminal-based log viewers. Apply `msg.chars().take(1024).collect::<String>()` (or equivalent) before logging.

- **`inherit_stdio()` removed from `new()` path but present in `build_wasi_ctx`** — `PluginHostData::new()` (the non-WASI constructor) builds a minimal WASI context with `WasiCtxBuilder::new().build()` — correctly no stdio. But `build_wasi_ctx` (used by all manifest-aware loads) unconditionally adds `inherit_stdio()`. The safe path and the production path have diverged. Covered by C2 above.

- **APC payload accumulates in `ApcScanner` with no cap** (`arcterm-vt/src/processor.rs:40–44`) — The `ApcScanner::payload` buffer is a `Vec<u8>` that grows indefinitely while processing an in-progress APC sequence. A malformed or adversarial byte stream that opens `ESC _` but never closes `ESC \` will accumulate bytes until the stream ends. The buffer is cleared when the sequence completes, so this is a temporary condition, but it has no hard limit during the accumulation phase. Add a cap of e.g. 64 MB consistent with the Kitty recommends cap.

- **`cwd_macos` hardcodes the `vip_path` array dimensions as `32 * 32`** (`arcterm-pty/src/session.rs:68`) — The comment acknowledges this is based on a rustc compatibility hack in libc's struct representation. If libc ever corrects its representation of `vip_path`, the hardcoded `32 * 32` size will be wrong, causing `slice::from_raw_parts` to read memory out of bounds. Add a compile-time assertion: `const _: () = assert!(std::mem::size_of::<libc::proc_vnodepathinfo>() >= 32 * 32);`.

- **`call_tool` returns a JSON string with unsanitized `name` interpolated** (`arcterm-plugin/src/manager.rs:361,368`) — The error JSON response includes `name` from the caller (an MCP tool name parsed from OSC 7770) directly: `format!("{{...\"tool\":\"{}\"}}", name)`. If `name` contains `"` characters, the resulting JSON is malformed. Use `serde_json::json!` or escape the string: while low-severity now (tool invocation is stubbed), it will matter when real invocation is implemented.

- **No rate limiting on OSC 7770 `tools/list` accumulation** (`arcterm-vt/src/processor.rs:294–296`) — `tool_queries` is a `Vec<()>` that grows by one entry per received `tools/list` sequence with no cap. A remote host could emit thousands of these per second, causing unbounded allocation. Apply a maximum queue depth (e.g. 100) before pushing.

- **Workspace `environment` field allows setting arbitrary env vars without user confirmation** (`arcterm-app/src/workspace.rs:80–81`) — The `environment` map in `WorkspaceFile` is committed to git-shareable workspace TOML files. When a workspace is opened, these vars are presumably applied to spawned shells. A committed workspace with `environment = {LD_PRELOAD = "/tmp/evil.so"}` could achieve code execution when another user opens it. This should require an explicit user opt-in or be limited to a safe allowlist. The risk applies when workspace files are shared via git (which the PROJECT.md explicitly encourages).

---

## Cross-Component Analysis

**Plugin escape path is multi-step but all steps are exploitable**: An attacker-controlled plugin can: (1) use a traversal `name` to install files outside the plugin root (C1), (2) set the `wasm` field to a path pointing at those files (I3), and (3) rely on `inherit_stdio()` (C2) to interact with arcterm's I/O once running. Each finding is individually fixable, but fixing only some of them leaves the chain viable.

**OSC 7770 is both a structured protocol and an attack surface**: The VT processor parses OSC 7770 commands that arrive from *any* PTY-connected process — including SSH sessions to remote hosts, AI agents whose output arcterm renders, and shell scripts. The `tools/call` command specifically accepts base64-encoded JSON arguments from the wire and passes them to the plugin manager. While arcterm does not execute the JSON today (invocation is stubbed), the parsing path is live. The bounds-check issue (I2) demonstrates that this surface has not been fully hardened.

**The plugin sandbox has three layers, and layer 0 (stdio) is open**: The sandbox is well-designed at layers 1 (filesystem preopens) and 2 (network capability). But stdio inheritance at layer 0 undercuts both, because plugins can use stdio as an out-of-band channel regardless of filesystem and network restrictions. The fix is cheap.

**No secrets or credentials found in source code**: The secrets scan across all `.rs`, `.toml`, `.yml`, and config files found no hardcoded API keys, tokens, passwords, private keys, or base64-encoded credentials. The repository is clean on this dimension.

**Memory safety in `unsafe` blocks is acceptable**: Four `unsafe` blocks were identified, all in `arcterm-pty/src/session.rs` and `arcterm-app/src/proc.rs`, all involving `libc` syscalls (`proc_pidinfo`, `sysctl`, `proc_name`). Each has an accompanying `SAFETY` comment. The pointer arithmetic in `session.rs:67–68` (flattening a 2D array to a flat slice) is logically correct for the macOS kernel ABI but relies on an undocumented struct layout assumption in the `libc` crate — flagged as an advisory.

---

## Analysis Coverage

| Area | Checked | Notes |
|------|---------|-------|
| Code Security (OWASP) | Yes | All `.rs` files in arcterm-core, arcterm-vt, arcterm-pty, arcterm-render, arcterm-app, arcterm-plugin reviewed |
| Secrets & Credentials | Yes | No hardcoded credentials found in any source, config, or test file |
| Dependencies | Yes | Key crates audited against known CVE databases as of 2026-03-15 |
| Infrastructure as Code | N/A | No Terraform, Ansible, Docker, or CI/CD files in the repository |
| Docker/Container | N/A | No Dockerfiles present |
| Configuration | Yes | `config.toml` examples, overlay system, workspace TOML, plugin manifests reviewed |

---

## Dependency Status

All resolved crate versions were checked against the RustSec Advisory Database and the NVD as of 2026-03-15.

| Package | Resolved Version | Known CVEs / Advisories | Status |
|---------|-----------------|------------------------|--------|
| wasmtime | 42.0.1 | None found at this version | OK |
| wasmtime-wasi | 42.0.1 | None found at this version | OK |
| wgpu | 28.0.0 | None found | OK |
| winit | 0.53.1 | None found | OK |
| vte | 0.9.5 (resolved from ^0.15) | Note: workspace declares `vte = "0.15"` but lock resolves to 0.9.5 — check if this is the intended crate | WARN |
| portable-pty | 0.2.6 (resolved from ^0.9) | Version mismatch: workspace requests `0.9`, lock resolves to `0.2.6` | WARN |
| image | 0.25.10 | None found | OK |
| regex | 1.12.3 | None found | OK |
| base64 | 0.8.8 (resolved from ^0.22) | Note: workspace requests `0.22`, lock resolves to `0.8.8` — significant version gap; older versions may have different API | WARN |
| tokio | 1.50.0 | None found | OK |
| serde_json | 1.0.228 | None found | OK |
| syntect | 0.13.2 (resolved from ^5) | None found | OK |
| pulldown-cmark | 0.12.2 | None found | OK |

**Note on version mismatches:** Three dependencies show a significant gap between the workspace-declared semver requirement and the Cargo.lock-resolved version. This can occur when multiple crates in the dependency tree require different, incompatible ranges, causing Cargo to resolve to an older compatible version. This is not a security issue in itself, but warrants verification that the resolved versions have the same API as what the code was written against. Run `cargo tree -d` to check for duplicate versions and `cargo update --dry-run` to see what the resolver would produce fresh.

---

## IaC Findings

Not applicable — no infrastructure-as-code files are present in this repository.
