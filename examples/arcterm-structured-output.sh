#!/usr/bin/env bash
# ArcTerm Structured Output Examples
# Demonstrates OSC 7770 escape sequences for rich terminal content.
# Run in ArcTerm to see rendered output. In other terminals, nothing appears.

if [ "$TERM_PROGRAM" != "ArcTerm" ]; then
    echo "This script requires ArcTerm for structured output rendering."
    echo "In other terminals, OSC 7770 sequences are silently ignored."
    echo ""
fi

echo "=== ArcTerm Structured Output Demo ==="
echo ""

# Code block with syntax highlighting
echo "1. Syntax-highlighted Python code:"
printf '\033]7770;{"type":"code","language":"python","title":"fibonacci.py","content":"def fibonacci(n):\\n    if n <= 1:\\n        return n\\n    return fibonacci(n-1) + fibonacci(n-2)\\n\\nfor i in range(10):\\n    print(fibonacci(i))"}\033\\'
echo ""

# JSON tree
echo "2. Interactive JSON tree:"
printf '\033]7770;{"type":"json","title":"config.json","content":"{\"name\":\"ArcTerm\",\"version\":\"1.0.0\",\"features\":{\"wasm_plugins\":true,\"ai_pane\":true,\"structured_output\":true},\"languages\":[\"rust\",\"lua\",\"python\"]}"}\033\\'
echo ""

# Colored diff
echo "3. Colored diff:"
printf '\033]7770;{"type":"diff","title":"hello.py","content":"--- a/hello.py\\n+++ b/hello.py\\n@@ -1,3 +1,4 @@\\n def greet(name):\\n-    print(f\\\"Hello, {name}\\\")\\n+    message = f\\\"Hello, {name}!\\\"\\n+    print(message)\\n     return True"}\033\\'
echo ""

echo "=== Demo Complete ==="
