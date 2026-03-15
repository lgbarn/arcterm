---
phase: structured-output
plan: "1.1"
wave: 1
dependencies: []
must_haves:
  - OSC 7770 match arm in osc_dispatch parsing start/end with type and key=value attrs
  - Handler trait methods structured_content_start and structured_content_end with default no-ops
  - StructuredContentAccumulator on GridState that buffers put_char between start and end
  - Existing VT processing unchanged for non-7770 sequences (zero interference)
files_touched:
  - arcterm-vt/src/processor.rs
  - arcterm-vt/src/handler.rs
  - arcterm-vt/src/lib.rs
tdd: true
---

# PLAN-1.1 -- OSC 7770 Protocol Parser

## Goal

Add OSC 7770 parsing to the VT processor so that `ESC ] 7770 ; start ; type=code_block ; lang=rust ST <content> ESC ] 7770 ; end ST` sequences are recognized, content between start and end is buffered, and the complete block is delivered to the Handler via new trait methods. This is the protocol foundation that every rich content renderer depends on.

## Why This Must Come First

Every structured content renderer (code blocks, diffs, JSON, markdown) receives its content via OSC 7770. Without the parser, nothing in Wave 2 or 3 can be tested with protocol-sourced data. The parser is also the lowest-risk change -- it extends existing `osc_dispatch` with a new match arm.

## Design Notes

The OSC 7770 protocol places content between two OSC sequences as plain PTY bytes (not inside an escape). The VT parser processes these bytes normally as `put_char` calls. The handler must detect when it is inside a structured block (between `start` and `end`) and buffer characters instead of (or in addition to) writing them to the grid.

The `StructuredContentAccumulator` is a field on `GridState` with states: `None` (normal), `Accumulating { content_type, attrs, buffer }`. When `put_char` is called during accumulation, the character is appended to the buffer. When `structured_content_end` fires, the accumulated content is delivered and the accumulator resets.

Content is ALSO written to the grid normally so that non-aware renderers see the raw text (fallback rendering). The structured block overlay will cover it during rendering.

## Tasks

<task id="1" files="arcterm-vt/src/handler.rs" tdd="true">
  <action>Add OSC 7770 handler methods and StructuredContentAccumulator to the Handler trait and GridState:

1. Define `ContentType` enum in `handler.rs`: `CodeBlock`, `Diff`, `Plan`, `Markdown`, `Json`, `Error`, `Progress`, `Image`. Derive `Debug, Clone, PartialEq, Eq`.

2. Add to the `Handler` trait two new methods with default no-op implementations:
   - `fn structured_content_start(&mut self, _content_type: ContentType, _attrs: Vec<(String, String)>) {}`
   - `fn structured_content_end(&mut self) {}`

3. Define `StructuredContentAccumulator` struct:
   ```
   pub struct StructuredContentAccumulator {
       pub content_type: ContentType,
       pub attrs: Vec<(String, String)>,
       pub buffer: String,
   }
   ```

4. Add `pub accumulator: Option<StructuredContentAccumulator>` field to `GridState`.

5. Implement `structured_content_start` on GridState: set `self.accumulator = Some(StructuredContentAccumulator { content_type, attrs, buffer: String::new() })`.

6. Implement `structured_content_end` on GridState: if `self.accumulator.is_some()`, take it and store the completed block in a new `pub completed_blocks: Vec<StructuredContentAccumulator>` field on GridState. If accumulator is None, no-op.

7. Modify `put_char` on GridState: if `self.accumulator.is_some()`, append the character to the accumulator's buffer IN ADDITION TO writing to the grid (call `self.grid.put_char_at_cursor(c)` as before). This ensures fallback rendering works.

Write tests first:
- `structured_content_start` sets accumulator with correct type and attrs
- `put_char` during accumulation appends to buffer AND writes to grid
- `structured_content_end` moves accumulator to completed_blocks and resets accumulator to None
- `structured_content_end` without prior start is a no-op (no panic)
- Multiple start/end cycles accumulate multiple completed blocks
- put_char without accumulation works exactly as before (regression test)</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-vt -- structured_content --nocapture</verify>
  <done>All structured content accumulation tests pass. `put_char` writes to both grid and accumulator buffer during accumulation. `completed_blocks` contains all finished blocks.</done>
</task>

<task id="2" files="arcterm-vt/src/processor.rs" tdd="true">
  <action>Add OSC 7770 parsing to `osc_dispatch` in `Performer`:

1. In the `match params[0]` block inside `osc_dispatch`, add a `b"7770"` arm.

2. Parse `params[1]` as either `b"start"` or `b"end"`.

3. For `b"start"`: parse `params[2..]` as key=value pairs. The first pair must be `type=<content_type>`. Map the type string to `ContentType` enum (e.g., `"code_block"` -> `ContentType::CodeBlock`). Remaining pairs are stored as `Vec<(String, String)>`. Call `self.handler.structured_content_start(content_type, attrs)`.

4. For `b"end"`: call `self.handler.structured_content_end()`.

5. For any other value of `params[1]`, or if `params.len() < 2`, ignore silently.

6. Key=value parsing: split each `params[i]` byte slice on `b'='`. The left side is the key, the right is the value. If no `=` found, skip the param.

Write tests first using the processor test pattern (feed raw bytes through `Processor::advance` with a `GridState`):
- Feed `ESC ] 7770 ; start ; type=code_block ; lang=rust ST` followed by `fn main() {}` followed by `ESC ] 7770 ; end ST`. Assert completed_blocks has one entry with type=CodeBlock, attrs contains ("lang", "rust"), buffer contains "fn main() {}".
- Feed `ESC ] 7770 ; start ; type=json ST` then `{"key": "val"}` then `ESC ] 7770 ; end ST`. Assert type=Json.
- Feed `ESC ] 7770 ; end ST` without prior start. Assert no panic, no completed blocks.
- Feed regular text before and after an OSC 7770 block. Assert grid contains all text (both structured and non-structured). Assert non-structured text does NOT appear in any accumulator buffer.
- Feed `ESC ] 7770 ; start ; type=markdown ST` then multi-line text with CR/LF. Assert buffer captures all characters.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-vt -- osc_7770 --nocapture</verify>
  <done>All OSC 7770 processor tests pass. The parser correctly handles start/end sequences, key=value attrs, content accumulation, and edge cases (no start before end, multi-line content).</done>
</task>

<task id="3" files="arcterm-vt/src/lib.rs" tdd="false">
  <action>Export new public types from `arcterm-vt/src/lib.rs`:

1. Add `pub use handler::{ContentType, StructuredContentAccumulator};` to the existing pub use block in `lib.rs`.

2. Run the full arcterm-vt test suite to verify zero regressions in existing Phase 1/2/3 processor and handler tests.

3. Run clippy on arcterm-vt to verify no new warnings.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-vt -- --nocapture && cargo clippy -p arcterm-vt -- -D warnings</verify>
  <done>All existing tests pass (zero regressions). Clippy clean. `ContentType` and `StructuredContentAccumulator` are publicly exported from `arcterm_vt`.</done>
</task>
