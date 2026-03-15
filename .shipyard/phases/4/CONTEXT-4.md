# Phase 4 Context — Design Decisions

## Syntax Highlighting
**Decision:** syntect (Sublime Text syntax definitions)
**Rationale:** 400+ languages, pure Rust, well-maintained. Used by bat, delta. tree-sitter deferred for future semantic features.

## Auto-Detection Engine
**Decision:** Regex heuristics with conservative thresholds.
**Rules:**
- Fenced code blocks (``` markers) → code_block render
- Unified diff headers (--- a/ +++ b/) → diff render
- JSON object/array at line start ({, [) → json render (require valid parse to confirm)
- Markdown headers (# at line start) → markdown render
- All detection is opt-out per-pane via config
- Require strong signals before switching — prefer false negatives over false positives

## Kitty Graphics Protocol
**Decision:** Include basic raster image support (PNG/JPEG).
**Scope:** Image upload via Kitty protocol, render as wgpu texture quad at grid position. No animation, no virtual placements initially.

## Content Interactivity
**Decision:** Read-only with copy for Phase 4.
- Code blocks: syntax highlighted + copy button (rendered as a small quad)
- JSON: pretty-printed with indentation and colors, no collapse/expand
- Diffs: colored with file headers, no navigation
- Markdown: rendered headings, lists, inline code styling
- Full interactivity (collapse, fold, click) deferred to future phases

## OSC 7770 Protocol
**Format:** As defined in PROJECT.md:
```
ESC ] 7770 ; start ; type=<content_type> [; key=value]* ST
  <content>
ESC ] 7770 ; end ST
```
**Content types (v1):** code_block, diff, plan, markdown, json, error, progress, image
