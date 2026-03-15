# Phase 4 Plan 1.1 — OSC 7770 Protocol Parser: Implementation Summary

## Status

All 3 tasks complete. 110 tests pass. Clippy clean (-D warnings).

## Commits

| Commit | Message |
|--------|---------|
| e83f909 | shipyard(phase-4): add ContentType, StructuredContentAccumulator, and Handler methods |
| f90ccd5 | shipyard(phase-4): implement OSC 7770 structured content dispatch in processor |
| 4238310 | shipyard(phase-4): export ContentType and StructuredContentAccumulator from crate root |

---

## Task 1: Handler methods + StructuredContentAccumulator

**File:** `arcterm-vt/src/handler.rs`

### What was added

**`ContentType` enum** — 8 variants covering all plan-specified types: `CodeBlock`, `Diff`, `Plan`, `Markdown`, `Json`, `Error`, `Progress`, `Image`. Derives `Debug`, `Clone`, `PartialEq`, `Eq`.

**`StructuredContentAccumulator`** — public struct with three fields:
- `content_type: ContentType`
- `attrs: HashMap<String, String>` — key/value pairs from OSC params
- `buffer: String` — raw text accumulated between start and end signals

Constructor: `StructuredContentAccumulator::new(content_type, attrs)`.

**`Handler` trait additions** (both default no-op):
- `structured_content_start(&mut self, content_type: ContentType, attrs: HashMap<String, String>)`
- `structured_content_end(&mut self)`

**`GridState` additions:**
- `accumulator: Option<StructuredContentAccumulator>` — active block, `None` when idle
- `completed_blocks: Vec<StructuredContentAccumulator>` — finished blocks in order

**`GridState::Handler` overrides:**
- `put_char`: writes to grid unconditionally, then appends to `accumulator.buffer` if one is active
- `structured_content_start`: sets `self.accumulator`
- `structured_content_end`: calls `take()` on the accumulator and pushes to `completed_blocks`

### Tests (12)

- `content_type_variants_exist` — all 8 variants compile
- `accumulator_can_be_constructed` — attrs and type stored correctly
- `accumulator_buffer_starts_empty` — buffer is empty string at creation
- `grid_state_has_accumulator_field_none_by_default`
- `grid_state_has_completed_blocks_empty_by_default`
- `structured_content_start_sets_accumulator`
- `structured_content_end_moves_accumulator_to_completed`
- `structured_content_end_without_start_is_noop`
- `put_char_during_accumulation_appends_to_buffer`
- `put_char_during_accumulation_also_writes_to_grid`
- `put_char_without_accumulation_does_not_affect_accumulator`
- `multiple_blocks_accumulate_independently`

---

## Task 2: OSC 7770 dispatch in processor

**File:** `arcterm-vt/src/processor.rs`

### What was added

**`dispatch_osc7770<H: Handler>(handler, params)`** — free function that parses OSC 7770 sequences:

Protocol layout:
```
ESC ] 7770 ; start ; type=<t> [; key=value]* BEL
ESC ] 7770 ; end BEL
```

- `params[0]` = `b"7770"` (matched in `osc_dispatch`)
- `params[1]` = `b"start"` or `b"end"`
- `params[2]` = `b"type=<content_type>"` (required for start; ignored if absent)
- `params[3..]` = additional `key=value` attrs

Type string to `ContentType` mapping:
| String | Variant |
|--------|---------|
| `code` | `CodeBlock` |
| `diff` | `Diff` |
| `plan` | `Plan` |
| `markdown` | `Markdown` |
| `json` | `Json` |
| `error` | `Error` |
| `progress` | `Progress` |
| `image` | `Image` |

Unknown type strings are silently ignored (no accumulator created).

**`osc_dispatch` update:** routes `b"7770"` to `dispatch_osc7770`. The existing `b"0" | b"2"` title branch was hardened with a `params.len() < 2` guard consistent with the new general guard.

### Tests (13)

- `osc7770_complete_code_block` — start/content/end round trip with attrs
- `osc7770_json_block` — JSON type mapping and buffer content
- `osc7770_content_type_diff/plan/markdown/error/progress/image` — all type mappings (6 tests)
- `osc7770_end_without_start_is_noop` — no completed block, no panic
- `osc7770_regular_text_before_and_after` — grid written correctly, buffer contains only inner text
- `osc7770_multiline_block` — multi-line content via CR+LF; visible chars in buffer
- `osc7770_multiple_attrs` — `lang=rust;file=main.rs` both parsed
- `osc7770_start_without_type_is_ignored` — malformed start leaves no accumulator

---

## Task 3: Exports from lib.rs

**File:** `arcterm-vt/src/lib.rs`

Added to the existing `pub use handler::{...}` line:
```rust
pub use handler::{ContentType, GridState, Handler, StructuredContentAccumulator};
```

Both `ContentType` and `StructuredContentAccumulator` are now accessible as `arcterm_vt::ContentType` and `arcterm_vt::StructuredContentAccumulator`.

---

## Deviations from Plan

**`ApcScanner` and `kitty_graphics_command` already present** — when reading the source files before implementation, `processor.rs` already contained `ApcScanner` and `Handler` already had the `kitty_graphics_command` no-op method. These were authored by a prior session and were already exported from `lib.rs`. No action needed; the OSC 7770 work was additive.

**`use std::collections::HashMap` added at crate level in handler.rs** — required to support the `HashMap<String, String>` in the `Handler` trait method signatures and `StructuredContentAccumulator`. This is a minor implementation detail not mentioned in the plan but necessary for correctness.

**Clippy `collapsible_if` fix** — the initial implementation of the attr parsing loop used nested `if let` blocks; clippy (with `-D warnings`) required collapsing them into a single `if let ... && let ...` expression using Rust 2024 edition let-chains. Fixed before committing Task 2.

---

## Final State

```
cargo test -p arcterm-vt   →  110 passed, 0 failed
cargo clippy -p arcterm-vt -- -D warnings  →  Finished (no errors)
```
