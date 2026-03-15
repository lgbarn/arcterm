# SUMMARY-1.2.md — Phase 4, Plan 1.2
## APC Scanner, Kitty Handler, and Crate Dependencies

**Executed:** 2026-03-15
**Branch:** master
**Commits:**
- `ed3624d` — shipyard(phase-4): add ApcScanner + kitty_graphics_command to Handler trait
- `75aab34` — shipyard(phase-4): add crate dependencies for phase 4 features
- `262b599` — shipyard(phase-4): export ApcScanner from crate root, full suite green

---

## Task 1: ApcScanner + kitty_graphics_command (TDD)

### Implementation

**Handler trait addition** (`arcterm-vt/src/handler.rs`):
- Added `fn kitty_graphics_command(&mut self, _payload: &[u8]) {}` as a default no-op method in the `Handler` trait under a new `// Kitty graphics protocol (APC, Phase 4)` section.
- No existing implementors required changes — default no-op ensures backward compatibility with `GridState`, `Grid`, and any downstream types.

**ApcScanner** (`arcterm-vt/src/processor.rs`):
- Implemented `ApcState` enum with four states: `Normal`, `PendingEsc`, `InApc`, `InApcPendingEsc`.
- `ApcScanner` struct wraps an inner `Processor` and holds a `payload: Vec<u8>` buffer.
- `advance<H: Handler>` method uses the state machine to intercept `ESC _` … `ESC \` (ST) sequences at the byte level, dispatching the stripped payload via `handler.kitty_graphics_command()`.
- Non-APC bytes are batch-forwarded to the inner `Processor` using slice windows (a single `inner.advance` call per contiguous non-ESC run) for performance.
- `PendingEsc` state: if the byte following ESC is not `_`, both ESC and the byte are forwarded as a two-byte slice to the inner Processor, preserving VT escape routing.

### TDD Test Results

Eight tests written before implementation, all confirmed failing first, all passing after:

| Test | Result |
|------|--------|
| `complete_apc_sequence_dispatches_payload` | PASS |
| `split_at_esc_boundary_reconstructs_apc` | PASS |
| `split_at_st_boundary_reconstructs_apc` | PASS |
| `non_apc_input_forwarded_as_plain_chars` | PASS |
| `esc_non_underscore_forwarded_not_apc` | PASS |
| `empty_apc_payload_dispatches_empty_slice` | PASS |
| `large_payload_dispatched_completely` | PASS |
| `multiple_apc_sequences_in_one_buffer` | PASS |

### Deviation Noted

Plan 1.1 was actively writing to `handler.rs` and `processor.rs` concurrently. Specifically:
- Plan 1.1's linter added `ContentType`, `StructuredContentAccumulator`, `accumulator`, and `completed_blocks` fields to `GridState`, and `structured_content_start`/`structured_content_end` to the `Handler` trait — all in non-conflicting code regions.
- Plan 1.1's linter added `dispatch_osc7770` and `use` imports to `processor.rs` in the OSC dispatch region.
- These additions did not conflict with the APC scanner (different code regions). The `kitty_graphics_command` method was added after `structured_content_end` without collision.

---

## Task 2: Add Crate Dependencies

### Workspace Cargo.toml additions

```toml
syntect = "5"
pulldown-cmark = "0.12"
image = "0.25"
regex = "1"
base64 = "0.22"
serde_json = "1"
```

### Per-crate wiring

| Crate | Dependencies added |
|-------|--------------------|
| `arcterm-render` | `syntect`, `pulldown-cmark`, `image` |
| `arcterm-app` | `regex`, `serde_json` |
| `arcterm-vt` | `base64` |

### Verification

`cargo check --workspace` passed clean. All six new crates resolved and compiled (syntect pulled onig; image pulled ravif and cosmic-text). No `cargo check` errors.

---

## Task 3: Export ApcScanner from lib.rs

### Verification

`pub use processor::{ApcScanner, Processor};` was present from Task 1. Plan 1.1's linter additionally exported `ContentType`, `GridState`, `Handler`, and `StructuredContentAccumulator` from the crate root.

**Full test suite:** 110 tests, 0 failures, 0 ignored.
**Clippy:** `cargo clippy -p arcterm-vt` — zero warnings, zero errors.

---

## Final State

| File | Change |
|------|--------|
| `arcterm-vt/src/handler.rs` | Added `kitty_graphics_command` default method to `Handler` trait |
| `arcterm-vt/src/processor.rs` | Added `ApcState` enum and `ApcScanner` struct (140 lines) |
| `arcterm-vt/src/lib.rs` | Re-exported `ApcScanner`; added 8 TDD tests in `apc_scanner_tests` module |
| `Cargo.toml` | 6 new workspace dependencies |
| `arcterm-vt/Cargo.toml` | `base64` dependency |
| `arcterm-render/Cargo.toml` | `syntect`, `pulldown-cmark`, `image` dependencies |
| `arcterm-app/Cargo.toml` | `regex`, `serde_json` dependencies |
| `Cargo.lock` | Updated for all new transitive dependencies |

No architectural deviations. No blockers encountered. Plan 1.1 parallel changes integrated cleanly.
