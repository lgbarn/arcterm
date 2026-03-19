# Feature Specification: Structured Output via OSC 7770

**Feature Branch**: `003-structured-output-osc7770`
**Created**: 2026-03-19
**Status**: Draft
**Input**: User description: "Structured output rendering via OSC 7770 escape sequence for rich content display in terminal"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Syntax-Highlighted Code Blocks (Priority: P1)

A developer runs a command-line tool (compiler, linter, AI assistant, or documentation viewer) that outputs code snippets. The tool emits an OSC 7770 escape sequence containing a code block with a language tag. ArcTerm renders the code with syntax highlighting — colored keywords, strings, comments, and operators — directly in the terminal output, rather than displaying raw text.

**Why this priority**: Code is the most common structured content developers encounter in terminals. Syntax highlighting transforms walls of monochrome text into readable, scannable code. This is the minimum viable demonstration of structured output and the foundation all other content types build on.

**Independent Test**: Run `echo -e "\033]7770;{\"type\":\"code\",\"language\":\"python\",\"content\":\"def hello():\\n    print('world')\"}\033\\"` in ArcTerm. Verify the output renders with Python syntax colors (keywords in one color, strings in another) rather than displaying the raw escape sequence or plain text.

**Acceptance Scenarios**:

1. **Given** ArcTerm receives an OSC 7770 sequence with `type: "code"` and `language: "python"`, **When** the terminal renders the output, **Then** the code block appears with syntax highlighting appropriate for Python
2. **Given** ArcTerm receives an OSC 7770 code block with an unknown language tag, **When** the terminal renders the output, **Then** the code displays as monospace plain text without errors
3. **Given** a terminal running inside a non-ArcTerm emulator, **When** the same OSC 7770 sequence is sent, **Then** the escape sequence is silently ignored (standard OSC behavior) and the content does not appear as garbled text
4. **Given** an OSC 7770 code block with 500+ lines, **When** the terminal renders it, **Then** the block renders without lag and the user can scroll through it normally

---

### User Story 2 - Collapsible JSON Trees (Priority: P2)

A developer queries an API or runs a tool that returns JSON output. The tool emits an OSC 7770 sequence containing JSON data. ArcTerm renders it as an interactive tree view — keys and values are color-coded, nested objects are collapsible, and the user can expand/collapse sections to navigate large JSON structures without scrolling through hundreds of lines.

**Why this priority**: JSON is ubiquitous in modern development workflows (API responses, config files, log entries). An interactive tree view transforms unreadable flattened JSON into navigable structure. This is the second-highest value content type after code.

**Independent Test**: Emit an OSC 7770 JSON block with a nested object (3+ levels deep). Verify the tree renders with color-coded keys/values and that clicking or pressing a key on a collapsed node expands it.

**Acceptance Scenarios**:

1. **Given** ArcTerm receives an OSC 7770 sequence with `type: "json"`, **When** the terminal renders it, **Then** the JSON displays as a tree with colored keys, string values, numbers, booleans, and null values
2. **Given** a rendered JSON tree with nested objects, **When** the user toggles a node, **Then** the node's children collapse or expand
3. **Given** a JSON tree with more than 100 keys at the top level, **When** the terminal renders it, **Then** only the first level is expanded by default (deeper levels collapsed)
4. **Given** invalid JSON content in an OSC 7770 JSON block, **When** the terminal receives it, **Then** it falls back to displaying the raw text content

---

### User Story 3 - Side-by-Side Diffs (Priority: P2)

A developer runs a diff tool, code review command, or version control operation that produces a diff. The tool emits an OSC 7770 diff block. ArcTerm renders the diff in a side-by-side layout with additions highlighted in green, deletions in red, and change context in neutral tones — similar to a code review interface.

**Why this priority**: Diffs are a core developer workflow (git, code review, deployments). A structured side-by-side view is dramatically more readable than unified diff text. This transforms the terminal from a text-only tool into a genuine development interface.

**Independent Test**: Emit an OSC 7770 diff block with unified diff content. Verify ArcTerm renders a side-by-side view with green additions and red deletions.

**Acceptance Scenarios**:

1. **Given** ArcTerm receives an OSC 7770 sequence with `type: "diff"` containing unified diff format, **When** the terminal renders it, **Then** additions appear highlighted in green, deletions in red, and context lines in neutral color
2. **Given** a diff that spans the full terminal width, **When** the terminal renders side-by-side, **Then** each side gets approximately half the terminal width with horizontal scroll if needed
3. **Given** a diff with binary file markers, **When** the terminal renders it, **Then** binary files are indicated with a "Binary file changed" label rather than attempting to render binary content

---

### User Story 4 - Inline Images (Priority: P3)

A developer uses a tool that generates diagrams, charts, or screenshots and outputs them to the terminal. The tool emits an OSC 7770 image block containing image data. ArcTerm renders the image inline in the terminal output at an appropriate size, fitting within the terminal width.

**Why this priority**: Inline images enable workflows that traditionally required switching to a browser or image viewer (viewing generated diagrams, chart output, test screenshots). Lower priority because image rendering is more complex and less universally needed than code/JSON/diffs.

**Independent Test**: Emit an OSC 7770 image block with a small PNG (base64-encoded). Verify the image renders inline in the terminal output at a reasonable size.

**Acceptance Scenarios**:

1. **Given** ArcTerm receives an OSC 7770 sequence with `type: "image"` and base64-encoded PNG data, **When** the terminal renders it, **Then** the image displays inline scaled to fit within the terminal width
2. **Given** an image wider than the terminal, **When** the terminal renders it, **Then** the image is scaled down proportionally to fit the available width
3. **Given** an OSC 7770 image block with corrupted or unsupported image data, **When** the terminal receives it, **Then** a placeholder is shown with an error message rather than crashing

---

### User Story 5 - CLI Tool Integration (Priority: P3)

A CLI tool developer wants their tool to emit structured output that ArcTerm can render richly. They use a simple protocol: wrap content in an OSC 7770 escape sequence with a JSON header specifying the content type. The protocol is documented, easy to implement in any language, and degrades gracefully in non-ArcTerm terminals (the escape sequence is silently ignored).

**Why this priority**: The value of structured output scales with ecosystem adoption. Making the protocol simple and well-documented encourages tool authors to adopt it. This is lower priority because it's documentation/API design rather than a user-facing feature.

**Independent Test**: Write a 10-line shell script that emits an OSC 7770 code block. Run it in ArcTerm (renders with highlighting) and in a standard terminal (renders nothing or plain text). Verify both behaviors.

**Acceptance Scenarios**:

1. **Given** a tool author reads the OSC 7770 protocol documentation, **When** they implement structured output in a new CLI tool, **Then** they can emit working structured content in under 15 minutes
2. **Given** a tool that emits OSC 7770 sequences, **When** run in a terminal that doesn't support the protocol, **Then** the escape sequences are silently consumed and no garbled output appears
3. **Given** the protocol specification, **When** a tool author checks for ArcTerm support, **Then** they can detect ArcTerm via the `TERM_PROGRAM` environment variable and conditionally emit structured output

---

### Edge Cases

- What happens when an OSC 7770 sequence is truncated (e.g., terminal connection drops mid-sequence)? The partial sequence is discarded and the terminal continues operating normally.
- What happens when an extremely large payload is sent via OSC 7770 (e.g., a 100MB JSON blob)? The terminal enforces a maximum payload size (configurable, default 10MB) and rejects payloads exceeding it with a log warning.
- What happens when structured content is included in terminal scrollback? Structured blocks are stored in scrollback and re-rendered when scrolled into view.
- What happens when the user copies structured content to clipboard? The plain text representation of the content is copied (code text, JSON text, diff text), not the escape sequences.
- What happens when multiple OSC 7770 blocks appear in sequence? Each block is rendered independently in the order received.
- What happens when a structured block is partially off-screen? The block renders fully when any part of it is visible, with normal scrolling behavior.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: ArcTerm MUST parse OSC 7770 escape sequences in the terminal output stream and extract the JSON-encoded payload
- **FR-002**: The OSC 7770 payload MUST contain a `type` field that determines how the content is rendered (`code`, `json`, `diff`, `image`)
- **FR-003**: ArcTerm MUST render `code` blocks with syntax highlighting based on the `language` field in the payload
- **FR-004**: ArcTerm MUST render `json` blocks as interactive tree views with collapsible nodes
- **FR-005**: ArcTerm MUST render `diff` blocks as side-by-side colored views with additions in green and deletions in red
- **FR-006**: ArcTerm MUST render `image` blocks inline, scaled to fit the terminal width
- **FR-007**: Unknown `type` values MUST be rendered as plain text without errors
- **FR-008**: Malformed or invalid payloads MUST be handled gracefully — log a warning, display nothing or fall back to plain text, never crash
- **FR-009**: ArcTerm MUST enforce a configurable maximum payload size (default 10MB) for OSC 7770 sequences
- **FR-010**: Structured content MUST be preserved in terminal scrollback and re-rendered when scrolled into view
- **FR-011**: Copy-to-clipboard from structured content MUST produce the plain text representation (source code, raw JSON, unified diff text)
- **FR-012**: The OSC 7770 protocol MUST degrade gracefully in non-ArcTerm terminals — escape sequences MUST be silently consumed per standard OSC behavior
- **FR-013**: Structured content rendering MUST NOT block the terminal's input or output processing
- **FR-014**: ArcTerm MUST support a `title` field in the OSC 7770 payload for optional display above the structured content block
- **FR-015**: Structured output blocks MUST respect the terminal's current color scheme and font settings

### Key Entities

- **Structured Block**: A unit of rich content parsed from an OSC 7770 sequence, containing a type, optional title, and type-specific content data. Stored in scrollback alongside normal terminal lines.
- **Block Type**: An enum of supported content types (`code`, `json`, `diff`, `image`) that determines which renderer is used for display.
- **OSC 7770 Payload**: The JSON-encoded data between the OSC 7770 introducer and the string terminator. Contains `type`, optional `title`, and type-specific fields (`language`, `content`, `data`).

## Assumptions

- The OSC 7770 escape sequence format follows standard OSC conventions: `ESC ] 7770 ; <JSON payload> ST` where ST is `ESC \` or `BEL`
- The number 7770 is chosen to avoid conflicts with existing OSC sequences (iTerm2 uses 1337, WezTerm uses 52/7/8, etc.)
- Syntax highlighting supports at minimum: Python, JavaScript/TypeScript, Rust, Go, Java, C/C++, Ruby, Shell/Bash, YAML, TOML, JSON, Markdown, HTML/CSS, SQL. Additional languages are a nice-to-have.
- The JSON tree view supports keyboard navigation (arrow keys to move, enter/space to toggle) in addition to mouse interaction
- Diff rendering supports unified diff format as input; other diff formats (context diff, ed diff) are out of scope for the initial implementation
- Image rendering supports PNG and JPEG formats; SVG and other formats are out of scope initially
- The `TERM_PROGRAM=ArcTerm` environment variable (already set by the rebrand) is the standard way for tools to detect ArcTerm support
- Structured blocks occupy terminal lines proportional to their rendered height — a 20-line code block takes 20+ lines of scrollback

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A syntax-highlighted code block renders correctly for at least 14 programming languages
- **SC-002**: A JSON tree with 1000+ keys renders within 500ms and remains interactive (expand/collapse responds within 100ms)
- **SC-003**: Terminal remains responsive (60fps rendering, sub-100ms input latency) while displaying 10+ structured blocks in the visible area
- **SC-004**: The OSC 7770 protocol can be implemented by a tool author in under 15 minutes using the documentation and a 10-line example
- **SC-005**: Structured blocks survive terminal resize — re-rendering adjusts to the new width within one frame
- **SC-006**: All existing ArcTerm tests pass (`cargo test --all` green) with the structured output system integrated
- **SC-007**: Copy-to-clipboard from any structured block produces valid, readable plain text 100% of the time
