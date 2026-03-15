---
phase: structured-output
plan: "2.1"
wave: 2
dependencies: ["1.1", "1.2"]
must_haves:
  - StructuredBlock data model with rendered line spans (text + color per span)
  - syntect-based code block highlighter producing colored spans
  - Diff renderer with file headers and +/- coloring
  - JSON pretty-printer with key/value/string/number color coding
  - SyntaxSet and ThemeSet loaded once at startup, shared via reference
files_touched:
  - arcterm-render/src/structured.rs (new)
  - arcterm-render/src/lib.rs
tdd: true
---

# PLAN-2.1 -- Structured Content Renderers (Code, Diff, JSON)

## Goal

Implement the three primary content renderers that transform raw text content from OSC 7770 blocks (or auto-detection) into colored line spans suitable for rendering by glyphon. Each renderer takes plain text content and produces `Vec<RenderedLine>` where each line is a sequence of `(text, r, g, b)` spans. This plan focuses on the data transformation layer -- no GPU rendering yet.

## Why Wave 2

These renderers consume the `ContentType` and content buffers produced by PLAN-1.1. They produce data consumed by PLAN-3.1 (rendering integration). They are pure data transformations with no wgpu dependency, making them ideal for TDD.

## Design Notes

The `StructuredBlock` model is the bridge between the VT parser (Wave 1) and the renderer (Wave 3):

```
pub struct StyledSpan {
    pub text: String,
    pub color: (u8, u8, u8),      // RGB
    pub bold: bool,
    pub italic: bool,
}

pub struct RenderedLine {
    pub spans: Vec<StyledSpan>,
}

pub struct StructuredBlock {
    pub block_type: ContentType,
    pub start_row: usize,         // grid row where this block starts
    pub line_count: usize,        // number of grid rows this block covers
    pub rendered_lines: Vec<RenderedLine>,
    pub raw_content: String,      // original content for copy-to-clipboard
}
```

The `SyntaxSet` and `ThemeSet` are expensive to load (~23ms). They must be loaded once. The renderer module exposes a `HighlightEngine` struct that owns them and provides methods for each content type.

## Tasks

<task id="1" files="arcterm-render/src/structured.rs, arcterm-render/src/lib.rs" tdd="true">
  <action>Create `arcterm-render/src/structured.rs` with the StructuredBlock data model and syntect-based code block highlighter:

1. Define `StyledSpan`, `RenderedLine`, `StructuredBlock` structs as described above. Import `ContentType` from `arcterm_vt`.

2. Define `HighlightEngine` struct:
   ```
   pub struct HighlightEngine {
       syntax_set: SyntaxSet,
       theme_set: ThemeSet,
   }
   ```

3. Implement `HighlightEngine::new()`: call `SyntaxSet::load_defaults_newlines()` and `ThemeSet::load_defaults()`. Store both.

4. Implement `HighlightEngine::highlight_code(&self, content: &str, language_hint: Option<&str>) -> Vec<RenderedLine>`:
   - Look up syntax by extension (`self.syntax_set.find_syntax_by_extension(hint)`), fall back to `find_syntax_by_first_line(content)`, then to `find_syntax_plain_text()`.
   - Create `HighlightLines::new(syntax, &self.theme_set.themes["base16-ocean.dark"])`.
   - For each line in `content.lines()`: call `highlight_line(line, &self.syntax_set)`. Map each `(Style, &str)` to a `StyledSpan` using `style.foreground.r/g/b` and `style.font_style` flags.
   - Return the Vec of RenderedLines.

5. Add `pub mod structured;` to `arcterm-render/src/lib.rs`.

6. Add `arcterm-vt = { path = "../arcterm-vt" }` to `arcterm-render/Cargo.toml` dependencies (needed for ContentType import).

Write tests first:
- `highlight_code("fn main() {}", Some("rs"))` returns at least one RenderedLine with at least one span whose color is NOT (0,0,0) and NOT (255,255,255) (confirming syntax highlighting is active)
- `highlight_code` with `None` language hint on Rust code still produces colored output (first-line detection)
- `highlight_code` with unknown extension falls back to plain text (all spans have same color)
- Empty content returns empty Vec
- Multi-line content returns one RenderedLine per line</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-render -- structured --nocapture</verify>
  <done>All code highlighting tests pass. syntect produces colored spans for Rust, Python, and plain text. RenderedLine count matches input line count.</done>
</task>

<task id="2" files="arcterm-render/src/structured.rs" tdd="true">
  <action>Add diff renderer and JSON pretty-printer to `HighlightEngine`:

1. Implement `HighlightEngine::highlight_diff(&self, content: &str) -> Vec<RenderedLine>`:
   - Parse line-by-line. Color rules:
     - Lines starting with `---` or `+++`: file header color (bright white, 220, 220, 220)
     - Lines starting with `@@`: hunk header color (cyan, 0, 180, 180)
     - Lines starting with `+`: addition color (green, 80, 200, 80)
     - Lines starting with `-`: deletion color (red, 200, 80, 80)
     - All other lines: context color (default gray, 180, 180, 180)
   - Each line becomes a single-span RenderedLine (no intra-line coloring for Phase 4).

2. Implement `HighlightEngine::highlight_json(&self, content: &str) -> Vec<RenderedLine>`:
   - First, attempt `serde_json::from_str::<serde_json::Value>(content)`. If parse fails, return the content as plain uncolored lines.
   - If parse succeeds, pretty-print with `serde_json::to_string_pretty(&value)`.
   - Scan the pretty-printed output line by line. For each line, produce spans:
     - Quoted strings on the left of `:` (keys): key color (140, 200, 255)
     - Quoted strings on the right of `:` (values): string color (200, 200, 100)
     - Numbers: number color (180, 140, 255)
     - `true`/`false`: bool color (255, 140, 100)
     - `null`: null color (150, 150, 150)
     - Structural characters (`{`, `}`, `[`, `]`, `,`, `:`): structure color (200, 200, 200)
   - Use a simple state machine or character-by-character scan of each pretty-printed line. No need for a full JSON tokenizer since the input is `to_string_pretty` output (known format).

3. Add `serde_json` dependency to `arcterm-render/Cargo.toml`: `serde_json.workspace = true`.

Write tests first:
- Diff: `"--- a/file.rs\n+++ b/file.rs\n@@ -1,3 +1,3 @@\n context\n-old\n+new"` produces 6 RenderedLines with correct colors for each line type
- Diff: line with no prefix gets context color
- JSON: `{"name": "arcterm", "version": 1, "active": true, "data": null}` produces colored spans with keys in key color, string values in string color, 1 in number color, true in bool color, null in null color
- JSON: invalid JSON input (`{not json}`) returns uncolored plain text lines
- JSON: empty object `{}` produces at least one RenderedLine
- JSON: nested object produces indented pretty-printed output</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-render -- structured --nocapture</verify>
  <done>All diff and JSON renderer tests pass. Diff lines have correct colors by prefix. JSON keys, values, numbers, booleans, and null each have distinct colors. Invalid JSON falls back to plain text.</done>
</task>

<task id="3" files="arcterm-render/src/structured.rs" tdd="true">
  <action>Add markdown renderer and a unified `render_block` dispatch method to `HighlightEngine`:

1. Implement `HighlightEngine::highlight_markdown(&self, content: &str) -> Vec<RenderedLine>`:
   - Use `pulldown_cmark::Parser::new_ext(content, pulldown_cmark::Options::all())` wrapped in `pulldown_cmark::TextMergeStream::new(parser)`.
   - Walk events building `Vec<RenderedLine>`. Maintain a style stack:
     - `Event::Start(Tag::Heading { level, .. })`: push heading style (bold=true, color based on level: h1=(100, 200, 255), h2=(140, 200, 220), h3+=(180, 200, 200))
     - `Event::End(TagEnd::Heading(_))`: pop style, push a newline RenderedLine
     - `Event::Start(Tag::Strong)`: push bold=true
     - `Event::Start(Tag::Emphasis)`: push italic=true
     - `Event::Code(text)`: emit span with inline code color (200, 180, 140) and the text content
     - `Event::Start(Tag::List(None))`: track indent level for bullet prefix
     - `Event::Start(Tag::Item)`: prepend "  " * indent + "- " to the next Text event
     - `Event::Text(text)`: emit span with current top-of-stack style. Split on newlines to create multiple RenderedLines.
     - `Event::Start(Tag::CodeBlock(kind))`: delegate inner text to `highlight_code` with the language hint from `kind`
     - `Event::SoftBreak` / `Event::HardBreak`: start a new RenderedLine
   - Accumulate spans into the current RenderedLine. Start a new RenderedLine on newlines and breaks.

2. Implement `HighlightEngine::render_block(&self, content_type: ContentType, content: &str, attrs: &[(String, String)]) -> Vec<RenderedLine>`:
   - Match on `content_type`:
     - `CodeBlock` -> `highlight_code(content, attrs.iter().find(|(k,_)| k == "lang").map(|(_,v)| v.as_str()))`
     - `Diff` -> `highlight_diff(content)`
     - `Json` -> `highlight_json(content)`
     - `Markdown` -> `highlight_markdown(content)`
     - `Error` -> single RenderedLine with red color (255, 80, 80)
     - `Progress` / `Plan` / `Image` -> plain text fallback (default color 200, 200, 200)

Write tests first:
- Markdown: `"# Hello\n\nSome **bold** and *italic* text"` produces RenderedLines where "Hello" has heading color and bold, "bold" has bold=true, "italic" has italic=true
- Markdown: `"- item 1\n- item 2"` produces lines with bullet prefixes
- Markdown: inline code `` `code` `` produces a span with code color
- Markdown: fenced code block with language hint delegates to highlight_code and produces colored output
- `render_block` dispatches CodeBlock to highlight_code
- `render_block` dispatches Diff to highlight_diff
- `render_block` dispatches Json to highlight_json
- `render_block` dispatches Error to red-colored line</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-render -- structured --nocapture</verify>
  <done>All markdown and dispatch tests pass. Headings, bold, italic, inline code, lists, and fenced code blocks render correctly. `render_block` dispatches to the correct renderer for each ContentType.</done>
</task>
