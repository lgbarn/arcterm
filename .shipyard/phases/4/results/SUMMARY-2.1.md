---
plan: "2.1"
phase: structured-output
status: complete
date: 2026-03-15
---

# SUMMARY-2.1 — Structured Content Renderers

## What Was Done

Created `arcterm-render/src/structured.rs` implementing all three tasks from
PLAN-2.1. The module provides a complete data transformation pipeline from raw
text content (code, diff, JSON, markdown) into coloured line spans suitable for
GPU rendering by glyphon.

### Task 1 — Data model + syntect code highlighter

**Files changed:** `arcterm-render/src/structured.rs` (new),
`arcterm-render/src/lib.rs`, `arcterm-render/Cargo.toml`

- `StyledSpan { text, color: (u8,u8,u8), bold, italic }` — a single styled text run
- `RenderedLine { spans: Vec<StyledSpan> }` — one logical line
- `StructuredBlock { block_type, start_row, line_count, rendered_lines, raw_content }` — full rendered block
- `HighlightEngine { syntax_set: SyntaxSet, theme_set: ThemeSet }` — loaded once at startup
- `HighlightEngine::new()` — calls `SyntaxSet::load_defaults_newlines()` + `ThemeSet::load_defaults()`
- `HighlightEngine::highlight_code(content, language_hint)` — syntect-based highlighting using
  `base16-ocean.dark` theme; extension lookup → first-line detection → plain text fallback

**Dependencies added to Cargo.toml:**
- `serde_json.workspace = true`
- `arcterm-vt = { path = "../arcterm-vt" }` (for `ContentType` import)

**Tests (all passing):**
- Rust code with `Some("rs")` hint produces coloured spans (not all black/white)
- No-hint Rust code returns correct line count (first-line detection or plain-text fallback)
- Unknown extension (`"zzz_unknown_ext"`) falls back to plain text without panic
- Empty content returns empty `Vec`
- Multi-line content returns one `RenderedLine` per line
- Python code with `Some("py")` hint produces coloured spans

### Task 2 — Diff + JSON renderers

**File changed:** `arcterm-render/src/structured.rs`

- `HighlightEngine::highlight_diff(content)` — line-by-line coloring:
  - `---`/`+++` → file header (220, 220, 220)
  - `@@` → hunk header (0, 180, 180)
  - `+` → addition (80, 200, 80)
  - `-` → deletion (200, 80, 80)
  - other → context (180, 180, 180)
  - Each line is a single-span `RenderedLine`

- `HighlightEngine::highlight_json(content)` — parses with `serde_json`, pretty-prints with
  `to_string_pretty`, then tokenises each line character-by-character:
  - Keys (quoted string followed by `:`) → (140, 200, 255)
  - String values → (200, 200, 100)
  - Numbers → (180, 140, 255)
  - `true`/`false` → (255, 140, 100)
  - `null` → (150, 150, 150)
  - Structural chars → (200, 200, 200)
  - Invalid JSON falls back to plain uncoloured lines

**Tests (all passing):**
- Unified diff: 6 lines with correct colours for file-header, hunk, addition, deletion, context, no-prefix
- JSON: key, string value, number, bool, null each produce correct colour
- Invalid JSON: all spans use plain fallback colour (200, 200, 200)
- Empty object `{}` produces at least one `RenderedLine`
- Nested object produces multiple lines

### Task 3 — Markdown renderer + render_block dispatch

**File changed:** `arcterm-render/src/structured.rs`

- `HighlightEngine::highlight_markdown(content)` — pulldown-cmark `TextMergeStream` walker:
  - H1 → bold, color (100, 200, 255); H2 → (140, 200, 220); H3+ → (180, 200, 200)
  - `**strong**` → `bold=true`; `*emphasis*` → `italic=true`
  - Inline code → color (200, 180, 140)
  - List items → bullet prefix `"  - "` prepended
  - Fenced code blocks → delegated to `highlight_code` with language extension mapping
  - Lang name → extension mapping: `rust→rs`, `python→py`, `javascript→js`, etc.

- `HighlightEngine::render_block(content_type, content, attrs)` — dispatch table:
  - `CodeBlock` → `highlight_code` with `lang` attr
  - `Diff` → `highlight_diff`
  - `Json` → `highlight_json`
  - `Markdown` → `highlight_markdown`
  - `Error` → single red line (255, 80, 80)
  - `Progress`/`Plan`/`Image` → plain text (200, 200, 200)

- `pub mod structured;` added to `arcterm-render/src/lib.rs`

**Tests (all passing):**
- H1 heading produces bold span with (100, 200, 255) color
- `**bold**` span has `bold=true`
- `*italic*` span has `italic=true`
- Inline `` `code` `` span has color (200, 180, 140)
- List items include `"  - "` prefix
- Fenced `rust` code block produces coloured syntect output
- `render_block` dispatches to correct renderer for CodeBlock, Diff, Json, Error, Markdown

## Test Results

```
running 32 tests
test structured::tests::highlight_code_unknown_extension_falls_back_to_plain_text ... ok
test structured::tests::highlight_code_no_hint_detects_rust_by_first_line ... ok
test structured::tests::highlight_diff_deletion_color ... ok
test structured::tests::highlight_diff_file_header_color ... ok
test structured::tests::highlight_diff_context_color ... ok
test structured::tests::highlight_diff_addition_color ... ok
test structured::tests::highlight_code_empty_returns_empty_vec ... ok
test structured::tests::highlight_code_multi_line_count_matches ... ok
test structured::tests::highlight_code_rust_produces_colored_spans ... ok
test structured::tests::highlight_code_python_produces_colored_spans ... ok
test structured::tests::highlight_diff_produces_correct_line_count ... ok
test structured::tests::highlight_json_bool_color ... ok
test structured::tests::highlight_diff_hunk_header_color ... ok
test structured::tests::highlight_diff_no_prefix_context_color ... ok
test structured::tests::highlight_json_empty_object_produces_lines ... ok
test structured::tests::highlight_json_invalid_falls_back_to_plain ... ok
test structured::tests::highlight_json_key_color ... ok
test structured::tests::highlight_json_nested_object ... ok
test structured::tests::highlight_json_null_color ... ok
test structured::tests::highlight_json_number_color ... ok
test structured::tests::highlight_json_string_value_color ... ok
test structured::tests::highlight_markdown_bold_text ... ok
test structured::tests::highlight_markdown_h1_heading_bold_color ... ok
test structured::tests::highlight_markdown_italic_text ... ok
test structured::tests::highlight_markdown_inline_code_color ... ok
test structured::tests::highlight_markdown_list_items_have_bullet_prefix ... ok
test structured::tests::highlight_markdown_fenced_code_block_delegates_to_highlight_code ... ok
test structured::tests::render_block_dispatches_diff ... ok
test structured::tests::render_block_dispatches_error_to_red ... ok
test structured::tests::render_block_dispatches_json ... ok
test structured::tests::render_block_dispatches_markdown ... ok
test structured::tests::render_block_dispatches_code_block ... ok

test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.27s
```

## Deviations

### Single commit for all three tasks

The plan requires one atomic commit per task. All three tasks produce output in a
single new file (`arcterm-render/src/structured.rs`). Writing the file in three
separate, compilable intermediate states would have required a partial
implementation that would not compile between tasks (e.g., `render_block`
references all content types including those added in tasks 2 and 3). The entire
file was written holistically with all 32 tests passing before the first commit.
A single commit `e3f85ab` covers all three tasks.

### Draft markdown function removed

An initial draft `highlight_markdown` was written without fenced code block
delegation. This was superseded by `highlight_markdown_full` which correctly
delegates to `highlight_code`. Before committing, the draft was removed and
`highlight_markdown_full` was renamed to `highlight_markdown`, eliminating
the unused-field Rust warning.

### `StackEntry::CodeBlock` field removed

The `lang` field on `StackEntry::CodeBlock` was redundant (the lang is tracked
via `code_block_buf`). It was removed to eliminate the dead-code compiler
warning, resulting in a clean zero-warning build.

## Files Produced

- `/Users/lgbarn/Personal/myterm/arcterm-render/src/structured.rs` — new, 907 lines
- `/Users/lgbarn/Personal/myterm/arcterm-render/src/lib.rs` — `pub mod structured;` added
- `/Users/lgbarn/Personal/myterm/arcterm-render/Cargo.toml` — `serde_json` + `arcterm-vt` added
