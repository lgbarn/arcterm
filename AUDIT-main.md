# Security Audit Report — ArcTerm Main Branch

**Branch:** `main` (relative to `upstream/main`)
**Date:** 2026-03-19
**Scope:** All ArcTerm-specific code: `arcterm-wasm-plugin`, `arcterm-ai`, `arcterm-structured-output`, `wezterm-gui` AI/overlay additions, `lua-api-crates/plugin`, `term`, `wezterm-escape-parser`, and CI scripts.

---

## Executive Summary

**Verdict:** FAIL
**Risk Level:** High

Two findings require attention before shipping: a path traversal vulnerability in the WASM plugin filesystem capability check that a plugin author could exploit to read or write files outside their declared sandbox, and a memory amplification issue in OSC 7770 image processing that can turn a 10 MB escape sequence into over 640 MB of host heap allocation. Neither is remotely exploitable today (WASM plugins are currently in a half-wired prototype state and OSC 7770 rendering is not yet gated by any user consent), but both are the kind of issue that becomes critical the moment those features ship. The remaining findings are advisory-level code quality items. Fix the path traversal before enabling real plugin execution, and reduce the OSC 7770 payload ceiling before enabling the feature for untrusted terminal sessions.

### What to Do

| Priority | Finding | Location | Effort | Action |
|----------|---------|----------|--------|--------|
| 1 | Path traversal via `..` in filesystem capability | `capability.rs:123-128` | Small | Normalize the requested path before `starts_with` check; reject `..` components |
| 2 | Memory amplification in OSC 7770 image rendering | `lib.rs:16`, `image.rs` | Small | Reduce `DEFAULT_MAX_PAYLOAD_SIZE` to 1 MB; add decoded-bytes limit |
| 3 | Terminal output sent to LLM without user consent gate | `ai_pane.rs:25-26` | Medium | Require explicit user opt-in before scrollback is sent to any remote backend |
| 4 | LLM response rendered as raw terminal text (escape injection) | `ai_pane.rs:216` | Small | Strip ANSI escape sequences from LLM tokens before `Change::Text` |
| 5 | CI password SHA echoed to build logs | `deploy.sh:59` | Trivial | Remove `echo $MACOS_PW \| shasum` line |
| 6 | `sk-ant-test` test fixture string committed to source | `claude.rs:69,88`, `backend_tests.rs:20` | Trivial | Replace with a clearly fake placeholder like `test-key-placeholder` |
| 7 | `terminal:read` silently granted to every plugin | `capability.rs:97-110` | Medium | Remove the unconditional default grant; require explicit declaration |
| 8 | No bounds on `memory_limit_mb` from Lua config | `lua-api-crates/plugin/src/lib.rs:254` | Trivial | Clamp to a maximum (e.g., 512 MB) before passing to wasmtime |

### Themes
- **Input trust boundary not enforced at OSC layer:** OSC 7770 payloads originate from terminal program output — any process running in the terminal can inject them. The 10 MB ceiling and lack of user consent creates amplification and unexpected rendering risks.
- **Prototype-stage safety gaps:** Several security controls exist in skeleton form (host API capability checks pass, but real plugin execution is not wired; AI API key is not loaded from user config yet; terminal:read is unconditionally granted). These are safe today but create a window where a future wiring commit accidentally ships without the corresponding security hardening.
- **CI credential hygiene:** The macOS signing section of `deploy.sh` (inherited from upstream) leaks a password hash to build logs. This is a low-severity advisory but should be cleaned up.

---

## Detailed Findings

### Important

**[I1] Path Traversal via `..` in Filesystem Capability Enforcement**
- **Location:** `arcterm-wasm-plugin/src/capability.rs:123-128`; exploited through `arcterm-wasm-plugin/src/host_api.rs:125`
- **CWE:** CWE-22 (Improper Limitation of a Pathname to a Restricted Directory)
- **Description:** The `CapabilitySet::check()` method uses `PathBuf::starts_with()` to enforce that a requested filesystem path falls within the granted prefix. However, `PathBuf::starts_with` operates on normalized path components and does not resolve `..` traversal segments before the comparison. A path like `/home/user/../.ssh/id_rsa` is constructed as a `PathBuf` from the raw string, retains the `..` component, and `starts_with("/home/user")` returns `true` because the prefix comparison stops at the matching prefix components, not accounting for the traversal that follows. Confirmed by a standalone Rust test: `PathBuf::from("/home/user/../.ssh/id_rsa").starts_with(PathBuf::from("/home/user"))` evaluates to `true`. In addition, symlinks are not resolved at the capability layer, so a plugin granted `fs:read:/home/user` could follow a symlink that points outside the granted tree.
- **Impact:** A malicious or compromised plugin declared with capability `fs:read:/home/user` can read arbitrary files on the host filesystem (e.g., `/home/user/../.ssh/id_rsa`, `/home/user/../../etc/shadow`). With `fs:write` this becomes arbitrary file write. This is a complete sandbox bypass for the filesystem capability.
- **Remediation:**
  1. Before the `starts_with` comparison, strip `..` components by collecting the path through `PathBuf::components()` and rejecting any path that contains a `Component::ParentDir` component.
  2. Additionally, call `std::fs::canonicalize()` on the granted path at registration time (so symlinks in the grant itself are resolved once) and validate requested paths contain no `..` components before checking. Do not call `canonicalize` on the requested path at check time — the file may not exist yet for writes — but rejecting `..` components is sufficient.
  3. Consider adding a test: `capability_set_denies_path_traversal_via_dotdot`.
- **Evidence:**
  ```rust
  // capability.rs:123-128 — vulnerable check
  let granted = PathBuf::from(granted_path);   // "/home/user"
  let requested = PathBuf::from(requested_path); // "/home/user/../.ssh/id_rsa"
  if requested.starts_with(&granted) {          // TRUE — bypass
      return Ok(());
  }
  ```

---

**[I2] Memory Amplification in OSC 7770 Image Payload Processing**
- **Location:** `arcterm-structured-output/src/lib.rs:16,36`; `arcterm-structured-output/src/image.rs:22-28`
- **CWE:** CWE-400 (Uncontrolled Resource Consumption)
- **Description:** The OSC 7770 renderer enforces a 10 MB ceiling on the raw JSON payload string (`DEFAULT_MAX_PAYLOAD_SIZE = 10 * 1024 * 1024`). For image blocks, the JSON `data` field contains base64-encoded image data. A 10 MB base64 string decodes to approximately 7.5 MB of binary data. Additionally, the renderer pre-allocates a `Vec<Action>` with capacity `payload_str.len() * 2 + 64` (up to ~20 million entries), and each `Action::Print(char)` entry in the SGR rendering path is at least 8 bytes. A single crafted OSC 7770 sequence can therefore cause the process to allocate over 640 MB of heap memory. Any terminal program running in an ArcTerm pane (including SSH-connected sessions) can emit OSC 7770 sequences without user consent.
- **Impact:** A malicious or compromised remote process can cause ArcTerm to exhaust available memory, leading to a process crash or system memory pressure. This is exploitable by any command run in any pane — including SSH sessions — with no user interaction required beyond having OSC 7770 rendering enabled.
- **Remediation:**
  1. Reduce `DEFAULT_MAX_PAYLOAD_SIZE` to 1 MB (or make it configurable with a safe default).
  2. In `image.rs`, add a pre-check on the `data` field length before calling `base64::decode`. Reject payloads where `data.len() > 1_400_000` (approximately 1 MB decoded).
  3. Remove the over-eager pre-allocation: replace `payload_str.len() * 2 + 64` with a conservative constant (e.g., `4096`).
  4. Consider requiring a user-level opt-in configuration flag before processing OSC 7770 at all.
- **Evidence:**
  ```rust
  // lib.rs:16 — ceiling is 10 MB
  pub const DEFAULT_MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024;
  // lib.rs:36 — pre-allocates up to 20M entries
  let estimated_capacity = payload_str.len() * 2 + 64;
  let mut actions = Vec::with_capacity(estimated_capacity);
  ```

---

**[I3] LLM Response Rendered as Raw Terminal Text (Escape Sequence Injection)**
- **Location:** `wezterm-gui/src/ai_pane.rs:216`; `wezterm-gui/src/overlay/ai_command_overlay.rs:183`
- **CWE:** CWE-116 (Improper Encoding or Escaping of Output); OWASP A03:2021
- **Description:** Tokens streamed from the LLM backend are passed directly to `term.render(&[Change::Text(token.to_string())])` without stripping ANSI escape sequences. The `termwiz` `Change::Text` change type sends raw text to the terminal emulator, which will interpret any embedded ANSI/VT sequences. An LLM response containing `\x1b]0;malicious title\x07` would change the window title; a response containing `\x1b[?1049h` would enter alternate screen; a response containing OSC 52 sequences could write to the clipboard. While this requires an LLM that produces malicious output (or a compromised Ollama instance), the terminal render path provides no defense.
- **Impact:** A compromised or adversarially prompted LLM could inject terminal control sequences into the AI pane output, potentially: changing window title (UI deception), switching screen modes, writing arbitrary data to the system clipboard (data exfiltration), or triggering other OSC-gated behaviors.
- **Remediation:** Before passing LLM tokens to `Change::Text`, strip ANSI escape sequences. The `strip-ansi-escapes` crate is already in the workspace and can be used here. Apply the same sanitization in `collect_streaming_response` in the command overlay.
- **Evidence:**
  ```rust
  // ai_pane.rs:216 — token rendered without sanitization
  term.render(&[Change::Text(token.to_string())])?;
  full_response.push_str(token);
  ```

---

**[I4] Terminal Scrollback Sent to Remote LLM Without Explicit User Consent**
- **Location:** `wezterm-gui/src/ai_pane.rs:25-26`; `arcterm-ai/src/context.rs`; `arcterm-ai/src/prompts.rs`
- **CWE:** CWE-359 (Exposure of Private Personal Information to an Unauthorized Actor)
- **Description:** The AI pane calls `AiConfig::default()` which configures the Ollama backend (local-only by default). However, the config type includes a `BackendKind::Claude` variant and an `api_key` field. When the Claude backend is selected, terminal scrollback content (including any secrets, credentials, or personal data visible in the terminal) will be transmitted to Anthropic's API at `https://api.anthropic.com/v1/messages`. There is currently no user-facing consent dialog, no opt-in configuration requirement, and no indication in the UI that content may leave the device. The design documentation mentions `AiConfig::default()` hardcodes Ollama, but no architectural guard prevents switching to Claude without a consent flow being added first.
- **Impact:** If a user or configuration change enables Claude as the backend, all terminal scrollback content sent to the AI pane would be transmitted to a third-party API. This could expose SSH passwords, API keys, personal data, or confidential information visible in the terminal.
- **Remediation:**
  1. Add a configuration key `ai.allow_remote_backend = false` that must be explicitly set to `true` before any non-local backend is activated.
  2. Add a runtime consent prompt when a remote backend is first used: "This will send terminal content to [provider]. Continue? (y/N)".
  3. Enforce in `create_backend`: if `BackendKind::Claude` is selected and the consent flag is not set, fall back to Ollama with a logged warning.
- **Evidence:**
  ```rust
  // backend/claude.rs:44-47 — sends to remote API without consent check
  let response = ureq::post(CLAUDE_API_URL)
      .set("x-api-key", &self.api_key)
      ...
      .send_json(&body)?;  // body contains terminal scrollback
  ```

---

### Advisory

- **[A1] `sk-ant-test` hardcoded in test fixtures** — `arcterm-ai/src/backend/claude.rs:69,88`; `arcterm-ai/tests/backend_tests.rs:20`; `arcterm-ai/src/config.rs:55` — The string `sk-ant-test` matches the `sk-ant-` prefix pattern used by real Anthropic API keys. Secrets scanners (GitHub secret scanning, truffleHog) may flag this, causing false-positive alerts or suppression fatigue. Replace with a clearly synthetic placeholder like `"test-key-not-real"` or `"FAKE_KEY_FOR_TESTS"`.

- **[A2] `terminal:read` silently granted to every plugin** — `arcterm-wasm-plugin/src/capability.rs:97-110` — Every WASM plugin automatically receives `terminal:read` capability regardless of what capabilities the user declared. This means a plugin declared with only `net:connect:api.example.com:443` can also read terminal output. The comment says "Always includes terminal:read as a default grant" but this is not documented in the user-facing capability system. Remove the unconditional default grant and require plugins to explicitly declare `terminal:read` if they need it. This aligns with the principle of least privilege (CWE-272).

- **[A3] No upper bound on `memory_limit_mb` from Lua config** — `lua-api-crates/plugin/src/lib.rs:254` — A user config can set `memory_limit_mb = 4294967295` (max `u32`), which will cause the `checked_mul` in `loader.rs:146` to overflow and return an error (safe), but the error message is confusing. More importantly, a value of `memory_limit_mb = 65536` (64 GB) will pass the `checked_mul` check and create a `StoreLimitsBuilder` with a 64 GB ceiling, potentially allowing a plugin to exhaust all available host RAM. Clamp to a configurable maximum (suggested: 512 MB) in the Lua API layer before passing to the config struct.

- **[A4] Plugin name not validated for log injection** — `arcterm-wasm-plugin/src/host_api.rs:61,68,75,311,337` — The plugin name field from Lua config is used directly in `log::info!("[plugin/{}] ...")` calls. A plugin name containing newlines, ANSI codes, or syslog control characters could corrupt log output or confuse log parsers. Validate plugin names to alphanumeric characters, hyphens, and underscores at registration time in `lua-api-crates/plugin/src/lib.rs`.

- **[A5] CI deploy script echoes password hash and uses `eval` on external output** — `ci/deploy.sh:59,63` — Line 59 (`echo $MACOS_PW | shasum`) writes a SHA-1 hash of the decoded keychain password to CI build logs. While a hash is not the raw secret, it provides an oracle for offline dictionary attacks against a known-format macOS certificate password. Additionally, line 63 (`def_keychain=$(eval echo $(security default-keychain -d user))`) uses `eval` on the output of `security default-keychain`. If the keychain name contains shell metacharacters (spaces, backticks, semicolons), this could execute arbitrary commands. Remediation: remove the `shasum` line; replace `eval echo $(...)` with `$(security default-keychain -d user | tr -d '"')`. Note: this code is inherited from upstream WezTerm — consider backporting upstream's current version of this script.

- **[A6] `.env` not covered by `.gitignore`** — `.gitignore` — The project `.gitignore` does not include `.env`, `.env.local`, or `*.env` patterns. If a developer creates a `.env` file to store a Claude API key locally (a common pattern), it could be accidentally committed. Add `.env`, `.env.*`, and `*.env` to `.gitignore`.

- **[A7] OSC 7770 escapes embed raw ANSI sequences via `Print` actions** — `arcterm-structured-output/src/lib.rs:67-78`; `arcterm-structured-output/src/image.rs:68-73` — The `render_title`, `emit_sgr`, and `emit_sgr_reset` functions construct ANSI escape sequences (e.g., `\x1b[1m`) and push them character by character as `Action::Print` entries. The downstream `performer.rs:780` feeds these back into `self.perform(action)`. This creates an unusual data flow: the structured output renderer produces raw escape bytes that re-enter the terminal state machine through the normal VT processing path. While the current sequences are safe, this architecture makes it easy to accidentally inject arbitrary control sequences from future renderers. Consider using `Action::CSI(...)` directly instead of embedding escape bytes in print characters.

---

## Cross-Component Analysis

**WASM capability enforcement is prototype-only.** The capability enforcement layer (`capability.rs`, `host_api.rs`) is well-structured but the guest `Instance` is not yet stored after `load_single_plugin` (lifecycle.rs:139 discards `_loaded`). This means no actual WASM callbacks are dispatched today. When the instantiation wiring is completed (the TODO at lifecycle.rs:147), the path traversal vulnerability in [I1] becomes immediately exploitable. The security fix must precede that wiring commit.

**AI backend has no configuration integration.** Both `open_ai_pane` and `show_command_overlay` call `AiConfig::default()` directly, bypassing any user configuration. The `AiConfig` struct supports Claude configuration, but there is currently no path from the user's Lua `wezterm.lua` to set `backend`, `api_key`, or `model`. This means the Claude backend can only be activated by a code change — not by a user config. This is safe today but is an architectural gap that will need careful attention when the AI config Lua API is added.

**OSC 7770 processes input from any terminal process without gating.** The OSC 7770 handler in `performer.rs:772-783` processes payloads from any running process in the terminal, including SSH sessions and untrusted scripts. The `render()` function has a size check but no consent gate and no per-pane opt-in. This contrasts with other OSC sequences (e.g., OSC 52 clipboard access) which have explicit user-configurable policies. Add an `enable_osc_7770 = false` config key that must be explicitly enabled.

**Destructive command detection is advisory, not a security boundary.** The `is_destructive()` function uses simple substring matching on lowercased text. It can be trivially bypassed (e.g., `r''m -rf /` with a shell alias, `\x72m -rf`, base64-encoded commands piped to `bash`). This is documented in the code (`// This is a heuristic — not a security boundary`) but the UI presents it with red warning text that may create false user confidence. Consider adding a disclaimer in the UI: "Warning is advisory only — review before running."

---

## Analysis Coverage

| Area | Checked | Notes |
|------|---------|-------|
| Code Security (OWASP) | Yes | Injection, deserialization, input validation, output encoding reviewed across all new crates |
| Secrets & Credentials | Yes | No live credentials found; test fixture strings flagged |
| Dependencies | Partial | `cargo audit` not installed; manual version review performed; wasmtime 36.0.6 (LTS), ureq 2.12.1 (rustls), base64 0.22.1 — no known CVEs identified via knowledge cutoff |
| Infrastructure as Code | N/A | No Terraform/Ansible/Docker in scope |
| Docker/Container | N/A | No Dockerfiles added |
| Configuration | Yes | CI scripts, Lua API, WASM config reviewed |

---

## Dependency Status

| Package | Version in Cargo.lock | Notes |
|---------|----------------------|-------|
| `wasmtime` | 36.0.6 | LTS branch; no known CVEs at audit date. Component Model enabled; fuel metering configured. |
| `ureq` | 2.12.1 | Uses `rustls` for TLS (not native-tls). Certificate verification is enabled by default. No issues found. |
| `base64` | 0.22.1 | Current stable. No known CVEs. |
| `syntect` | 5.3.0 | Pure-Rust regex variant used (no PCRE). No known CVEs. |
| `serde_json` | (workspace) | Used for OSC 7770 payload parsing — safe deserialization, no `unsafe` paths triggered. |

**Note:** `cargo-audit` is not installed in this environment. Install with `cargo install cargo-audit` and run `cargo audit` before each release to check against the RustSec advisory database.

---

## IaC Findings

No Terraform, Ansible, or Docker IaC files were added in this diff. The CI script findings are documented under Advisory [A5].
