# Data Model: Structured Output via OSC 7770

**Date**: 2026-03-19
**Feature**: 003-structured-output-osc7770

## StructuredBlock

A parsed OSC 7770 payload representing one unit of rich content.

**Attributes**:
- `block_type: BlockType` — discriminator for rendering
- `title: Option<String>` — optional title displayed above the block
- `content: String` — the raw content (code text, JSON string, diff text)
- `language: Option<String>` — for code blocks, the syntax highlighting language
- `image_format: Option<String>` — for image blocks, the format (png, jpeg)
- `image_data: Option<Vec<u8>>` — for image blocks, decoded binary data

**Validation rules**:
- `block_type` must be one of the supported types
- `content` must not exceed the configured maximum payload size (default 10MB)
- `language` is required for `code` type, optional for others
- `image_format` and `image_data` are required for `image` type

## BlockType

Enum of supported structured content types.

**Variants**:
- `Code` — syntax-highlighted source code
- `Json` — interactive collapsible JSON tree
- `Diff` — side-by-side colored diff view
- `Image` — inline rendered image

## OSC 7770 Payload Schema

The JSON payload within the escape sequence.

**Common fields**:
- `type: string` (required) — one of `"code"`, `"json"`, `"diff"`, `"image"`
- `title: string` (optional) — displayed above the block

**Type-specific fields**:

### Code
- `language: string` (required) — language identifier for syntax highlighting
- `content: string` (required) — the source code text

### Json
- `content: string` (required) — the JSON string to parse and render as a tree

### Diff
- `content: string` (required) — unified diff text

### Image
- `format: string` (required) — `"png"` or `"jpeg"`
- `data: string` (required) — base64-encoded image data

## RenderedOutput

The result of converting a StructuredBlock into terminal-native output.

For **text-based types** (code, json, diff):
- A sequence of terminal lines with ANSI SGR color attributes
- Stored in scrollback as normal colored text
- Copy-to-clipboard produces the plain text content

For **image type**:
- An ImageCell grid using the existing iTerm2 image infrastructure
- Stored in scrollback as image cell references
- Copy-to-clipboard produces `[image]` placeholder text

## Relationships

```
OSC 7770 Sequence → parsed → StructuredBlock
StructuredBlock → rendered → RenderedOutput (SGR text or ImageCells)
RenderedOutput → inserted → Terminal scrollback (normal terminal lines)
Terminal scrollback → displayed → GPU renderer (existing pipeline)
```
