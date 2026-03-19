# Quickstart: Warp-Style AI UX

## Prerequisites

- ArcTerm built from `006-warp-style-ai-ux` branch
- Ollama running with `qwen2.5-coder:7b`

## Test Compact Command Panel

1. Start ArcTerm
2. Press `Ctrl+`` (backtick) — panel appears at bottom
3. Type: "find all .rs files modified today"
4. Verify: command appears with explanation, terminal content visible above
5. Press Enter (Run) — command executes in terminal
6. Verify: panel closes after execution

## Test Copy

1. Press `Ctrl+``
2. Type: "list docker containers"
3. Press `Ctrl+C` — command copied to clipboard
4. Verify: panel stays open, paste elsewhere to confirm

## Test Destructive Warning

1. Press `Ctrl+``
2. Type: "delete all files in current directory"
3. Verify: warning badge appears on the command
4. Press Enter — confirmation step required before execution

## Test Agent Mode

1. At the shell prompt, type: `# check disk usage and clean up temp files`
2. Verify: ArcTerm intercepts (does not send `#...` to shell)
3. Verify: multi-step plan appears (e.g., `du -sh /tmp`, `rm -rf /tmp/old-*`)
4. Press Enter to execute step 1, see output
5. Press Enter to execute step 2
6. Press `q` to abort remaining steps

## Test Agent Skip and Abort

1. Type: `# update all packages and restart services`
2. On step 1, press `s` to skip
3. Verify: skipped, moves to step 2
4. Press `q` to abort entire plan

## Test # Escape

1. Type `##` at the prompt
2. Verify: a single `#` is sent to the shell (not intercepted as agent mode)

## Test Inside Programs

1. Open `vim` or `htop`
2. Type `#` — verify it's sent to the program normally (no interception)
