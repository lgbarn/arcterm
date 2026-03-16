# Research: Phase 7 — AI Integration Layer

## Context

Arcterm is a GPU-rendered Rust terminal emulator (wgpu + winit + tokio, Rust 2024 edition)
that has completed Phases 1 through 6. The existing codebase provides:

- **Pane tracking**: `AppState.panes: HashMap<PaneId, Terminal>`, one `Terminal` per pane.
  Each `Terminal` exposes `child_pid() -> Option<u32>` and `cwd() -> Option<PathBuf>`.
  No per-pane AI-type flag, last-command, or exit-code field exists yet.
- **Process detection pattern**: `arcterm-app/src/neovim.rs` implements
  `process_comm(pid)` (macOS via `libc::proc_name`; Linux via `/proc/<pid>/comm`) and
  `process_args(pid)` (macOS via `KERN_PROCARGS2` sysctl; Linux via `/proc/<pid>/cmdline`),
  with a 2-second cache (`NeovimState` + `CACHE_TTL`). This is the exact pattern to reuse
  for AI tool detection.
- **Keymap dispatch**: `arcterm-app/src/keymap.rs` — `KeymapHandler` is a two-state machine
  (Normal / LeaderPending). Adding a new leader action requires: (a) a new `KeyAction` enum
  variant, (b) a match arm in the `LeaderPending` branch of `handle_logical_key_with_time`,
  (c) a handler in `AppState`'s `WindowEvent::KeyboardInput` dispatch. Actions `a` and `p`
  are not yet assigned.
- **OSC 7770 protocol**: `arcterm-vt/src/handler.rs` defines `ContentType`, `Handler` trait
  methods `structured_content_start` / `structured_content_end`, and a
  `StructuredContentAccumulator`. A query/response subprotocol for OSC 7770 does NOT yet
  exist — the current implementation is unidirectional (terminal writes structured output;
  no handshake).
- **Plugin MCP registry**: `arcterm-plugin/src/manager.rs` exposes
  `list_tools() -> Vec<ToolSchema>`. `ToolSchema` is a WIT-generated type carrying `name`
  and `description` fields (from Phase 6 `host.rs`). The registry is populated by plugins
  calling the host `register_tool` function during their `init()` export. Phase 7 must
  expose this registry to AI panes.
- **Renderer**: `arcterm-render/src/renderer.rs` — `render_multipane` accepts
  `overlay_quads: &[OverlayQuad]` and `overlay_text: &[(String, f32, f32)]`. These two
  parameters are the correct hooks for a status strip: render a fixed-height `OverlayQuad`
  at the bottom of the window and place text within it via `overlay_text`. No additional
  renderer changes are needed for the ambient strip; the expanded plan view would use the
  existing command-palette overlay pattern (`PaletteState`).
- **File watching**: `notify = "8"` is already declared in `arcterm-app/Cargo.toml`.
  `arcterm-app/src/config.rs` uses `notify::recommended_watcher` + `std::sync::mpsc`
  channel for config hot-reload. The identical pattern applies to plan-file watching.
- **Auto-detection engine**: `arcterm-app/src/detect.rs` — `AutoDetector` scans grid rows
  for structured content patterns. It is per-pane and already integrated into `AppState`.

Phase 7 scope: AI tool detection, cross-pane context API, MCP tool discovery via OSC 7770,
plan status layer, Leader+a pane jump, and error bridging.

---

## Topic 1: AI Tool Detection

### Comparison Matrix

| Criteria | Process name matching | CLAUDECODE env var (env-based) | OSC 7770 self-identification handshake |
|---|---|---|---|
| Implementation basis | Reuse `process_comm()` from `neovim.rs` | Read child env via `/proc/<pid>/environ` or sysctl on macOS | New OSC 7770 subtype; arcterm writes query, tool writes response |
| Reliability | High for known binaries; fragile if binary renamed | High — Claude Code sets `CLAUDECODE=1` in child shells it spawns | High, but requires tool cooperation; tools that are unaware produce no response |
| Coverage | `claude`, `codex`, `aider`, `gemini` (npm binary) | Claude Code only (currently the only tool that sets a known marker) | Future-facing; no current AI tool emits this unprompted |
| Latency | Identical to Neovim detection — one syscall per 2s TTL | One `/proc/<pid>/environ` read per 2s TTL | Async: arcterm sends query after pane start; tool may not respond for seconds |
| False positive rate | Low — process names are distinctive | Very low — env var is unambiguous | Zero if implemented (tool explicitly self-declares) |
| Privacy implications | No output read; only process name | No output read; only env var | No output read; only protocol handshake |
| Platform support | macOS + Linux (same as Neovim detection) | Linux (proc fs). macOS: `libproc` for env is more complex | Universal — OSC sequences work on all platforms |
| Code reuse | Direct — `process_comm()` is ready | New helper needed; `/proc/<pid>/environ` is Linux-only | Requires new OSC 7770 variant + response accumulator |

### Known AI CLI Binary Names (as of March 2026)

| Tool | Binary / process name | Self-identification env var |
|---|---|---|
| Claude Code | `claude` | `CLAUDECODE=1` set in spawned child processes |
| Codex CLI (OpenAI) | `codex` | None documented |
| Gemini CLI (Google) | `gemini` (npm global) | `GEMINI_CLI=1` set in subprocesses |
| Aider | `aider` (Python entry point) | None documented |
| Continue.dev CLI | `continue` | None documented |

Claude Code sets `CLAUDECODE=1` in the environment of shell processes it spawns; this does
NOT mean the `claude` process itself runs with that variable. Detecting the `claude` process
requires process name matching.

### Recommended Detection Strategy (Three Tiers)

**Tier 1 — Process name (immediate, always-on):**
Extend `NeovimState` pattern into a new `AiAgentState` struct. On each `PaneOpened` event
and on `NavigatePane`, call `process_comm(pid)` and check against the known-name list.
Cache result for 2 seconds. This fires immediately when the user starts an AI tool.

**Tier 2 — Environment variable (supplementary, Linux-primary):**
After process name detection fails or is ambiguous, read `/proc/<pid>/environ` on Linux
to check for `CLAUDECODE=1` or `GEMINI_CLI=1`. On macOS, skip this tier — reading another
process's environment requires elevated privileges not available to a normal application.
Mark this tier as Linux-only and fail gracefully.

**Tier 3 — OSC 7770 self-identification (future / cooperative):**
Define a new OSC 7770 subcommand: `ESC ] 7770 ; identify ST`. When arcterm sends this
to the PTY, a cooperating AI tool responds with
`ESC ] 7770 ; start ; type=identify ; tool=<name> ; version=<ver> ST ... ESC ] 7770 ; end ST`.
This tier is opt-in and provides the richest data. No current AI tool supports it; it is
a protocol extension designed for future adoption. Do not block Phase 7 ship on this tier.

---

## Topic 2: Cross-Pane Context API

### Current State

`Terminal.cwd()` delegates to `PtySession.cwd()`, which reads the shell CWD from OS-level
proc introspection. No `last_command`, `last_exit_code`, or `output_ring_buffer` fields
exist anywhere in `AppState` or `Terminal`.

### Comparison Matrix

| Criteria | Per-pane metadata in AppState | Embed in Terminal struct | Separate ContextStore |
|---|---|---|---|
| Coupling | Low — AppState already owns per-pane HashMaps | Medium — Terminal grows non-rendering concerns | Low — extra struct/module |
| Access pattern | Direct HashMap lookup by PaneId — consistent with existing `auto_detectors`, `structured_blocks` | Requires `&mut terminal` borrow | Requires separate borrow or Arc<Mutex<>> |
| Consistency with existing code | Matches the established pattern (nvim_states, auto_detectors, structured_blocks — all `HashMap<PaneId, T>`) | Diverges from existing pattern | Adds indirection without clear benefit |
| Implementation cost | Low | Low | Medium |
| Extensibility | Add new per-pane fields by adding new HashMap entries | Requires Terminal struct change | Flexible but over-engineered for Phase 7 |

**Recommendation: per-pane metadata in AppState as `HashMap<PaneId, PaneContext>`.**

### PaneContext Shape

A new `PaneContext` struct (in a new `arcterm-app/src/context.rs`) should hold:

- `ai_type: Option<AiAgentKind>` — detected AI tool variant or `None` for shell panes.
- `last_command: Option<String>` — last shell command (populated from OSC 7 shell
  integration sequences, if the shell emits them; otherwise `None`).
- `last_exit_code: Option<i32>` — last command exit code (from OSC 7 / shell integration).
- `output_ring: VecDeque<String>` — a fixed-capacity ring buffer of recent output lines.
  Capacity should be configurable; default 200 lines. Only enabled for panes where
  `context_from = ["all"]` is set in the workspace TOML (opt-in, per the Phase 7 privacy
  design rule in ROADMAP.md).

### Shell Integration for last_command and exit_code

No shell sets `last_command` automatically in a way arcterm can intercept without
cooperation. OSC 7 (`OSC 7 ; file://host/path ST`) is already used for CWD reporting
by bash/zsh with `PROMPT_COMMAND`. OSC 133 (FinalTerm / iTerm2 shell integration marks)
provides prompt start (`OSC 133 ; A`), command start (`OSC 133 ; B`), command end / exit
code (`OSC 133 ; D ; exit_code`). This is the correct mechanism for `last_command` and
`last_exit_code`.

OSC 133 is supported by bash (via `bash_completion` integration scripts), zsh (via
`precmd`/`preexec` hooks), and fish natively. The arcterm VT parser in
`arcterm-vt/src/handler.rs` does not yet handle OSC 133. Adding it requires a new
`Handler` trait method and a processor arm in the OSC dispatch. This is the preferred
approach over polling `/proc/<pid>/status`.

**Fallback for users without shell integration:** `last_command = None`,
`last_exit_code = None`. The CWD is always available via `Terminal.cwd()`.

---

## Topic 3: MCP Tool Discovery via OSC 7770

### MCP Protocol Specification (as of November 2025)

The MCP specification (version 2025-11-25) defines JSON-RPC 2.0 messages:

**tools/list request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} }
```

**tools/list response:**
```json
{
  "jsonrpc": "2.0", "id": 1,
  "result": {
    "tools": [
      {
        "name": "get_pods",
        "title": "List Pods",
        "description": "List pods in namespace",
        "inputSchema": { "type": "object", "properties": { "namespace": { "type": "string" } } }
      }
    ]
  }
}
```

**tools/call request:**
```json
{
  "jsonrpc": "2.0", "id": 2, "method": "tools/call",
  "params": { "name": "get_pods", "arguments": { "namespace": "default" } }
}
```

ToolSchema fields: `name` (required, 1-128 chars), `title` (optional human label),
`description` (required), `inputSchema` (JSON Schema 2020-12, required).

### Integration Path in Arcterm

Phase 6 already populates `PluginManager.list_tools() -> Vec<ToolSchema>` where
`ToolSchema` carries `name` and `description` (WIT-generated). `inputSchema` is not yet in
the WIT type — this needs to be added.

The question is how an AI pane running in arcterm can discover these tools. Three options:

| Criteria | OSC 7770 query/response | stdio JSON-RPC server in arcterm | File-based manifest drop |
|---|---|---|---|
| Architectural fit | Native — extends existing OSC 7770 protocol | Arcterm becomes a server process; complex lifecycle | Simple but polling-based; no change notification |
| Latency | Near-zero — response written to PTY immediately | Unix socket connect overhead | File stat on each query |
| Tool cooperation required | Yes — AI tool must query via OSC 7770 | Yes — AI tool must connect to socket | No — AI tool reads file passively |
| Implementation complexity | Medium — new OSC 7770 subtype + response serialization | High — async server, socket lifecycle, per-pane routing | Low — just write JSON to disk |
| Security | PTY-scoped — only the AI in that pane gets the response | Socket is process-wide; auth needed | World-readable if in /tmp |
| Change notification | Possible via push OSC 7770 | MCP `tools/list_changed` notification | File modification requires polling |
| Alignment with arcterm OSC 7770 design | Direct | Contradicts "no cloud service" identity; is a server | Workable stopgap only |

**Recommendation: OSC 7770 query/response for tool discovery.**

Define two new OSC 7770 subtypes:

1. **Query (AI tool → arcterm):**
   `ESC ] 7770 ; tools/list ST` — the AI tool writes this to its stdout; arcterm intercepts
   it in the VT processor and does not render it.

2. **Response (arcterm → AI tool's PTY input):**
   arcterm serializes `PluginManager.list_tools()` as a JSON array and injects it back into
   the PTY as:
   `ESC ] 7770 ; tools/response ; <base64(json)> ST`
   The AI tool's arcterm SDK (future) decodes this; until an SDK exists, document the
   protocol for manual integration.

3. **Tool invocation (AI tool → arcterm → plugin):**
   `ESC ] 7770 ; tools/call ; name=<tool_name> ; args=<base64(json_args)> ST`
   Arcterm routes the call to `PluginManager`, invokes the WASM tool, and returns the
   result via:
   `ESC ] 7770 ; tools/result ; id=<call_id> ; result=<base64(json)> ST`

This design keeps the AI tool unaware of arcterm internals beyond the OSC 7770 protocol.
The existing `Handler` trait and `StructuredContentAccumulator` need new arms for these
subtypes; no new crates are required.

**Important constraint from ROADMAP.md:** "implement the core tool discovery/invocation
pattern, not the full MCP spec." Full MCP (capability negotiation, pagination, server
lifecycle) is out of scope for Phase 7.

---

## Topic 4: Plan Status Layer

### Plan File Discovery

Per PROJECT.md: plan files are in `.shipyard/`, `PLAN.md`, or `TODO.md`. The workspace
TOML can declare custom paths. The status strip must detect these without requiring explicit
user configuration.

Detection order:
1. `<workspace_root>/.shipyard/` directory with any `PLAN-*.md` or `STATE.json` file.
2. `<workspace_root>/PLAN.md`
3. `<workspace_root>/TODO.md`

`workspace_root` is the CWD of the focused pane when no explicit workspace is loaded, or
the `directory` field from the workspace TOML when one is active.

### File Watching

`notify = "8"` is already in `arcterm-app/Cargo.toml` and `config.rs` uses
`notify::recommended_watcher` with a `std::sync::mpsc::Sender<notify::Result<notify::Event>>`.
The identical pattern applies to plan-file watching.

The `notify` crate v8.2.0 (latest stable, August 2025; v9.0.0-rc.1 released January 2026):
- Uses `recommended_watcher()` to select the optimal backend per platform
  (FSEvents on macOS, inotify on Linux, `ReadDirectoryChangesW` on Windows).
- Does NOT include built-in debouncing. The companion crates `notify-debouncer-mini` and
  `notify-debouncer-full` provide debouncing. For plan files, debouncing is unnecessary —
  plan files are written infrequently (on task completion). Raw events are fine.
- The `std::sync::mpsc::Sender<notify::Result<notify::Event>>` API is the simplest
  integration and already established in `config.rs`.

### Plan State Parsing

The strip needs only summary data: phase number, task number, total tasks, percent complete.
This can be extracted from `.shipyard/STATE.json` (if present) or from headings and
checkbox counts in `PLAN.md`/`TODO.md` via a simple regex pass — not a full markdown parser.

`pulldown-cmark = "0.12"` is already in the workspace dependencies (used by Phase 4
structured output); it can parse `PLAN.md` for `[x]` and `[ ]` checkbox counts.

### Rendering: Ambient Strip

The status strip is one row of text at the bottom of the window. The existing
`render_multipane` signature already accepts `overlay_quads` and `overlay_text`. The strip
requires:

1. Subtract strip height from each pane's `rect` before building `PaneRenderInfo`. The
   strip height is `tab_bar_height(cell_size, scale_factor)` — reuse the same calculation
   already used for the tab bar.
2. Push one `OverlayQuad` covering the strip area.
3. Push strip label text into `overlay_text`.

No new GPU pipeline is needed. This is the lowest-risk rendering approach given the
existing infrastructure.

### Rendering: Expanded Plan View (Leader+p)

The expanded plan view uses the same overlay mechanism as the command palette
(`PaletteState` in `palette.rs`). A new `PlanViewState` struct follows the identical
pattern: `query` (for filtering), `entries` (phase/task list), `filtered`, `selected`.
Keyboard navigation (`j`/`k`, `Enter`, `q`, `Escape`) follows the existing palette model.

---

## Topic 5: Leader+a and Leader+p Keybindings

### Current State

`keymap.rs` `KeymapHandler` dispatches leader sequences in the `LeaderPending` branch of
`handle_logical_key_with_time`. The following characters are already assigned:
`n` (split H), `v` (split V), `q` (close pane), `z` (zoom), `t` (new tab),
`w` (workspace switcher), `s` (save workspace), `W` (close tab), `1`-`9` (switch tab),
arrow keys (resize). The characters `a` and `p` are free.

### Adding Leader+a (Jump to AI Pane)

1. Add `JumpToAiPane` to `KeyAction` enum.
2. Add `"a" => Some(KeyAction::JumpToAiPane)` in the leader match arm.
3. In the `AppState` keyboard handler, scan `panes` for the most recently active pane with
   `AiAgentKind != None`, then call `set_focused_pane(id)`.

"Most recently active" requires tracking a `last_ai_pane: Option<PaneId>` in `AppState`,
updated whenever an AI pane receives focus or output. This is a single field addition.

### Adding Leader+p (Toggle Plan View)

1. Add `TogglePlanView` to `KeyAction` enum.
2. Add `"p" => Some(KeyAction::TogglePlanView)` in the leader match arm.
3. In `AppState`, toggle `plan_view_state: Option<PlanViewState>` — `None` = strip only,
   `Some(_)` = expanded overlay visible.

Pattern is identical to `workspace_switcher: Option<WorkspaceSwitcherState>` added in
Phase 5.

---

## Topic 6: Error Bridging (Build Failure → AI Context)

### Detection

Build failures produce non-zero exit codes and output matching patterns like
`error[E...]`, `error:`, `FAILED`, `make: *** Error`. The `AutoDetector` in `detect.rs`
already has a `ContentType::Error` variant but does not yet implement detection logic for
it (the `detect_all` detector array omits `Error`).

Two integration points exist:

1. **Exit code from OSC 133 D**: When the shell emits `OSC 133 ; D ; 1` (exit code 1),
   the pane metadata `last_exit_code` is set. This signals that the previous command failed.
2. **Output pattern from AutoDetector**: After setting `last_exit_code != 0`,
   the ring buffer contents are scanned for error patterns to extract a structured
   `ErrorContext` (file, line, message fields).

### Error Context Injection

When an AI pane is focused (Leader+a or manual), and a sibling pane has
`last_exit_code != Some(0)`, arcterm can format the sibling's `PaneContext` as a
structured context block and write it to the AI pane's PTY input:

```
ESC ] 7770 ; start ; type=error ; source=pane-2 ; exit_code=1 ST
error[E0308]: mismatched types
  --> src/main.rs:42:5
ESC ] 7770 ; end ST
```

This uses the existing OSC 7770 infrastructure. The AI tool receives it as a structured
`error` content block, which it already knows how to interpret (Phase 4 defined the `Error`
content type in the `Handler` trait).

**Privacy constraint (from ROADMAP.md):** Output sharing is opt-in. Default: share only
metadata (CWD, exit code, process name). Sharing the actual error output requires
`context_from = ["all"]` in the workspace TOML or a future per-session permission prompt.

---

## Comparison Matrix: notify Crate (file watching for plan layer)

| Criteria | notify v8 (raw) | notify-debouncer-mini | notify-debouncer-full |
|---|---|---|---|
| Already in Cargo.toml | Yes (notify = "8") | No | No |
| Debouncing | None built-in | Lightweight, single timeout | Full: coalesces events, configurable |
| API complexity | Low — same as config.rs pattern | Medium — additional wrapper | High — complex config |
| Suitable for plan files | Yes — plan files change rarely; no debounce needed | Overkill | Overkill |
| License | MIT/Apache-2.0 | MIT/Apache-2.0 | MIT/Apache-2.0 |
| Maintenance | 9.8M recent downloads; actively maintained | Same repo | Same repo |

**Recommendation: use raw `notify v8` without a debouncer**, matching the existing
`config.rs` pattern. Add a 500ms cooldown in the receiver loop (discard events that arrive
within 500ms of the last processed event) if rapid re-rendering becomes a problem.

---

## Recommendation Summary

| Area | Recommendation |
|---|---|
| AI detection | Three-tier: process name (Tier 1, immediate, reuse `neovim.rs` helpers) + env var (Tier 2, Linux only, supplementary) + OSC 7770 handshake (Tier 3, future/cooperative) |
| Context API | `HashMap<PaneId, PaneContext>` in `AppState`; new `context.rs` module; OSC 133 for last_command + exit_code |
| MCP tool discovery | OSC 7770 query/response subtypes (`tools/list`, `tools/response`, `tools/call`, `tools/result`); serialize `PluginManager.list_tools()` as base64-JSON |
| Plan status strip | `OverlayQuad` + `overlay_text` at bottom of window; reuse `tab_bar_height`; file-watched via existing `notify` pattern |
| Plan expanded view | New `PlanViewState` following `PaletteState` pattern; Leader+p toggles `Option<PlanViewState>` on `AppState` |
| Leader+a | New `KeyAction::JumpToAiPane`; `last_ai_pane: Option<PaneId>` tracked in `AppState` |
| Error bridging | OSC 133 exit-code detection → `last_exit_code` in `PaneContext`; AI pane injection via OSC 7770 `type=error` block; opt-in output sharing |
| File watching | `notify v8` (already in Cargo.toml); no new crate; same `std::sync::mpsc` pattern as `config.rs` |

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| No AI tool ships OSC 7770 support; Tier 3 detection is never triggered | High | Low | Tier 1 (process name) is primary. Tier 3 is future-facing; document it but don't block Phase 7 on it. |
| `process_comm()` returns truncated names on macOS (MAXCOMLEN = 16 chars) | Medium | Medium | The binary names `claude`, `codex`, `aider`, `gemini` are all ≤10 chars; truncation is not an issue for these. Add a comment noting the limit. |
| OSC 133 shell integration requires user shell configuration | High | Medium | Accept this limitation explicitly. Document the required shell hooks. Provide a sensible degraded experience: CWD always works; command/exit-code are `None` without shell integration. |
| plan file parsing breaks on non-shipyard PLAN.md formats | Medium | Low | Parse only what arcterm controls (`.shipyard/STATE.json`). For external `PLAN.md`, extract only `[x]` and `[ ]` checkbox counts — trivially robust. |
| MCP `inputSchema` not present in current `ToolSchema` WIT type | High (it's absent) | Medium | Phase 7 implementation must add `input_schema: Option<String>` (JSON string) to the WIT interface in `arcterm-plugin`. This is a minor WIT change but is a breaking change to plugin API v0. |
| Context injection into AI pane's PTY input races with user typing | Low | High | Write context only when explicitly triggered (Leader+a or a dedicated context-share command). Never inject automatically without user initiation. |
| notify v9.0.0-rc.1 API changes (released Jan 2026) | Medium | Low | Pin to `notify = "8"` explicitly in Cargo.toml (already the case). Do not upgrade to v9 until it reaches stable. |
| Output ring buffer memory growth for high-throughput panes | Medium | Medium | Default cap at 200 lines (~20KB). Make cap configurable in `config.toml`. Only enable ring buffer when `context_from` is set in workspace TOML. |

---

## Implementation Considerations

### Integration Points with Existing Code

- **`arcterm-app/src/neovim.rs`**: `process_comm()` and `process_args()` are private to
  that module. Extract them to a new `arcterm-app/src/proc.rs` shared utility module. Both
  `neovim.rs` (Neovim detection) and the new AI detection module will import from it.
- **`arcterm-app/src/main.rs` `AppState`**: Add fields:
  `pane_contexts: HashMap<PaneId, PaneContext>`,
  `last_ai_pane: Option<PaneId>`,
  `plan_state: Option<PlanStripState>` (the parsed summary, updated on file change),
  `plan_view: Option<PlanViewState>` (the expanded overlay, toggled by Leader+p),
  `plan_watcher: Option<notify::RecommendedWatcher>` (kept alive for the watcher lifetime).
- **`arcterm-vt/src/handler.rs` `Handler` trait**: Add OSC 133 methods
  (`shell_prompt_start`, `command_start`, `command_end(exit_code: i32)`) and OSC 7770
  tool-query methods (`tool_list_query`, `tool_call(name, args_json)`). These are default
  no-ops in the trait; `GridState` implements them and calls back to `AppState` via the
  existing event channel or a new callback.
- **`arcterm-render/src/renderer.rs` `render_multipane`**: No signature change needed.
  The status strip is composed before the call by adjusting pane rects and adding one
  `OverlayQuad` + text entries.
- **`arcterm-plugin/src/manager.rs`**: `list_tools()` already exists. Add a `call_tool(name, args_json) -> Result<String>` method for Phase 7 tool invocation routing.

### Testing Strategy

- `process_comm()` / AI detection: unit tests following the existing `neovim.rs` test
  pattern (test against PID 1, which is never an AI tool).
- `PaneContext` ring buffer: unit tests for capacity cap and eviction behavior.
- OSC 7770 tool query/response: integration test using a mock PTY session that writes
  the query sequence and verifies arcterm writes a valid JSON response back.
- Plan file parsing: unit tests with fixture `.shipyard/` directories containing known
  `STATE.json` and `PLAN.md` files.
- `KeyAction::JumpToAiPane` and `KeyAction::TogglePlanView`: follow existing keymap test
  pattern in `keymap.rs` — inject synthetic key events, assert returned `KeyAction`.
- Error bridging: unit test that a `PaneContext` with `last_exit_code = Some(1)` and a
  non-empty ring buffer produces the expected OSC 7770 `type=error` injection string.

### Performance Implications

- AI detection adds one `process_comm()` syscall per pane per 2 seconds (same budget as
  Neovim detection, already measured acceptable).
- Plan file watching: zero overhead when no plan files are present. When present, the
  notify watcher uses native kernel events (zero polling cost).
- Output ring buffer: bounded at 200 lines per pane. At ~100 bytes/line average, this is
  20KB per pane — negligible versus the existing scrollback buffer (10,000+ lines).
- OSC 7770 tool discovery: synchronous JSON serialization of `list_tools()` on query.
  For typical plugin counts (<10 plugins, <50 tools), serialization is microsecond-range;
  no async overhead needed.

---

## Sources

1. Claude Code environment variables: https://code.claude.com/docs/en/env-vars
2. Claude Code `CLAUDECODE=1` env var: https://gist.github.com/unkn0wncode/f87295d055dd0f0e8082358a0b5cc467
3. Codex CLI binary name and overview: https://developers.openai.com/codex/cli
4. Gemini CLI `GEMINI_CLI=1` and configuration: https://geminicli.com/docs/reference/configuration/
5. Aider process name and scripting: https://aider.chat/docs/scripting.html
6. MCP tools specification (2025-11-25): https://modelcontextprotocol.io/specification/2025-11-25/server/tools
7. notify crate v8.2.0 documentation: https://docs.rs/notify/latest/notify/
8. notify crate on crates.io (version, download stats): https://crates.io/crates/notify

---

## Uncertainty Flags

1. **OSC 133 support in arcterm VT parser**: The handler.rs `Handler` trait does not yet
   handle OSC 133 sequences. Whether the existing processor dispatch in `arcterm-vt/src/processor.rs`
   can be extended without a full rewrite of the OSC dispatch table requires reading
   `processor.rs` more closely than this research covers.

2. **ToolSchema WIT type — inputSchema field**: The current WIT-generated `ToolSchema` in
   Phase 6 carries `name` and `description`. Whether `input_schema` (as a JSON string or
   structured type) was included in the Phase 6 WIT file was not confirmed from the source.
   Check `arcterm-plugin/src/host.rs` generated bindings for the full `ToolSchema` field
   list before designing the MCP response serialization.

3. **macOS child process environment reading**: Reading another process's environment on
   macOS via sysctl `KERN_PROCARGS2` is possible (the existing `process_args()` in
   `neovim.rs` does it), but environment variables are stored separately from argv in the
   kernel args layout. Whether `KERN_PROCARGS2` exposes env vars in addition to argv was
   not fully confirmed. This affects whether Tier 2 env-var detection can be made to work
   on macOS.

4. **Gemini CLI binary name**: The npm package `@google/gemini-cli` installs a `gemini`
   binary, but the underlying node process visible in `process_comm()` may appear as
   `node` rather than `gemini`. Testing on a live system is needed to confirm what
   `process_comm(gemini_pid)` returns.

5. **Aider process name**: Aider is a Python tool; `process_comm()` may return `python3`
   or `python` rather than `aider`. The Python entry point sets `sys.argv[0]` to `aider`,
   visible via `process_args()` but not `process_comm()`. Tier 1 detection for aider
   should use `process_args()[0].ends_with("aider")` not `process_comm()`.
