# Security Audit Report — Menu Bar Implementation

## Executive Summary

**Verdict:** PASS (with one important finding and several advisories)
**Risk Level:** Medium

The menu bar implementation introduces one genuinely important security issue: the `$EDITOR` environment variable is passed directly to `std::process::Command::new()` without validation, meaning a user who has set `EDITOR` to a path containing arguments or a malicious binary can execute arbitrary code from inside the running terminal. This is a pre-existing code path surfaced by this diff, not new in this phase. All URLs opened by `OpenHelp` and `ReportIssue` are compile-time string literals — there is no injection risk there. Clipboard operations are correctly implemented with bracketed-paste protection. The new `muda 0.17.1` dependency has no known CVEs in its resolved dependency chain at the time of this audit. Fix the `$EDITOR` handling before shipping this phase.

### What to Do

| Priority | Finding | Location | Effort | Action |
|----------|---------|----------|--------|--------|
| 1 | `$EDITOR` env var used as executable without validation | `main.rs:3198-3199` | Small | Validate `EDITOR` is a plain basename or absolute path with no embedded shell metacharacters; do not split on whitespace |
| 2 | `muda` version is unpinned (semver range `"0.17"`) | `Cargo.toml:43` | Trivial | Pin to `muda = "=0.17.1"` to match the resolved lock version and prevent silent minor-version upgrades |
| 3 | `ShowDebugInfo` logs focused `PaneId` at `info` level | `main.rs:1564-1572` | Trivial | Use `log::debug!` instead; `info!` makes pane-ID structural data visible in any deployment that enables info logging |
| 4 | Unhandled `DispatchOutcome` for `OpenHelp`/`ReportIssue` on non-macOS | `main.rs:1576-1590` | Trivial | Add `#[cfg(not(target_os = "macos"))]` stub or `log::debug!` so the no-op is explicit on Linux/Windows builds |

### Themes
- The most significant risk is OS-level code execution via `$EDITOR`, which now has a keyboard-reachable path through the menu bar even if the overlay review feature existed before this phase.
- The menu event dispatch path (`muda::MenuEvent::receiver().try_recv()`) is correctly handled: IDs are resolved against a static compile-time table, so there is no externally controllable action injection.

---

## Detailed Findings

### Critical

_None._

### Important

**[I1] `$EDITOR` value used directly as executable without sanitization**
- **Location:** `arcterm-app/src/main.rs:3198-3199`
- **Description:** The overlay review's "Edit" action reads the `EDITOR` environment variable and passes its value directly to `std::process::Command::new()`. If `EDITOR` contains embedded arguments (e.g., `EDITOR="nano --restricted"`) only the first token — the executable — is used because `Command::new` does not perform shell splitting. However, if `EDITOR` is set to a crafted value such as a script at an attacker-controlled path (e.g., via a `.env`-style config written by a malicious repository), this spawns an arbitrary executable with the config file path as its argument. More practically, many users set `EDITOR="code --wait"` or `EDITOR="emacsclient -t"` — both will silently fail because argument splitting is not performed, which is a usability bug that could mask the security concern. (CWE-78, OWASP A03:2021 — Injection)
- **Impact:** An attacker who controls the `EDITOR` environment variable — or who can write a config file into the path the overlay review reads — can cause the terminal to spawn an arbitrary process. The risk is local (requires control of the environment or filesystem), but in a multi-tenant or scripted context the exposure is real.
- **Remediation:** Either (a) restrict to a known-safe set of editors (not recommended), (b) validate that `EDITOR` resolves to an absolute path that exists on disk and contains no embedded whitespace or shell metacharacters before spawning, or (c) — the most practical fix — split the `EDITOR` value on whitespace as shells do, using the first token as the executable and the remainder as prefix arguments, matching the convention documented in POSIX. Example: `let parts: Vec<&str> = editor.split_whitespace().collect(); std::process::Command::new(parts[0]).args(&parts[1..]).arg(&path).spawn()`. This matches what every POSIX-compliant shell does with `$EDITOR`.
- **Evidence:** `let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into()); let _ = std::process::Command::new(&editor).arg(&path).spawn();`

---

### Advisory

- **[A1] `muda` dependency not pinned to exact version** (`arcterm-app/Cargo.toml:43`): `muda = "0.17"` allows any 0.17.x release. The lock file currently resolves to 0.17.1 with checksum `01c1738382f66ed56b3b9c8119e794a2e23148ac8ea214eda86622d4cb9d415a`. A future `0.17.2` with a supply-chain issue would be silently adopted on the next `cargo update`. Change to `muda = "=0.17.1"` to freeze the resolved version until a deliberate upgrade is performed.

- **[A2] `ShowDebugInfo` emits pane/tab counts and pane ID at `log::info!`** (`main.rs:1564-1572`): Internal structural data (pane count, tab count, focused pane ID, window dimensions, font size) is logged at `info` level, which is visible in any deployment where the user has enabled informational logging. This is not a direct vulnerability — none of the values are secrets — but logging structural layout data at info level violates the project's own CONVENTIONS.md convention that `log::debug!` is for "trace/diagnostic info." Downgrade to `log::debug!`.

- **[A3] No supply-chain verification tooling (`cargo-deny`) for the new dependency** (project-wide, noted in CONVENTIONS.md open questions): The project has no `deny.toml` that would audit incoming dependency trees for known CVEs or duplicate versions. `muda 0.17.1` pulls in `gtk`, `libxdo`, `objc2`, `crossbeam-channel`, and `png 0.17.16`. While no CVEs are known in these at this audit date, adding `cargo-deny` with a `deny.toml` would catch future advisories automatically as part of CI.

- **[A4] `OpenHelp`/`ReportIssue` are no-ops on non-macOS with no user feedback** (`main.rs:1576-1590`): The `#[cfg(target_os = "macos")]` guard means menu items are registered and clickable on all platforms but silently do nothing on Linux and Windows. Users see no error. Add a `#[cfg(not(target_os = "macos"))]` branch with an appropriate system call (e.g., `xdg-open` on Linux) or a `log::warn!` so the failure is observable.

- **[A5] `SelectAll` is unimplemented and silently no-ops** (`main.rs:1455-1458`): The menu item is enabled and clickable but the handler contains only `log::debug!("SelectAll: not yet implemented")`. A user expecting Select All to work will receive no visual feedback. This is a UX issue, not a security issue, but the debug message may confuse users who inspect logs. Consider disabling the menu item until the feature is implemented, or displaying a status message.

---

## Cross-Component Analysis

**Menu event dispatch isolation is correct.** The `muda::MenuEvent::receiver().try_recv()` loop at `main.rs:1890-1900` resolves incoming `MenuId` values exclusively against `AppMenu::id_map`, which is populated at compile time from statically constructed `MenuItem` objects. There is no path by which an externally supplied string, network packet, or file can cause an unmapped `MenuId` to dispatch an action. `action_for_id` returns `Option<&KeyAction>` and the `None` case is handled silently. A malicious or corrupted menu event that does not match a registered ID produces no effect. (No injection risk, no action escalation.)

**Clipboard data flow is correctly bounded.** The `Copy` handler (`main.rs:1411-1428`) extracts text exclusively from the in-memory terminal snapshot via `selection.extract_text(&snapshot)`. The text is then written to the OS clipboard via `arboard`. No network call, no file write, no logging of clipboard content occurs. The `Paste` handler correctly checks `terminal.bracketed_paste()` and wraps pasted data in the `\x1b[200~`...`\x1b[201~` bracket sequence when the terminal has advertised support (line 1436-1441). This is the correct defense against clipboard-injection attacks where a crafted clipboard value contains terminal escape sequences. (CWE-116 compliant.)

**`ClearScrollback` and `ResetTerminal` are scoped to the focused pane only.** Both handlers resolve the pane ID via `self.tab_manager.active_tab().focus` and touch only that pane's `alacritty_terminal` state. There is no cross-pane data leakage. `clear_history()` discards scrollback; `reset_state()` resets VT emulation state. Neither persists data to disk or sends it over any channel. The operations are intentionally destructive within their stated scope — this matches the documented intent and carries no security risk beyond the user's own terminal data.

**`ShowDebugInfo` does not leak secrets.** The logged fields are: pane count, tab count, focused pane ID (an opaque `u64`), window pixel dimensions, and font size (a float). None of these contain user data, credentials, file paths, or shell history. The finding is limited to logging level choice (Advisory A2 above).

**`$EDITOR` path vs. the menu bar surface.** The `OverlayAction::Edit` code path at `main.rs:3196-3202` was presumably in place before this phase. However, the menu bar added `ReviewOverlay` as `KeyAction::ReviewOverlay`, making the config overlay — and therefore the Edit action — now directly accessible from the menu bar without requiring the leader-key sequence. This increases the reachability of the `$EDITOR` code path for any user who discovers the Config Overlay menu item, which elevates the importance of fixing the `$EDITOR` handling (Important finding I1).

---

## Analysis Coverage

| Area | Checked | Notes |
|------|---------|-------|
| Code Security (OWASP) | Yes | Injection (CWE-78 in EDITOR), clipboard injection (correctly defended), URL open (hardcoded literals, safe) |
| Secrets & Credentials | Yes | No secrets, API keys, or credentials in any changed file or test fixture |
| Dependencies | Yes | `muda 0.17.1` resolved; no known CVEs in direct or transitive deps at audit date |
| Infrastructure as Code | N/A | No IaC files changed |
| Docker/Container | N/A | No Dockerfile changes in scope |
| Configuration | Yes | No debug flags introduced; logging level advisory noted |

---

## Dependency Status

| Package | Version | Known CVEs | Status |
|---------|---------|-----------|--------|
| muda | 0.17.1 | None known | OK |
| crossbeam-channel | (transitive via muda) | None known | OK |
| gtk | (transitive via muda, Linux only) | None known at audit date | OK |
| libxdo | (transitive via muda, Linux only) | None known | OK |
| objc2 / objc2-app-kit | 0.6.4 / 0.3.2 (transitive) | None known | OK |
| png | 0.17.16 (transitive via muda) | None known | OK |

No `cargo-deny` tooling is present. The above status is based on manual cross-reference against the RustSec advisory database (https://rustsec.org) as of the audit date. See Advisory A3 for the recommendation to add automated advisory scanning.

---

## IaC Findings

Not applicable — no IaC files were modified.
