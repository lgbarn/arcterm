# Phase 1 Context — Design Decisions

## Decisions Captured During Brainstorming (2026-03-19)

### Escape Injection Fix
- **Approach:** Inline termwiz parser in arcterm-ai (option A over exporting strip-ansi-escapes lib)
- **Rationale:** Simpler, keeps sanitization close to usage, zero new dependencies

### Consent Gate
- **Approach:** Config flag only (option A over interactive prompt or both)
- **Rationale:** Matches WezTerm's pattern for sensitive config (e.g., OSC 52 clipboard). Explicit opt-in in arcterm.lua is sufficient.

### Lua AI API Scope
- **Approach:** Config struct + new lua-api-crates/ai-funcs (option B over config-only)
- **Rationale:** AI-native terminal should expose AI to Lua plugins. Enables keybinding-driven AI queries and custom status bar widgets.
- **Functions:** `is_available()`, `query(prompt, opts)`, `get_config()`
- **Constraints:** No streaming, no conversation history, no pane manipulation
