# Security Audit Report — Phase 11

## Executive Summary

**Verdict:** PASS
**Risk Level:** Low

Phase 11 is a defensive hardening phase and it delivers on that goal. The three changes — scrollback cap, GPU init error handling, and async image decode — all move the codebase in the right direction. No exploitable vulnerabilities, secrets, or unsafe code were introduced. One advisory gap is worth fixing before the cap is considered complete: `scrollback_lines = 0` is accepted by config validation without clamping, and while it does not crash today (the grid cap logic handles it correctly with a `while len > 0` condition that is immediately false), it silently disables scrollback with no user warning. All other findings are informational.

### What to Do

| Priority | Finding | Location | Effort | Action |
|----------|---------|----------|--------|--------|
| 1 | No minimum bound on `scrollback_lines` | `config.rs:158` | Trivial | Add `if self.scrollback_lines == 0` warn-and-clamp to 1 (or document 0 as valid) |
| 2 | Image decode error logs `image_id` verbatim | `terminal.rs:127-129` | Trivial | Confirm `image_id` is never attacker-controlled in a privacy-sensitive way (it is a u32 from the Kitty protocol — acceptable) |

### Themes

- Input validation is present but one-sided: the upper bound on `scrollback_lines` is enforced, the lower bound is not.
- Error propagation is now consistent across the GPU stack — a clear improvement over the previous `expect()` pattern.

---

## Detailed Findings

### Critical

None.

### Important

None.

### Advisory

- No lower bound enforced on `scrollback_lines = 0` (`arcterm-app/src/config.rs:158`) — the grid handles 0 safely (the cap loop exits immediately), but the user gets no warning and effectively has no scrollback. Add a minimum of 1 and a `log::warn!` to match the upper-bound pattern already established.

- `image_channels` cleanup is present in all pane-close paths (`main.rs:885, 1688, 2972, 2998, 3078, 3275, 3290, 3334`) but is not present in a `spawn_default_pane` error path; that path calls `std::process::exit(1)` so no leak occurs, but the pattern is worth noting.

- The image channel is bounded at 32 (`terminal.rs:71`). Under adversarial input (a PTY stream delivering many large images faster than the render loop drains them), `try_send` will warn and silently drop images. This is the correct back-pressure choice, but the warning message (`"Kitty image channel send failed: {e}"`) does not include the `image_id`, making it harder to diagnose in logs. Low severity.

---

## Cross-Component Analysis

**GPU init failure path is safe.** After `Renderer::new` fails, `event_loop.exit()` is called and the function returns immediately (`main.rs:1019-1022`). No subsequent code in `resumed()` is reached; there is no `AppState` constructed on this path, so there is no possibility of accessing uninitialized state.

**Scrollback cap is applied at both load sites.** `ArctermConfig::validate()` is called in `load_from_file` and in the layered `load` path (`config.rs:213, 319`). Both the `Default` impl path and the explicit parse path run validation before the config is handed to the rest of the system.

**Async image channel lifecycle matches pane lifecycle.** Every site that inserts a pane into `panes` and `pty_channels` also inserts a corresponding entry in `image_channels`. Every site that removes from `pty_channels` also removes from `image_channels`. Reviewed: `spawn_pane_with_cwd`, workspace restore in `resumed`, all pane-close paths in `about_to_wait` and `execute_key_action`. No orphaned receivers found.

**`spawn_blocking` safety.** The closure captures `tx` (a cheap clone of `mpsc::Sender`) and `decoded_bytes` / `meta` by move. No shared mutable state is accessed inside the closure. `try_send` is used instead of `send`, so the blocking thread cannot block waiting on the render loop. This is correct.

---

## Analysis Coverage

| Area | Checked | Notes |
|------|---------|-------|
| Code Security (OWASP) | Yes | No injection, no auth changes, no deserialization of untrusted external data beyond image bytes via `image` crate |
| Secrets & Credentials | Yes | No secrets, tokens, API keys, or credentials in diff |
| Dependencies | Yes | No new dependencies added in this phase |
| Infrastructure as Code | N/A | No IaC changes |
| Docker/Container | N/A | No container changes |
| Configuration | Yes | Config validation logic reviewed; advisory finding on missing lower bound |

---

## Dependency Status

No dependency changes in this phase. No new crates introduced.

| Package | Version | Known CVEs | Status |
|---------|---------|------------|--------|
| wgpu | 28.x | None known | OK |
| image | workspace | None known | OK |
| tokio | workspace | None known | OK |

---

## IaC Findings

Not applicable for this phase.
