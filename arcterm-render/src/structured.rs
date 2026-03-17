//! Structured content renderers for OSC 7770 blocks.
//!
//! This module transforms raw text content (code, diff, JSON, markdown) into
//! coloured line spans (`Vec<RenderedLine>`) suitable for GPU rendering by glyphon.

use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// ContentType — semantic classification for OSC 7770 structured content
// ---------------------------------------------------------------------------

/// Classifies the type of structured content carried in an OSC 7770 block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentType {
    /// A syntax-highlighted source code block.
    CodeBlock,
    /// A unified diff.
    Diff,
    /// A structured plan or checklist.
    Plan,
    /// Markdown-formatted text.
    Markdown,
    /// JSON data.
    Json,
    /// An error message.
    Error,
    /// A progress indicator.
    Progress,
    /// An inline image (Kitty protocol).
    Image,
}
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, ThemeSet},
    parsing::SyntaxSet,
};

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// A single styled run of text within a rendered line.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledSpan {
    pub text: String,
    pub color: (u8, u8, u8),
    pub bold: bool,
    pub italic: bool,
}

impl StyledSpan {
    pub fn plain(text: impl Into<String>, color: (u8, u8, u8)) -> Self {
        Self {
            text: text.into(),
            color,
            bold: false,
            italic: false,
        }
    }
}

/// One logical line in rendered output: a sequence of styled spans.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedLine {
    pub spans: Vec<StyledSpan>,
}

impl RenderedLine {
    pub fn empty() -> Self {
        Self { spans: Vec::new() }
    }

    pub fn single(span: StyledSpan) -> Self {
        Self { spans: vec![span] }
    }

    pub fn plain(text: impl Into<String>, color: (u8, u8, u8)) -> Self {
        Self::single(StyledSpan::plain(text, color))
    }
}

/// A fully-rendered structured content block.
#[derive(Debug, Clone)]
pub struct StructuredBlock {
    pub block_type: ContentType,
    pub start_row: usize,
    pub line_count: usize,
    pub rendered_lines: Vec<RenderedLine>,
    pub raw_content: String,
}

// ---------------------------------------------------------------------------
// HighlightEngine — lazily loaded SyntaxSet and ThemeSet
//
// The syntect defaults take ~23 ms to load. Using OnceLock defers that cost
// to the first call to highlight_code() or highlight_markdown(), which happens
// only when the user actually receives structured output — not at cold start.
// ---------------------------------------------------------------------------

/// Process-wide lazily-initialised syntect state.
static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

pub struct HighlightEngine;

impl HighlightEngine {
    /// Create a new engine.
    ///
    /// Construction is now free: the underlying `SyntaxSet` and `ThemeSet` are
    /// loaded on first use via process-global `OnceLock`s.  This removes the
    /// ~23 ms syntect initialisation cost from the cold-start path.
    pub fn new() -> Self {
        Self
    }

    /// Ensure syntect defaults are loaded.  Called internally before any
    /// operation that requires the syntax or theme set.
    fn syntax_set() -> &'static SyntaxSet {
        SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
    }

    fn theme_set() -> &'static ThemeSet {
        THEME_SET.get_or_init(ThemeSet::load_defaults)
    }

    // -----------------------------------------------------------------------
    // Code highlighter
    // -----------------------------------------------------------------------

    /// Highlight source code with syntect.
    ///
    /// `language_hint` is a file extension (e.g. `"rs"`, `"py"`).  If `None`,
    /// syntect attempts first-line detection.  Falls back to plain text.
    pub fn highlight_code(&self, content: &str, language_hint: Option<&str>) -> Vec<RenderedLine> {
        if content.is_empty() {
            return Vec::new();
        }

        let ss = Self::syntax_set();
        let ts = Self::theme_set();

        let syntax = language_hint
            .and_then(|hint| ss.find_syntax_by_extension(hint))
            .or_else(|| ss.find_syntax_by_first_line(content))
            .unwrap_or_else(|| ss.find_syntax_plain_text());

        let theme = &ts.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        content
            .lines()
            .map(|line| {
                let spans_raw = highlighter.highlight_line(line, ss).unwrap_or_default();

                let spans = spans_raw
                    .into_iter()
                    .filter_map(|(style, text)| {
                        if text.is_empty() {
                            return None;
                        }
                        Some(StyledSpan {
                            text: text.to_string(),
                            color: (style.foreground.r, style.foreground.g, style.foreground.b),
                            bold: style.font_style.contains(FontStyle::BOLD),
                            italic: style.font_style.contains(FontStyle::ITALIC),
                        })
                    })
                    .collect();

                RenderedLine { spans }
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // Diff renderer
    // -----------------------------------------------------------------------

    /// Render a unified diff with per-line colouring.
    pub fn highlight_diff(&self, content: &str) -> Vec<RenderedLine> {
        content
            .lines()
            .map(|line| {
                let color = if line.starts_with("---") || line.starts_with("+++") {
                    (220, 220, 220) // file header: bright white
                } else if line.starts_with("@@") {
                    (0, 180, 180) // hunk header: cyan
                } else if line.starts_with('+') {
                    (80, 200, 80) // addition: green
                } else if line.starts_with('-') {
                    (200, 80, 80) // deletion: red
                } else {
                    (180, 180, 180) // context: gray
                };
                RenderedLine::plain(line, color)
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // JSON pretty-printer
    // -----------------------------------------------------------------------

    /// Render JSON with colour-coded keys, values, numbers, booleans, and null.
    ///
    /// On parse failure the content is returned as plain uncoloured text lines.
    pub fn highlight_json(&self, content: &str) -> Vec<RenderedLine> {
        match serde_json::from_str::<serde_json::Value>(content) {
            Err(_) => {
                // Invalid JSON — return plain text lines.
                content
                    .lines()
                    .map(|line| RenderedLine::plain(line, (200, 200, 200)))
                    .collect()
            }
            Ok(value) => {
                let pretty =
                    serde_json::to_string_pretty(&value).unwrap_or_else(|_| content.to_string());
                pretty.lines().map(Self::colorize_json_line).collect()
            }
        }
    }

    /// Tokenise one pretty-printed JSON line into coloured spans.
    fn colorize_json_line(line: &str) -> RenderedLine {
        // Colour constants.
        const KEY_COLOR: (u8, u8, u8) = (140, 200, 255);
        const STRING_COLOR: (u8, u8, u8) = (200, 200, 100);
        const NUMBER_COLOR: (u8, u8, u8) = (180, 140, 255);
        const BOOL_COLOR: (u8, u8, u8) = (255, 140, 100);
        const NULL_COLOR: (u8, u8, u8) = (150, 150, 150);
        const STRUCT_COLOR: (u8, u8, u8) = (200, 200, 200);

        let mut spans: Vec<StyledSpan> = Vec::new();
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;

        // Track whether we have seen a colon on this line (for key vs value distinction).
        // Keys appear before the first colon that follows a quoted string at the top level.
        let mut colon_seen = false;

        while i < len {
            let ch = chars[i];

            if ch == '"' {
                // Consume a quoted string.
                let start = i;
                i += 1;
                while i < len {
                    if chars[i] == '\\' {
                        i += 2; // skip escaped char
                    } else if chars[i] == '"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                let s: String = chars[start..i].iter().collect();

                // Peek ahead (skip whitespace) to see if ':' follows.
                let mut peek = i;
                while peek < len && chars[peek] == ' ' {
                    peek += 1;
                }
                let followed_by_colon = peek < len && chars[peek] == ':';

                let color = if !colon_seen && followed_by_colon {
                    KEY_COLOR
                } else {
                    STRING_COLOR
                };
                spans.push(StyledSpan::plain(s, color));
            } else if ch == ':' {
                colon_seen = true;
                spans.push(StyledSpan::plain(":", STRUCT_COLOR));
                i += 1;
            } else if "{[]}".contains(ch) || ch == ',' {
                spans.push(StyledSpan::plain(ch.to_string(), STRUCT_COLOR));
                i += 1;
            } else if ch.is_ascii_digit() || ch == '-' {
                // Number literal.
                let start = i;
                i += 1;
                while i < len
                    && (chars[i].is_ascii_digit()
                        || chars[i] == '.'
                        || chars[i] == 'e'
                        || chars[i] == 'E'
                        || chars[i] == '+'
                        || chars[i] == '-')
                {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                spans.push(StyledSpan::plain(s, NUMBER_COLOR));
            } else if line[i..].starts_with("true") {
                spans.push(StyledSpan::plain("true", BOOL_COLOR));
                i += 4;
            } else if line[i..].starts_with("false") {
                spans.push(StyledSpan::plain("false", BOOL_COLOR));
                i += 5;
            } else if line[i..].starts_with("null") {
                spans.push(StyledSpan::plain("null", NULL_COLOR));
                i += 4;
            } else if ch == ' ' {
                // Accumulate leading/inter-token whitespace as structure colour.
                let start = i;
                while i < len && chars[i] == ' ' {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                spans.push(StyledSpan::plain(s, STRUCT_COLOR));
            } else {
                // Any other character (shouldn't happen in valid pretty-printed JSON).
                spans.push(StyledSpan::plain(ch.to_string(), STRUCT_COLOR));
                i += 1;
            }
        }

        RenderedLine { spans }
    }

    // -----------------------------------------------------------------------
    // Markdown renderer
    // -----------------------------------------------------------------------

    /// Render Markdown with heading, bold, italic, list, and fenced code styling.
    ///
    /// Headings are coloured and bold; fenced code blocks delegate to
    /// `highlight_code` for syntax-highlighted output.
    pub fn highlight_markdown(&self, content: &str) -> Vec<RenderedLine> {
        use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, TextMergeStream};

        const H1_COLOR: (u8, u8, u8) = (100, 200, 255);
        const H2_COLOR: (u8, u8, u8) = (140, 200, 220);
        const H3_COLOR: (u8, u8, u8) = (180, 200, 200);
        const NORMAL_COLOR: (u8, u8, u8) = (200, 200, 200);
        const CODE_COLOR: (u8, u8, u8) = (200, 180, 140);

        #[derive(Clone)]
        enum StackEntry {
            Normal {
                color: (u8, u8, u8),
                bold: bool,
                italic: bool,
            },
            /// Marks an active fenced code block on the stack.
            /// The lang/text are tracked via `code_block_buf`.
            CodeBlock,
        }

        let mut lines: Vec<RenderedLine> = Vec::new();
        let mut current: Vec<StyledSpan> = Vec::new();

        let mut stack: Vec<StackEntry> = vec![StackEntry::Normal {
            color: NORMAL_COLOR,
            bold: false,
            italic: false,
        }];

        let mut list_indent: usize = 0;
        let mut pending_item_prefix = false;
        let mut code_block_buf: Option<(Option<String>, String)> = None; // (lang, text)

        let push_line_fn = |lines: &mut Vec<RenderedLine>, current: &mut Vec<StyledSpan>| {
            lines.push(RenderedLine {
                spans: std::mem::take(current),
            });
        };

        let top_style = |stack: &Vec<StackEntry>| -> ((u8, u8, u8), bool, bool) {
            for entry in stack.iter().rev() {
                if let StackEntry::Normal {
                    color,
                    bold,
                    italic,
                } = entry
                {
                    return (*color, *bold, *italic);
                }
            }
            (NORMAL_COLOR, false, false)
        };

        let parser = Parser::new_ext(content, Options::all());
        let stream = TextMergeStream::new(parser);

        for event in stream {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    let color = match level as u32 {
                        1 => H1_COLOR,
                        2 => H2_COLOR,
                        _ => H3_COLOR,
                    };
                    stack.push(StackEntry::Normal {
                        color,
                        bold: true,
                        italic: false,
                    });
                }
                Event::End(TagEnd::Heading(_)) => {
                    stack.pop();
                    push_line_fn(&mut lines, &mut current);
                    lines.push(RenderedLine::empty());
                }

                Event::Start(Tag::Paragraph) => {}
                Event::End(TagEnd::Paragraph) => {
                    if !current.is_empty() {
                        push_line_fn(&mut lines, &mut current);
                    }
                    lines.push(RenderedLine::empty());
                }

                Event::Start(Tag::Strong) => {
                    let (color, _, italic) = top_style(&stack);
                    stack.push(StackEntry::Normal {
                        color,
                        bold: true,
                        italic,
                    });
                }
                Event::End(TagEnd::Strong) => {
                    stack.pop();
                }

                Event::Start(Tag::Emphasis) => {
                    let (color, bold, _) = top_style(&stack);
                    stack.push(StackEntry::Normal {
                        color,
                        bold,
                        italic: true,
                    });
                }
                Event::End(TagEnd::Emphasis) => {
                    stack.pop();
                }

                Event::Start(Tag::List(_)) => {
                    list_indent += 1;
                }
                Event::End(TagEnd::List(_)) => {
                    list_indent = list_indent.saturating_sub(1);
                }
                Event::Start(Tag::Item) => {
                    pending_item_prefix = true;
                }
                Event::End(TagEnd::Item) => {
                    if !current.is_empty() {
                        push_line_fn(&mut lines, &mut current);
                    }
                }

                Event::Code(text) => {
                    current.push(StyledSpan {
                        text: text.to_string(),
                        color: CODE_COLOR,
                        bold: false,
                        italic: false,
                    });
                }

                Event::Start(Tag::CodeBlock(kind)) => {
                    let lang: Option<String> = match &kind {
                        CodeBlockKind::Fenced(info) => {
                            let s = info.split_whitespace().next().unwrap_or("").to_string();
                            if s.is_empty() { None } else { Some(s) }
                        }
                        CodeBlockKind::Indented => None,
                    };
                    // Flush any current line before the code block.
                    if !current.is_empty() {
                        push_line_fn(&mut lines, &mut current);
                    }
                    code_block_buf = Some((lang.clone(), String::new()));
                    stack.push(StackEntry::CodeBlock);
                }
                Event::End(TagEnd::CodeBlock) => {
                    stack.pop();
                    if let Some((lang, text)) = code_block_buf.take() {
                        let lang_ref = lang.as_deref();
                        let ext = lang_ref.map(|l| match l {
                            "rust" | "rs" => "rs",
                            "python" | "py" => "py",
                            "javascript" | "js" => "js",
                            "typescript" | "ts" => "ts",
                            "json" => "json",
                            "yaml" | "yml" => "yaml",
                            "toml" => "toml",
                            "bash" | "sh" => "sh",
                            "c" => "c",
                            "cpp" | "c++" => "cpp",
                            other => other,
                        });
                        let highlighted = self.highlight_code(&text, ext);
                        lines.extend(highlighted);
                    }
                }

                Event::Text(text) => {
                    // If inside a code block, accumulate into the buffer.
                    if let Some((_, buf)) = &mut code_block_buf {
                        buf.push_str(&text);
                        continue;
                    }

                    let (color, bold, italic) = top_style(&stack);

                    if pending_item_prefix {
                        pending_item_prefix = false;
                        let prefix = format!("{}  - ", "  ".repeat(list_indent.saturating_sub(1)));
                        current.push(StyledSpan {
                            text: prefix,
                            color,
                            bold,
                            italic,
                        });
                    }

                    for (idx, segment) in text.split('\n').enumerate() {
                        if idx > 0 {
                            push_line_fn(&mut lines, &mut current);
                        }
                        if !segment.is_empty() {
                            current.push(StyledSpan {
                                text: segment.to_string(),
                                color,
                                bold,
                                italic,
                            });
                        }
                    }
                }

                Event::SoftBreak | Event::HardBreak => {
                    push_line_fn(&mut lines, &mut current);
                }

                _ => {}
            }
        }

        if !current.is_empty() {
            push_line_fn(&mut lines, &mut current);
        }

        lines
    }

    // -----------------------------------------------------------------------
    // Unified dispatch
    // -----------------------------------------------------------------------

    /// Dispatch rendering to the appropriate renderer based on `content_type`.
    pub fn render_block(
        &self,
        content_type: ContentType,
        content: &str,
        attrs: &[(String, String)],
    ) -> Vec<RenderedLine> {
        match content_type {
            ContentType::CodeBlock => {
                let lang = attrs
                    .iter()
                    .find(|(k, _)| k == "lang")
                    .map(|(_, v)| v.as_str());
                self.highlight_code(content, lang)
            }
            ContentType::Diff => self.highlight_diff(content),
            ContentType::Json => self.highlight_json(content),
            ContentType::Markdown => self.highlight_markdown(content),
            ContentType::Error => {
                vec![RenderedLine::plain(content, (255, 80, 80))]
            }
            ContentType::Progress | ContentType::Plan | ContentType::Image => content
                .lines()
                .map(|l| RenderedLine::plain(l, (200, 200, 200)))
                .collect(),
        }
    }
}

impl Default for HighlightEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> HighlightEngine {
        HighlightEngine::new()
    }

    // -------------------------------------------------------------------------
    // Task 1: Code highlighting
    // -------------------------------------------------------------------------

    #[test]
    fn highlight_code_rust_produces_colored_spans() {
        let e = engine();
        let lines = e.highlight_code("fn main() {}", Some("rs"));
        assert!(!lines.is_empty(), "must produce at least one RenderedLine");
        let any_colored = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.color != (0, 0, 0) && s.color != (255, 255, 255));
        assert!(
            any_colored,
            "syntect must produce at least one non-black/white span for Rust"
        );
    }

    #[test]
    fn highlight_code_no_hint_detects_rust_by_first_line() {
        let e = engine();
        // A shebang or well-known first line would be detected. For Rust we use
        // `fn main()` which syntect may not detect via first-line. We fall back to
        // plain text. The important thing is it must not panic and must return lines.
        let lines = e.highlight_code("fn main() {\n    println!(\"hello\");\n}", None);
        assert_eq!(
            lines.len(),
            3,
            "must produce one RenderedLine per input line"
        );
    }

    #[test]
    fn highlight_code_unknown_extension_falls_back_to_plain_text() {
        let e = engine();
        let lines = e.highlight_code("hello world", Some("zzz_unknown_ext"));
        assert_eq!(lines.len(), 1);
        // Plain text: all spans have the same colour (the theme's default).
        let colors: Vec<_> = lines[0].spans.iter().map(|s| s.color).collect();
        // Plain text should produce exactly one span per line.
        // The key property: must not panic and must return a line.
        assert!(
            !colors.is_empty(),
            "plain-text fallback must still produce spans"
        );
    }

    #[test]
    fn highlight_code_empty_returns_empty_vec() {
        let e = engine();
        let lines = e.highlight_code("", Some("rs"));
        assert!(lines.is_empty(), "empty content must return empty Vec");
    }

    #[test]
    fn highlight_code_multi_line_count_matches() {
        let e = engine();
        let src = "fn foo() {}\nfn bar() {}\nfn baz() {}";
        let lines = e.highlight_code(src, Some("rs"));
        assert_eq!(
            lines.len(),
            3,
            "must produce one RenderedLine per input line"
        );
    }

    #[test]
    fn highlight_code_python_produces_colored_spans() {
        let e = engine();
        let src = "def hello():\n    print('world')";
        let lines = e.highlight_code(src, Some("py"));
        assert_eq!(lines.len(), 2);
        let any_colored = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.color != (0, 0, 0) && s.color != (255, 255, 255));
        assert!(
            any_colored,
            "Python highlighting must produce coloured spans"
        );
    }

    // -------------------------------------------------------------------------
    // Task 2: Diff renderer
    // -------------------------------------------------------------------------

    #[test]
    fn highlight_diff_produces_correct_line_count() {
        let e = engine();
        let diff = "--- a/file.rs\n+++ b/file.rs\n@@ -1,3 +1,3 @@\n context\n-old\n+new";
        let lines = e.highlight_diff(diff);
        assert_eq!(
            lines.len(),
            6,
            "must produce one RenderedLine per diff line"
        );
    }

    #[test]
    fn highlight_diff_file_header_color() {
        let e = engine();
        let diff = "--- a/file.rs\n+++ b/file.rs";
        let lines = e.highlight_diff(diff);
        assert_eq!(
            lines[0].spans[0].color,
            (220, 220, 220),
            "--- line must be bright white"
        );
        assert_eq!(
            lines[1].spans[0].color,
            (220, 220, 220),
            "+++ line must be bright white"
        );
    }

    #[test]
    fn highlight_diff_hunk_header_color() {
        let e = engine();
        let diff = "@@ -1,3 +1,3 @@";
        let lines = e.highlight_diff(diff);
        assert_eq!(
            lines[0].spans[0].color,
            (0, 180, 180),
            "@@ line must be cyan"
        );
    }

    #[test]
    fn highlight_diff_addition_color() {
        let e = engine();
        let diff = "+new line";
        let lines = e.highlight_diff(diff);
        assert_eq!(
            lines[0].spans[0].color,
            (80, 200, 80),
            "+ line must be green"
        );
    }

    #[test]
    fn highlight_diff_deletion_color() {
        let e = engine();
        let diff = "-old line";
        let lines = e.highlight_diff(diff);
        assert_eq!(lines[0].spans[0].color, (200, 80, 80), "- line must be red");
    }

    #[test]
    fn highlight_diff_context_color() {
        let e = engine();
        let diff = " context line";
        let lines = e.highlight_diff(diff);
        assert_eq!(
            lines[0].spans[0].color,
            (180, 180, 180),
            "context line must be gray"
        );
    }

    #[test]
    fn highlight_diff_no_prefix_context_color() {
        let e = engine();
        let diff = "no prefix";
        let lines = e.highlight_diff(diff);
        assert_eq!(
            lines[0].spans[0].color,
            (180, 180, 180),
            "no-prefix line must be gray"
        );
    }

    // -------------------------------------------------------------------------
    // Task 2: JSON renderer
    // -------------------------------------------------------------------------

    fn find_span_with_text<'a>(lines: &'a [RenderedLine], text: &str) -> Option<&'a StyledSpan> {
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.text == text)
    }

    #[test]
    fn highlight_json_key_color() {
        let e = engine();
        let json = r#"{"name": "arcterm"}"#;
        let lines = e.highlight_json(json);
        let key_span = find_span_with_text(&lines, "\"name\"");
        assert!(key_span.is_some(), "must find 'name' key span");
        assert_eq!(
            key_span.unwrap().color,
            (140, 200, 255),
            "JSON key must use key color"
        );
    }

    #[test]
    fn highlight_json_string_value_color() {
        let e = engine();
        let json = r#"{"name": "arcterm"}"#;
        let lines = e.highlight_json(json);
        let val_span = find_span_with_text(&lines, "\"arcterm\"");
        assert!(val_span.is_some(), "must find 'arcterm' value span");
        assert_eq!(
            val_span.unwrap().color,
            (200, 200, 100),
            "JSON string value must use string color"
        );
    }

    #[test]
    fn highlight_json_number_color() {
        let e = engine();
        let json = r#"{"version": 1}"#;
        let lines = e.highlight_json(json);
        let num_span = find_span_with_text(&lines, "1");
        assert!(num_span.is_some(), "must find numeric span '1'");
        assert_eq!(
            num_span.unwrap().color,
            (180, 140, 255),
            "JSON number must use number color"
        );
    }

    #[test]
    fn highlight_json_bool_color() {
        let e = engine();
        let json = r#"{"active": true}"#;
        let lines = e.highlight_json(json);
        let bool_span = find_span_with_text(&lines, "true");
        assert!(bool_span.is_some(), "must find 'true' span");
        assert_eq!(
            bool_span.unwrap().color,
            (255, 140, 100),
            "JSON bool must use bool color"
        );
    }

    #[test]
    fn highlight_json_null_color() {
        let e = engine();
        let json = r#"{"data": null}"#;
        let lines = e.highlight_json(json);
        let null_span = find_span_with_text(&lines, "null");
        assert!(null_span.is_some(), "must find 'null' span");
        assert_eq!(
            null_span.unwrap().color,
            (150, 150, 150),
            "JSON null must use null color"
        );
    }

    #[test]
    fn highlight_json_invalid_falls_back_to_plain() {
        let e = engine();
        let bad = "{not json}";
        let lines = e.highlight_json(bad);
        // Must return at least one line with the original content visible.
        assert!(!lines.is_empty(), "invalid JSON must still produce lines");
        // All spans must use the plain fallback color (200, 200, 200).
        for line in &lines {
            for span in &line.spans {
                assert_eq!(
                    span.color,
                    (200, 200, 200),
                    "invalid JSON fallback must use plain color"
                );
            }
        }
    }

    #[test]
    fn highlight_json_empty_object_produces_lines() {
        let e = engine();
        let lines = e.highlight_json("{}");
        assert!(
            !lines.is_empty(),
            "empty object must produce at least one RenderedLine"
        );
    }

    #[test]
    fn highlight_json_nested_object() {
        let e = engine();
        let json = r#"{"outer": {"inner": 42}}"#;
        let lines = e.highlight_json(json);
        // Pretty-printed nested JSON must produce multiple lines.
        assert!(
            lines.len() > 1,
            "nested JSON must produce multiple RenderedLines"
        );
    }

    // -------------------------------------------------------------------------
    // Task 3: Markdown renderer
    // -------------------------------------------------------------------------

    #[test]
    fn highlight_markdown_h1_heading_bold_color() {
        let e = engine();
        let md = "# Hello";
        let lines = e.highlight_markdown(md);
        // Find a span with "Hello" text.
        let heading_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.text.contains("Hello"));
        assert!(heading_span.is_some(), "must find 'Hello' span");
        let span = heading_span.unwrap();
        assert!(span.bold, "h1 heading text must be bold");
        assert_eq!(span.color, (100, 200, 255), "h1 heading must use H1_COLOR");
    }

    #[test]
    fn highlight_markdown_bold_text() {
        let e = engine();
        let md = "Some **bold** text";
        let lines = e.highlight_markdown(md);
        let bold_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.text == "bold");
        assert!(bold_span.is_some(), "must find 'bold' span");
        assert!(bold_span.unwrap().bold, "bold span must have bold=true");
    }

    #[test]
    fn highlight_markdown_italic_text() {
        let e = engine();
        let md = "Some *italic* text";
        let lines = e.highlight_markdown(md);
        let italic_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.text == "italic");
        assert!(italic_span.is_some(), "must find 'italic' span");
        assert!(
            italic_span.unwrap().italic,
            "italic span must have italic=true"
        );
    }

    #[test]
    fn highlight_markdown_inline_code_color() {
        let e = engine();
        let md = "Use `code` here";
        let lines = e.highlight_markdown(md);
        let code_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.text == "code");
        assert!(code_span.is_some(), "must find inline code span");
        assert_eq!(
            code_span.unwrap().color,
            (200, 180, 140),
            "inline code must use CODE_COLOR"
        );
    }

    #[test]
    fn highlight_markdown_list_items_have_bullet_prefix() {
        let e = engine();
        let md = "- item 1\n- item 2";
        let lines = e.highlight_markdown(md);
        // At least one line must start with a bullet prefix.
        let has_bullet = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.text.contains("- "));
        assert!(has_bullet, "list items must include bullet prefix");
    }

    #[test]
    fn highlight_markdown_fenced_code_block_delegates_to_highlight_code() {
        let e = engine();
        let md = "```rust\nfn main() {}\n```";
        let lines = e.highlight_markdown(md);
        // The fenced block must produce colored spans (not plain gray).
        let any_colored = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.color != (200, 200, 200) && !s.text.trim().is_empty());
        assert!(
            any_colored,
            "fenced code block must delegate to highlight_code and produce colors"
        );
    }

    // -------------------------------------------------------------------------
    // Task 3: render_block dispatch
    // -------------------------------------------------------------------------

    #[test]
    fn render_block_dispatches_code_block() {
        let e = engine();
        let attrs = vec![("lang".to_string(), "rs".to_string())];
        let lines = e.render_block(ContentType::CodeBlock, "fn main() {}", &attrs);
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_block_dispatches_diff() {
        let e = engine();
        let lines = e.render_block(ContentType::Diff, "+new\n-old", &[]);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].spans[0].color, (80, 200, 80));
        assert_eq!(lines[1].spans[0].color, (200, 80, 80));
    }

    #[test]
    fn render_block_dispatches_json() {
        let e = engine();
        let lines = e.render_block(ContentType::Json, r#"{"k": 1}"#, &[]);
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_block_dispatches_error_to_red() {
        let e = engine();
        let lines = e.render_block(ContentType::Error, "something went wrong", &[]);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].color, (255, 80, 80));
    }

    #[test]
    fn render_block_dispatches_markdown() {
        let e = engine();
        let lines = e.render_block(ContentType::Markdown, "# Hello", &[]);
        let found = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.text.contains("Hello"));
        assert!(
            found,
            "render_block with Markdown must produce heading spans"
        );
    }
}
