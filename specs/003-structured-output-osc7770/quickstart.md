# Quickstart: Structured Output via OSC 7770

## Prerequisites

- ArcTerm built from `003-structured-output-osc7770` branch
- No special tools needed — testing uses shell printf commands

## Build

```bash
cargo build --release
cargo test --all
```

## Test Code Block Rendering

```bash
# Emit a Python code block
printf '\033]7770;{"type":"code","language":"python","content":"def fibonacci(n):\n    if n <= 1:\n        return n\n    return fibonacci(n-1) + fibonacci(n-2)\n\nfor i in range(10):\n    print(fibonacci(i))"}\033\\'

# Should render with Python syntax highlighting (keywords, strings, numbers colored)
```

## Test JSON Tree

```bash
# Emit a JSON tree
printf '\033]7770;{"type":"json","content":"{\"name\":\"ArcTerm\",\"version\":\"1.0\",\"features\":{\"plugins\":true,\"ai\":false},\"languages\":[\"rust\",\"lua\"]}"}\033\\'

# Should render as a colored, collapsible tree
```

## Test Diff

```bash
# Emit a unified diff
printf '\033]7770;{"type":"diff","content":"--- a/hello.py\n+++ b/hello.py\n@@ -1,3 +1,3 @@\n def greet():\n-    print(\"hello\")\n+    print(\"hello, world!\")\n     return True"}\033\\'

# Should render with green additions and red deletions
```

## Test Graceful Degradation

```bash
# Test in a non-ArcTerm terminal — should produce no visible output
printf '\033]7770;{"type":"code","language":"python","content":"print(42)"}\033\\'

# Test with unknown type — should render as plain text
printf '\033]7770;{"type":"unknown","content":"some text"}\033\\'

# Test with malformed JSON — should be silently ignored
printf '\033]7770;{invalid json}\033\\'
```

## Test Detection

```bash
if [ "$TERM_PROGRAM" = "ArcTerm" ]; then
    printf '\033]7770;{"type":"code","language":"bash","content":"echo ArcTerm detected!"}\033\\'
else
    echo "Not running in ArcTerm"
fi
```

## Verify Copy-to-Clipboard

1. Render a code block using the command above
2. Select the rendered code block with the mouse
3. Copy to clipboard (Cmd+C / Ctrl+Shift+C)
4. Paste into a text editor — should contain the plain source code, not escape sequences
