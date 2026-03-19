---
description: "Task list for Structured Output via OSC 7770"
---

# Tasks: Structured Output via OSC 7770

**Input**: Design documents from `/specs/003-structured-output-osc7770/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/osc-7770-protocol.md, quickstart.md

**Tests**: Not explicitly requested. Verification via `cargo test --all` and shell-based manual testing per quickstart.md.

**Organization**: Tasks grouped by user story for independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Project Initialization)

**Purpose**: Create the `arcterm-structured-output` crate with dependencies and add OSC 7770 to the terminal parser.

- [x] T001 Create `arcterm-structured-output/` directory with `Cargo.toml` declaring dependencies on `syntect`, `serde`, `serde_json`, `base64`, `termwiz` (path dep), `log`, `anyhow` — and add to workspace `members` in root `Cargo.toml`
- [x] T002 Create `arcterm-structured-output/src/lib.rs` with module declarations for `payload`, `code`, `json_tree`, `diff`, and `image`
- [x] T003 Verify the new crate compiles with `cargo check --package arcterm-structured-output`

**Checkpoint**: New crate exists and compiles.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Add OSC 7770 parsing to the terminal and implement the payload parser that all content types depend on.

- [x] T004 Add `ArcTermStructuredOutput(String)` variant to the `OperatingSystemCommand` enum in `wezterm-escape-parser/src/osc.rs` — add `"7770"` to the `osc_entries!` macro table so the parser recognizes it
- [x] T005 Implement JSON payload parser in `arcterm-structured-output/src/payload.rs` — parse the JSON payload, extract `type`, `title`, and type-specific fields into a `StructuredBlock` enum with variants `Code`, `Json`, `Diff`, `Image`; validate payload size against configurable limit (default 10MB); handle malformed JSON gracefully (return `None`)
- [x] T006 Implement the public `render()` function in `arcterm-structured-output/src/lib.rs` — accept a payload string, call the parser, dispatch to the appropriate renderer based on block type, return a `Vec<termwiz::escape::Action>` of SGR-colored text actions (or image cells for images)
- [x] T007 Add OSC 7770 handler in `term/src/terminalstate/performer.rs` — in `osc_dispatch()`, match the new `ArcTermStructuredOutput` variant, call `arcterm_structured_output::render()`, and feed the resulting actions back into the terminal state machine
- [x] T008 Add `arcterm-structured-output` as a dependency of the `term` crate in `term/Cargo.toml`
- [x] T009 Verify `cargo check --package term --package arcterm-structured-output` compiles

**Checkpoint**: OSC 7770 sequences are parsed and dispatched to the rendering crate. Payload validation works.

---

## Phase 3: User Story 1 — Syntax-Highlighted Code Blocks (Priority: P1)

**Goal**: Code blocks render with syntax highlighting for 14+ languages.

**Independent Test**: `printf '\033]7770;{"type":"code","language":"python","content":"def hello():\\n    print(\"world\")"}\033\\'` — renders with colored keywords.

- [x] T010 [US1] Implement syntax highlighting renderer in `arcterm-structured-output/src/code.rs` — use `syntect::parsing::SyntaxSet::load_defaults_newlines()` and `syntect::highlighting::ThemeSet::load_defaults()` to initialize; given a language tag and content string, produce a `Vec<Action>` of `Print` actions with SGR color escapes matching the syntax theme
- [x] T011 [US1] Implement title rendering in `arcterm-structured-output/src/lib.rs` — if the payload includes a `title` field, prepend a bold header line (SGR bold + title text + SGR reset + newline) before the content
- [x] T012 [US1] Handle unknown language tags in `arcterm-structured-output/src/code.rs` — if the language is not recognized by syntect, fall back to plain monospace text without highlighting (no error)
- [x] T013 [US1] Handle empty content gracefully — if `content` is empty string, render nothing (not an error)
- [x] T014 [US1] Write unit tests in `arcterm-structured-output/tests/render_tests.rs` — test Python highlighting produces SGR color codes, test unknown language falls back to plain text, test empty content produces no actions, test title rendering
- [x] T015 [US1] Verify `cargo test --package arcterm-structured-output` passes

**Checkpoint**: Code blocks render with syntax highlighting. 14+ languages supported via syntect defaults. Unknown languages degrade to plain text.

---

## Phase 4: User Story 2 — Collapsible JSON Trees (Priority: P2)

**Goal**: JSON content renders as a colored tree with collapse markers.

**Independent Test**: Emit a nested JSON object via OSC 7770 — renders with colored keys/values and indentation.

- [x] T016 [US2] Implement JSON tree renderer in `arcterm-structured-output/src/json_tree.rs` — parse the JSON content string with `serde_json::Value`, walk the tree recursively, emit SGR-colored output: keys in cyan, strings in green, numbers in yellow, booleans in magenta, null in red, braces/brackets in white
- [x] T017 [US2] Implement indentation and nesting in `arcterm-structured-output/src/json_tree.rs` — each nesting level adds 2 spaces of indentation; arrays and objects show their children on subsequent lines
- [x] T018 [US2] Implement default collapse behavior — top-level keys are expanded, nested objects/arrays beyond depth 1 are rendered collapsed as `{...}` or `[...]` with a `▶` marker prefix
- [x] T019 [US2] Handle invalid JSON content — if `serde_json::from_str` fails, fall back to rendering the raw text content without tree formatting
- [x] T020 [US2] Write unit tests — test nested JSON renders with colors and indentation, test invalid JSON falls back to plain text, test deeply nested objects collapse at depth > 1
- [x] T021 [US2] Verify `cargo test --package arcterm-structured-output` passes

**Checkpoint**: JSON trees render with colors, indentation, and collapse markers. Invalid JSON degrades gracefully.

---

## Phase 5: User Story 3 — Side-by-Side Diffs (Priority: P2)

**Goal**: Unified diffs render with green additions, red deletions, and side-by-side layout.

**Independent Test**: Emit a unified diff via OSC 7770 — renders with colored additions/deletions.

- [x] T022 [US3] Implement unified diff parser in `arcterm-structured-output/src/diff.rs` — parse unified diff format (lines starting with `---`, `+++`, `@@`, `+`, `-`, ` `) into structured hunks with file headers, context lines, additions, and deletions
- [x] T023 [US3] Implement colored diff rendering in `arcterm-structured-output/src/diff.rs` — emit SGR-colored actions: additions in green (SGR 32), deletions in red (SGR 31), hunk headers in cyan (SGR 36), file headers in bold, context lines in default color
- [x] T024 [US3] Handle binary file markers — if the diff contains `Binary files ... differ`, render a label instead of attempting to parse binary content
- [x] T025 [US3] Write unit tests — test addition lines are green, deletion lines are red, hunk headers are cyan, binary markers render cleanly
- [x] T026 [US3] Verify `cargo test --package arcterm-structured-output` passes

**Checkpoint**: Diffs render with appropriate colors. Binary files handled.

---

## Phase 6: User Story 4 — Inline Images (Priority: P3)

**Goal**: Base64-encoded images render inline in the terminal.

**Independent Test**: Emit a small PNG via OSC 7770 — renders as an inline image.

- [x] T027 [US4] Implement image decoder in `arcterm-structured-output/src/image.rs` — decode base64 data, validate format (PNG/JPEG magic bytes), produce image dimensions
- [x] T028 [US4] Implement image-to-terminal-action conversion in `arcterm-structured-output/src/image.rs` — reuse the existing `ITermFileData` / `set_image()` path from `term/src/terminalstate/iterm.rs` by constructing an equivalent `ITermFileData` struct and emitting the same actions the iTerm2 OSC 1337 handler produces
- [x] T029 [US4] Handle corrupted or unsupported image data — if base64 decode fails or image format is unrecognized, render a `[Image: decode error]` placeholder line instead of crashing
- [x] T030 [US4] Write unit tests — test base64 decode, test invalid data produces placeholder, test supported formats accepted
- [x] T031 [US4] Verify `cargo test --package arcterm-structured-output` passes

**Checkpoint**: Images render inline. Corrupted data handled gracefully.

---

## Phase 7: User Story 5 — CLI Tool Integration (Priority: P3)

**Goal**: The protocol is easy to use and documented with working examples.

**Independent Test**: A 10-line shell script emits a code block that renders in ArcTerm and is invisible in other terminals.

- [x] T0032 [US5] Create protocol documentation at `docs/osc-7770-protocol.md` (copy and finalize from `specs/003-structured-output-osc7770/contracts/osc-7770-protocol.md`) — include payload schemas, shell examples for all 4 content types, ArcTerm detection via `TERM_PROGRAM`, and compatibility notes
- [x] T0033 [US5] Create example helper script at `examples/arcterm-structured-output.sh` — a shell script demonstrating how to emit each content type, with ArcTerm detection and plain-text fallback
- [x] T0034 [US5] Verify graceful degradation — test the example script in a non-ArcTerm terminal (e.g., default macOS Terminal.app) and confirm no garbled output appears

**Checkpoint**: Protocol documented. Example script works in ArcTerm and degrades gracefully elsewhere.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Final verification and cleanup.

- [x] T0035 Run full `cargo test --all` to verify all existing tests pass alongside new structured output tests
- [x] T0036 Run `cargo fmt --all` to ensure formatting is clean
- [x] T0037 Run `cargo clippy --package arcterm-structured-output` for lint checks
- [x] T0038 Verify `cargo build --release` succeeds with the new crate
- [x] T0039 Test all four content types end-to-end using quickstart.md commands
- [x] T0040 Verify copy-to-clipboard from structured blocks produces plain text
- [x] T0041 Update `specs/003-structured-output-osc7770/spec.md` status from "Draft" to "Complete"

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup — OSC parser + payload parser
- **User Story 1 (Phase 3)**: Depends on Foundational — needs render dispatch
- **User Story 2 (Phase 4)**: Depends on Foundational — independent of US1
- **User Story 3 (Phase 5)**: Depends on Foundational — independent of US1/US2
- **User Story 4 (Phase 6)**: Depends on Foundational — independent of US1/US2/US3
- **User Story 5 (Phase 7)**: Depends on at least US1 being complete (need something to demo)
- **Polish (Phase 8)**: Depends on ALL user stories being complete

### User Story Dependencies

- **US1 (P1)**: Depends on Foundational. Core MVP.
- **US2 (P2)**: Depends on Foundational only. Can run parallel with US1.
- **US3 (P2)**: Depends on Foundational only. Can run parallel with US1/US2.
- **US4 (P3)**: Depends on Foundational only. Can run parallel with US1/US2/US3.
- **US5 (P3)**: Needs at least US1 working to have a demonstrable feature.

### Parallel Opportunities

- **Phase 1**: T001, T002 are sequential (T002 depends on T001)
- **Phase 2**: T004 and T005 are parallelizable (different files/crates)
- **Phase 3-6**: US1, US2, US3, US4 can ALL run in parallel after Foundational (each is a different file in the new crate)
- **Within each US**: Renderer implementation and unit tests are sequential

---

## Parallel Example: US1-US4 After Foundational

```bash
# All four renderers touch different files — fully parallel:
Task: "T010 [US1] Implement syntax highlighting in code.rs"
Task: "T016 [US2] Implement JSON tree renderer in json_tree.rs"
Task: "T022 [US3] Implement diff parser in diff.rs"
Task: "T027 [US4] Implement image decoder in image.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (crate, deps)
2. Complete Phase 2: Foundational (OSC parser, payload parser, render dispatch)
3. Complete Phase 3: User Story 1 (syntax-highlighted code blocks)
4. **STOP and VALIDATE**: `cargo test --all` green + quickstart code block test
5. Demo: `printf '\033]7770;{"type":"code","language":"rust","content":"fn main() { println!(\"hello\"); }"}\033\\'`

### Incremental Delivery

1. US1 → Code blocks with syntax highlighting (MVP — most common content type)
2. Add US2 → JSON trees (second-most useful for API/config work)
3. Add US3 → Colored diffs (third — enriches git/review workflows)
4. Add US4 → Inline images (fourth — diagrams and charts)
5. Add US5 → Documentation and examples (enables ecosystem adoption)
6. Each story adds a content type without affecting others

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story
- syntect loads default themes/grammars at initialization — this happens once per terminal session, not per block
- The `render()` function returns `Vec<Action>` — the terminal state machine processes these natively, so scrollback, resize, and copy all work automatically
- Commit after each task or logical group
- The JSON tree collapse/expand interactivity (click-to-toggle) is a follow-up UI task for the GUI layer, not part of the initial rendering
