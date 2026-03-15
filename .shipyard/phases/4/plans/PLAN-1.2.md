---
phase: structured-output
plan: "1.2"
wave: 1
dependencies: []
must_haves:
  - ApcScanner pre-processor that intercepts ESC _ ... ESC \ before vte sees them
  - Stateful across partial PTY reads (handles ESC _ split across advance calls)
  - Handler trait method kitty_graphics_command with default no-op
  - StructuredBlock data model and ContentType shared types for app layer
  - Crate dependency additions to workspace Cargo.toml
files_touched:
  - arcterm-vt/src/processor.rs
  - arcterm-vt/src/handler.rs
  - Cargo.toml (workspace)
  - arcterm-vt/Cargo.toml
  - arcterm-render/Cargo.toml
  - arcterm-app/Cargo.toml
tdd: true
---

# PLAN-1.2 -- APC Scanner, Kitty Handler, and Crate Dependencies

## Goal

Implement the APC byte-stream pre-processor that intercepts Kitty graphics protocol sequences before vte silently drops them, add the `kitty_graphics_command` handler method, and add all Phase 4 crate dependencies to the workspace. This plan runs in parallel with PLAN-1.1 (no file overlap).

## Why This Must Come First

vte 0.15 silently drops all APC sequences -- there is no callback. The Kitty graphics protocol uses APC (`ESC _ G ... ESC \`). Without this pre-processor, image data is permanently lost. The APC scanner must be in place before any Kitty graphics work in Wave 3. The crate dependencies must be declared before any Wave 2 plan can use syntect, pulldown-cmark, or image.

## Design Notes

The `ApcScanner` wraps the existing `Processor`. It maintains a two-state machine: `Normal` and `InApc { buf: Vec<u8> }`. On each byte:
- In `Normal`: if byte is `0x1B` (ESC), peek at next byte. If next is `0x5F` (`_`), enter `InApc`. Otherwise forward to vte.
- In `InApc`: accumulate bytes until `ESC \` (ST) is detected. Then call `handler.kitty_graphics_command(&buf)` and return to `Normal`.

The tricky part is partial reads: `ESC` might be the last byte of one `advance()` call and `_` the first byte of the next. The scanner must hold a `pending_esc: bool` flag across calls.

## Tasks

<task id="1" files="arcterm-vt/src/processor.rs, arcterm-vt/src/handler.rs" tdd="true">
  <action>Implement `ApcScanner` and `kitty_graphics_command` handler method:

1. Add to `Handler` trait: `fn kitty_graphics_command(&mut self, _payload: &[u8]) {}` with default no-op.

2. Create `pub struct ApcScanner` in `processor.rs`:
   ```
   pub struct ApcScanner {
       processor: Processor,
       state: ApcState,
   }

   enum ApcState {
       Normal,
       PendingEsc,
       InApc { buf: Vec<u8> },
       InApcPendingEsc { buf: Vec<u8> },
   }
   ```

3. Implement `ApcScanner::new() -> Self` (wraps `Processor::new()`).

4. Implement `ApcScanner::advance<H: Handler>(&mut self, handler: &mut H, bytes: &[u8])`:
   - Iterate bytes one at a time.
   - `Normal` + `0x1B` -> `PendingEsc`.
   - `Normal` + any other byte -> forward to internal `Processor::advance(handler, &[byte])`.
   - `PendingEsc` + `0x5F` (`_`) -> `InApc { buf: Vec::new() }`.
   - `PendingEsc` + any other byte -> forward both `0x1B` and this byte to Processor, go to `Normal`.
   - `InApc` + `0x1B` -> `InApcPendingEsc { buf }`.
   - `InApc` + any other byte -> `buf.push(byte)`.
   - `InApcPendingEsc` + `0x5C` (`\`) -> call `handler.kitty_graphics_command(&buf)`, go to `Normal`.
   - `InApcPendingEsc` + any other byte -> push `0x1B` and byte to buf, go back to `InApc`.

5. Performance note: batch consecutive non-ESC bytes in Normal state into a single `Processor::advance` call rather than byte-by-byte. Track the start of normal runs and flush before state transitions.

Write tests first:
- Complete APC sequence in single advance call: `ESC _ G a=T,f=100; <base64> ESC \` delivers correct payload to a mock handler
- APC sequence split at ESC boundary: first call ends with `ESC`, second call starts with `_ G ...`
- APC sequence split at ST boundary: first call ends with `ESC` inside APC, second starts with `\`
- Non-APC bytes forwarded correctly: `"Hello"` followed by APC sequence followed by `"World"` -- verify all text reaches the grid AND the APC payload is delivered
- ESC followed by non-`_` byte (e.g., `ESC [`) is forwarded to vte correctly (CSI must still work)
- Empty APC sequence (`ESC _ ESC \`) delivers empty payload
- Very large APC payload (64KB) delivered without truncation</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-vt -- apc_scanner --nocapture</verify>
  <done>All APC scanner tests pass including boundary splits. Non-APC bytes (CSI, plain text, OSC) pass through correctly. Kitty graphics payloads are delivered intact.</done>
</task>

<task id="2" files="Cargo.toml, arcterm-vt/Cargo.toml, arcterm-render/Cargo.toml, arcterm-app/Cargo.toml" tdd="false">
  <action>Add all Phase 4 crate dependencies to the workspace and member Cargo.toml files:

1. In workspace `Cargo.toml` `[workspace.dependencies]`, add:
   - `syntect = { version = "5", default-features = false, features = ["default-fancy"] }`
   - `pulldown-cmark = "0.13"`
   - `image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }`
   - `regex = "1"`
   - `base64 = "0.22"`
   - `serde_json = "1"`

2. In `arcterm-render/Cargo.toml` `[dependencies]`, add:
   - `syntect.workspace = true`
   - `pulldown-cmark.workspace = true`
   - `image.workspace = true`

3. In `arcterm-app/Cargo.toml` `[dependencies]`, add:
   - `regex.workspace = true`
   - `serde_json.workspace = true`

4. In `arcterm-vt/Cargo.toml` `[dependencies]`, add:
   - `base64.workspace = true`
   (needed for Kitty graphics base64 payload decoding in the APC handler)

5. Run `cargo check --workspace` to verify all dependencies resolve and the workspace compiles.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo check --workspace 2>&1 | tail -5</verify>
  <done>`cargo check --workspace` succeeds with no errors. All new dependencies resolve. No existing code is broken by the additions.</done>
</task>

<task id="3" files="arcterm-vt/src/lib.rs" tdd="false">
  <action>Export `ApcScanner` from arcterm-vt and verify full test suite:

1. Add `pub use processor::ApcScanner;` to `arcterm-vt/src/lib.rs`.

2. Run the full workspace test suite to confirm zero regressions across all crates.

3. Run clippy on the entire workspace.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --workspace -- --nocapture 2>&1 | tail -10 && cargo clippy --workspace -- -D warnings 2>&1 | tail -5</verify>
  <done>Full workspace tests pass. Clippy clean. `ApcScanner` is publicly exported from `arcterm_vt`.</done>
</task>
