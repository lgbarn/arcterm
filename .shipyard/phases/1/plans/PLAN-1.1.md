---
phase: ai-feature-hardening
plan: "1.1"
wave: 1
dependencies: []
must_haves:
  - LLM output tokens cannot inject terminal escape sequences into rendered surfaces
  - sanitize_llm_output() lives in arcterm-ai and is unit-tested
  - Both ai_pane.rs and ai_command_overlay.rs call the sanitizer before rendering
files_touched:
  - arcterm-ai/src/sanitize.rs
  - arcterm-ai/src/lib.rs
  - arcterm-ai/Cargo.toml
  - wezterm-gui/src/ai_pane.rs
  - wezterm-gui/src/overlay/ai_command_overlay.rs
tdd: true
---

# PLAN-1.1 — Escape Injection Fix

## Context

Two surfaces render LLM tokens directly into `TermWizTerminal` via
`Change::Text(...)` without stripping terminal escape sequences first:

- `wezterm-gui/src/ai_pane.rs:221` — `term.render(&[Change::Text(display_token)])?`
- `wezterm-gui/src/overlay/ai_command_overlay.rs:183` — `result.push_str(token)` (fed into `Change::Text` at line 129)

`termwiz` is a workspace dependency but is **not yet in `arcterm-ai/Cargo.toml`**.
The plan adds it there so the sanitizer can reuse the termwiz escape parser
inline rather than writing a bespoke state machine.

---

<task id="1" files="arcterm-ai/Cargo.toml, arcterm-ai/src/sanitize.rs, arcterm-ai/src/lib.rs" tdd="true">
  <action>
    1. Add `termwiz = { workspace = true }` to `arcterm-ai/Cargo.toml` under
       `[dependencies]`.

    2. Create `arcterm-ai/src/sanitize.rs` with a public function:

       ```rust
       /// Strip all ANSI/VT escape sequences from `raw` and return plain text.
       ///
       /// Uses the termwiz `Parser` to consume the input byte-by-byte, keeping
       /// only `Action::Print` characters. Everything else (CSI, OSC, DCS,
       /// private sequences) is silently discarded.
       pub fn sanitize_llm_output(raw: &str) -> String { ... }
       ```

       The implementation should construct a `termwiz::escape::parser::Parser`,
       feed it `raw.as_bytes()` via `parser.parse(bytes, |action| { ... })`,
       and accumulate only `Action::Print(ch)` characters into the output
       `String`.

    3. Write inline tests in `sanitize.rs` under `#[cfg(test)]` covering:
       - Plain text passes through unchanged.
       - CSI SGR sequences (e.g. `\x1b[31m`, `\x1b[0m`) are stripped.
       - OSC sequences (e.g. `\x1b]0;title\x07`) are stripped.
       - Mixed plain text + escapes returns only the plain text.
       - Empty string returns empty string.

    4. Re-export the function from `arcterm-ai/src/lib.rs`:
       `pub mod sanitize; pub use sanitize::sanitize_llm_output;`
  </action>
  <verify>cargo test --package arcterm-ai sanitize</verify>
  <done>All five inline tests pass. `cargo check --package arcterm-ai` reports no errors.</done>
</task>

<task id="2" files="wezterm-gui/src/ai_pane.rs, wezterm-gui/src/overlay/ai_command_overlay.rs" tdd="false">
  <action>
    Apply `sanitize_llm_output` at every point where LLM tokens reach a
    `Change::Text` render call.

    **ai_pane.rs (line 218-221):**
    Add `use arcterm_ai::sanitize_llm_output;` to the import block (already
    imports `arcterm_ai::backend` etc.).
    Change the token rendering block from:
    ```rust
    let display_token = token.replace('\n', "\r\n");
    term.render(&[Change::Text(display_token)])?;
    ```
    to:
    ```rust
    let safe_token = sanitize_llm_output(token);
    let display_token = safe_token.replace('\n', "\r\n");
    term.render(&[Change::Text(display_token)])?;
    ```

    **ai_command_overlay.rs (line 183 inside `collect_streaming_response`):**
    Add `use arcterm_ai::sanitize_llm_output;` at the top of the file.
    Change:
    ```rust
    result.push_str(token);
    ```
    to:
    ```rust
    result.push_str(&sanitize_llm_output(token));
    ```

    No changes are needed to the `Change::Text(format!(...))` call at line 129
    because that string is built from `display` (the already-safe
    `maybe_warn(&command)` output, not raw LLM tokens).
  </action>
  <verify>cargo check --package wezterm-gui</verify>
  <done>`cargo check --package wezterm-gui` exits 0 with no errors or warnings about the changed lines.</done>
</task>
