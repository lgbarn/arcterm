# Phase 7 Context — Design Decisions (autonomous defaults)

## AI Tool Detection
**Decision:** Process name matching + OSC 7770 self-identification + heuristic fallback
- Process names: claude, codex, gemini, aider, cursor, copilot, chatgpt
- OSC 7770 handshake: type=ai_agent in structured content start
- Heuristic: detect markdown-heavy output patterns (code blocks, bullet lists)
- Detection cached per-pane with 5s TTL

## Cross-Pane Context API
**Decision:** Per-pane metadata shared via opt-in config
- Always shared: CWD, last command (from shell prompt detection), exit code
- Opt-in: recent output ring buffer (last 100 lines, configurable)
- Context exposed via OSC 7770 type=context response when AI tools query

## MCP Tool Discovery
**Decision:** In-process registry (from Phase 6 plugin system) + OSC 7770 protocol extension
- AI tools query available tools via OSC 7770 type=tool_query
- arcterm responds with OSC 7770 type=tool_list containing JSON schemas
- Tools come from loaded WASM plugins' get_tools() exports

## Plan Status Layer
**Decision:** Ambient status strip + expandable view
- Thin bar (1 row height) at bottom of window showing phase/task progress
- Watches .shipyard/STATE.json + PLAN.md files via notify
- Leader+p toggles expanded plan view (rendered as overlay pane)
- Keyboard navigable: j/k to move, Enter to expand, q to close

## AI Pane Tracking
**Decision:** Leader+a jumps to most recently active AI pane
- Track last_ai_pane_id on AppState
- Updated whenever an AI-detected pane receives focus or PTY output

## Error Bridging
**Decision:** Detect build failures via exit code + output patterns
- Non-zero exit code in a shell pane triggers error context capture
- Captured: last command, exit code, last 20 lines of output
- Made available to AI panes via the cross-pane context API
