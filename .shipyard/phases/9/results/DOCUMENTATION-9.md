# Documentation Report
**Phase:** 9 — Stabilization (bugfixes)
**Date:** 2026-03-15

## Summary

- API/Code docs: 4 public interfaces documented or updated in source
- Architecture updates: none required (no structural changes)
- User-facing docs: 1 example README needs an update; 1 gap flagged for Phase 10

No `docs/` directory exists in this project. All documentation lives in source-level
doc comments and the `examples/` READMEs. Recommendations below are scoped accordingly.

---

## API Documentation

### `Grid` — `arcterm-core/src/grid.rs`

**Public interfaces added/changed:** 3

#### `scroll_offset: usize` — now private
The field was `pub scroll_offset: usize`. It is now private. Two public accessor methods
replace direct field access.

**Documentation status:** adequate — existing doc comment on the field (`/// How many rows
above the current screen bottom the viewport is scrolled. 0 = live view; >0 = scrolled back
into scrollback history.`) is preserved. The new methods carry their own doc comments
(added in the same commit).

#### `pub fn set_scroll_offset(&mut self, offset: usize)`
Doc comment present: `/// Set the scroll offset, clamping to the current scrollback length.`
Adequate for this method's contract.

#### `pub fn scroll_offset(&self) -> usize`
Doc comment present: `/// Current scroll offset (0 = live view).`
Adequate.

#### `pub fn set_scroll_region(&mut self, top: usize, bottom: usize)`
Doc comment updated: `/// Silently rejects invalid bounds: top >= rows, bottom >= rows, or top >= bottom.`
The rejection behavior is now documented inline. Adequate.

**Recommendation:** The "silently rejects" wording in `set_scroll_region` is technically
accurate but could mislead callers who expect a `Result`. Consider adding: `// Returns
without modifying state on invalid input; no error is returned.` This is a minor
suggestion — the existing comment is not wrong.

---

### `PluginInstance` — `arcterm-plugin/src/runtime.rs`

**Public interfaces added:** 1

#### `pub fn call_tool_export(&mut self, name: &str, args_json: &str) -> anyhow::Result<String>`
Doc comment present: `/// Dispatch a tool call to the WASM plugin's call-tool export.`

**Recommendation (actionable):** The doc comment is missing the epoch deadline side effect,
which callers need to know: each call resets the store's epoch deadline to 30 seconds. Add
one sentence: `// Resets the store epoch deadline to 3000 epochs (30 s) before dispatch.`
This matters because `call_tool_export`, `call_update`, and `call_render` each reset the
deadline independently — a caller composing multiple calls should understand this.

---

### `PluginManager::call_tool` — `arcterm-plugin/src/manager.rs`

Doc comment updated: old comment described a Phase 7 stub. New comment reads:
`/// Invoke a named tool by dispatching to the WASM plugin that owns it.`
Adequate — the stub language was correctly removed.

---

### `PluginManifest::validate` — `arcterm-plugin/src/manifest.rs`

No doc comment on this method. Three new validation rules were added (path traversal,
absolute paths, backslashes) without updating any doc comment. The method has no existing
doc comment, so there is nothing to become stale. Low priority.

---

## Architecture Updates

No architectural changes. Phase 9 is a bugfix-only phase. The epoch ticker spawned in
`PluginRuntime::new()` is an implementation detail (background OS thread, no public API
surface). No architecture documentation needs updating.

---

## User-Facing Documentation

### `examples/plugins/system-monitor/README.md`

**Issue:** The MCP tool section contains a stale forward reference:

> When the AI layer calls the `get-system-info` tool (after Phase 7 MCP
> JSON-RPC serving is wired up)...

Phase 9 (H-2) implemented real WASM tool dispatch. `call_tool()` in `PluginManager` now
dispatches to the plugin's `call-tool` WIT export rather than returning a stub error.
The "after Phase 7 is wired up" qualifier is no longer accurate.

**Recommended update** to `examples/plugins/system-monitor/README.md`, MCP tool section:

```markdown
## MCP tool: `get-system-info`

When the AI layer calls the `get-system-info` tool via OSC 7770, the host
dispatches it to this plugin's `call-tool` export and returns the result JSON
to the caller. The tool has no required inputs.
```

### `examples/plugins/hello-world/README.md` and `examples/plugins/system-monitor/README.md`

**Gap — `call-tool` export requirement:** The WIT world now requires plugins to export
`call-tool: func(name: string, args-json: string) -> string` (added in Phase 9, commit
`356e203`). Neither example README mentions this export. Plugin authors who implement
the guest-side WIT bindings by hand need to know this export is mandatory.

**Recommended addition** to the Notes/API section of both READMEs:

```markdown
- `call-tool(name, args-json)` — dispatched when the host routes an MCP tool call
  to this plugin. Return a JSON string result. If this plugin owns no tools, return
  `{"error":"tool not found"}`.
```

This is the only user-facing documentation gap introduced by Phase 9.

---

## Gaps

1. **`call-tool` export not documented in plugin example READMEs** — plugin authors
   will hit a link error if they implement the old WIT world (missing export). File:
   `examples/plugins/hello-world/README.md` and `examples/plugins/system-monitor/README.md`.

2. **`arcterm-app` callsite migration not yet documented** — `scroll_offset` is now
   private; `arcterm-app/src/main.rs` has 5 call sites that will break until Phase 10.
   This is tracked in the ROADMAP under Phase 10 and in VERIFICATION-9. No action needed
   here — document the migration in the Phase 10 documentation pass once the callsites
   are updated.

---

## Recommendations

| Priority | File | Action |
|---|---|---|
| Medium | `examples/plugins/system-monitor/README.md` | Remove stale "after Phase 7 is wired up" qualifier in MCP tool section |
| Medium | `examples/plugins/hello-world/README.md` | Add `call-tool` export to the API surface list |
| Medium | `examples/plugins/system-monitor/README.md` | Add `call-tool` export to the API surface table |
| Low | `arcterm-plugin/src/runtime.rs` line ~122 | Add epoch deadline note to `call_tool_export` doc comment |
| Deferred | `arcterm-app` migration guide | Document `scroll_offset` → `set_scroll_offset()`/`scroll_offset()` in Phase 10 pass |
