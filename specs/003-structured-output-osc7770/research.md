# Research: Structured Output via OSC 7770

**Date**: 2026-03-19
**Feature**: 003-structured-output-osc7770

## Decision 1: Rendering Strategy

**Decision**: Render code/JSON/diff blocks as ANSI SGR-colored text through
the existing terminal text pipeline. Do NOT create custom GPU rendering paths.
For images, reuse the existing iTerm2 image infrastructure.

**Rationale**: WezTerm's GPU renderer already handles ANSI-colored text
perfectly. By converting structured content into SGR escape sequences at the
OSC handler level, we get scrollback, copy-to-clipboard, resize handling, and
font rendering for free. The iTerm2 image path (`assign_image_to_cells()`)
already handles inline images with texture coordinates.

**Alternatives considered**:
- Custom GPU quads per block type — massive engineering effort, duplicate rendering logic
- External overlay window — breaks terminal workflow, doesn't integrate with scrollback

## Decision 2: OSC Parser Integration

**Decision**: Add OSC 7770 to the existing parser by: (1) adding `"7770"` to
the `osc_entries!` macro in `wezterm-escape-parser/src/osc.rs`, (2) adding an
`ArcTermStructuredOutput(String)` variant to `OperatingSystemCommand`, and (3)
handling it in `osc_dispatch()` in `term/src/terminalstate/performer.rs`.

**Rationale**: This follows the exact pattern used for iTerm2's OSC 1337,
OSC 7 (CWD), and OSC 8 (hyperlinks). Minimal new code, maximum reuse.

**Data flow**: PTY bytes → `Parser` → `Action::OperatingSystemCommand` →
`Performer::osc_dispatch()` → parse JSON payload → render as SGR text
or image cells → existing terminal model → GPU renderer.

## Decision 3: Syntax Highlighting Library

**Decision**: Use `syntect` v5.3.0 for syntax highlighting.

**Rationale**: Pure Rust (no C FFI), ~40 languages out of the box via Sublime
Text grammars, battle-tested (used by bat, delta, Zola). tree-sitter would
add 14+ C crates to the build for marginal accuracy gains irrelevant to
one-shot terminal output.

## Decision 4: Crate Structure

**Decision**: Create `arcterm-structured-output` as a new workspace member
crate. It depends on `termwiz` (for escape sequence types), `syntect` (for
highlighting), and `serde_json` (for payload parsing). The OSC handler hook
goes in `term/src/terminalstate/` and calls into the new crate.

**Rationale**: Keeps ArcTerm-specific code in `arcterm-*` crates per
constitution. The crate converts structured payloads into sequences of
`Action::Print` and `Action::Esc` (SGR color changes) that the terminal
state machine already processes natively.

## Decision 5: OSC 7770 Payload Format

**Decision**: JSON payload with `type` discriminator:

```json
{"type": "code", "language": "python", "title": "example.py", "content": "def hello():\n    print('world')"}
{"type": "json", "content": "{\"key\": \"value\"}"}
{"type": "diff", "content": "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new"}
{"type": "image", "format": "png", "data": "<base64>"}
```

Full sequence: `ESC ] 7770 ; <JSON> ST`

**Rationale**: JSON is universally parseable, self-describing, and
extensible. The `type` field enables future content types without
protocol changes. base64 for images follows the iTerm2 convention.

## Decision 6: JSON Tree Interactivity

**Decision**: Render JSON trees as collapsible text blocks using ANSI
SGR colors. Collapsed nodes show `{...}` or `[...]` with a marker.
Interactivity (expand/collapse) is handled by the GUI layer detecting
clicks on markers and re-emitting the block with different expand state.

**Rationale**: Text-based rendering integrates with scrollback and copy.
The GUI layer only needs to handle click-to-toggle, not custom rendering.

## Decision 7: Diff Rendering

**Decision**: Render diffs as colored text — additions in green (SGR 32),
deletions in red (SGR 31), context in default color, headers in bold.
Side-by-side layout is achieved by splitting the terminal width in half
and rendering left/right columns with appropriate padding.

**Rationale**: Unified diff parsing is well-understood. SGR colors are
the standard way terminals display colored text. Side-by-side layout
reuses the existing line-wrapping and padding logic.

## Decision 8: Payload Size Limits

**Decision**: Maximum payload size of 10MB (configurable in `arcterm.lua`).
Payloads exceeding this are rejected with a log warning and no rendering.

**Rationale**: Prevents denial-of-service via massive payloads. 10MB is
generous for code/JSON/diffs and sufficient for most images. Configurable
for users who need larger payloads.

## File Impact

### New crate
- `arcterm-structured-output/` — payload parsing, syntax highlighting, diff coloring, JSON tree rendering, image handling

### Modified files (minimal)
- `wezterm-escape-parser/src/osc.rs` — add OSC 7770 to macro table + enum variant
- `term/src/terminalstate/performer.rs` — add handler in `osc_dispatch()` that calls new crate
- `Cargo.toml` — add `arcterm-structured-output` to workspace members
