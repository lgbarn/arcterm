# OSC 7770 Protocol Specification

## Overview

OSC 7770 is a custom Operating System Command escape sequence for rendering
structured content in ArcTerm. It enables command-line tools to output rich
content (syntax-highlighted code, JSON trees, diffs, images) that ArcTerm
renders natively.

## Escape Sequence Format

```
ESC ] 7770 ; <JSON payload> ST
```

Where:
- `ESC ]` is the OSC introducer (bytes `0x1B 0x5D`)
- `7770` is the command number
- `;` separates the command number from the payload
- `<JSON payload>` is a UTF-8 encoded JSON object
- `ST` is the String Terminator: either `ESC \` (bytes `0x1B 0x5C`) or `BEL` (byte `0x07`)

## Payload Schema

### Common Fields

| Field   | Type   | Required | Description                                    |
|---------|--------|----------|------------------------------------------------|
| `type`  | string | Yes      | Content type: `"code"`, `"json"`, `"diff"`, `"image"` |
| `title` | string | No       | Optional title displayed above the content     |

### Code Block

| Field      | Type   | Required | Description                        |
|------------|--------|----------|------------------------------------|
| `language` | string | Yes      | Language identifier (e.g., `"python"`, `"rust"`, `"javascript"`) |
| `content`  | string | Yes      | Source code text                   |

Example:
```json
{"type": "code", "language": "python", "title": "example.py", "content": "def hello():\n    print('world')"}
```

### JSON Tree

| Field     | Type   | Required | Description            |
|-----------|--------|----------|------------------------|
| `content` | string | Yes      | JSON string to display |

Example:
```json
{"type": "json", "content": "{\"name\": \"ArcTerm\", \"version\": \"1.0\"}"}
```

### Diff

| Field     | Type   | Required | Description              |
|-----------|--------|----------|--------------------------|
| `content` | string | Yes      | Unified diff format text |

Example:
```json
{"type": "diff", "content": "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old line\n+new line"}
```

### Image

| Field    | Type   | Required | Description                         |
|----------|--------|----------|-------------------------------------|
| `format` | string | Yes      | Image format: `"png"` or `"jpeg"`  |
| `data`   | string | Yes      | Base64-encoded image binary data    |

Example:
```json
{"type": "image", "format": "png", "title": "diagram", "data": "iVBORw0KGgo..."}
```

## Shell Examples

### Emit a code block (Bash)
```bash
printf '\033]7770;{"type":"code","language":"python","content":"def hello():\\n    print(\\\"world\\\")"}\033\\'
```

### Emit a JSON tree (Bash)
```bash
printf '\033]7770;{"type":"json","content":"{\\\"key\\\": \\\"value\\\"}"}\033\\'
```

### Detect ArcTerm support
```bash
if [ "$TERM_PROGRAM" = "ArcTerm" ]; then
    # Emit structured output
    printf '\033]7770;...\033\\'
else
    # Fall back to plain text
    echo "plain text"
fi
```

## Limits

- Maximum payload size: 10MB (configurable in ArcTerm settings)
- Payloads exceeding the limit are silently discarded with a log warning

## Compatibility

- **ArcTerm**: Full rendering of all content types
- **Other terminals**: OSC 7770 is silently ignored per ECMA-48 standard
  (unknown OSC sequences are discarded by conforming terminal emulators)
- **Screen/tmux**: OSC passthrough behavior varies; structured content may
  not reach ArcTerm when running inside a multiplexer

## Versioning

The protocol is versioned implicitly by the `type` field. New content types
can be added in future versions. Unknown types are rendered as plain text.
