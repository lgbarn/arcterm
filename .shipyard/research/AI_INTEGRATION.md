# Research: AI Integration — Pane Context, HTTP, Pane Creation, Overlay Rendering

## Context

ArcTerm is a WezTerm fork. The planned `arcterm-ai` crate must provide two features:

1. **AI pane** — a persistent split pane holding a conversational LLM chat that has read access to sibling pane content (scrollback text, CWD, foreground process).
2. **Command overlay** — a transient, full-width popup that takes a natural-language prompt, calls an LLM one-shot, and returns a shell command string.

The existing stack is: Rust (edition 2018/2021), `smol`/`async-executor` for background work, a custom `SpawnQueue`-based main-thread scheduler (not Tokio), `reqwest 0.12` and `http_req 0.11` already present as workspace dependencies, and `TermWizTerminal` / `TermWizTerminalPane` as the established pattern for non-PTY panes.

---

## Topic 1: Pane Context Reading

### What methods exist on the `Pane` trait

Source file: `/Users/lgbarn/Personal/arcterm/mux/src/pane.rs`

The `Pane` trait (line 167) exposes these methods relevant to context capture:

| Method | Return | Notes |
|--------|--------|-------|
| `get_lines(range)` | `(StableRowIndex, Vec<Line>)` | Raw cell matrix for any scrollback range |
| `get_logical_lines(range)` | `Vec<LogicalLine>` | Wrapped lines reunited into logical lines |
| `get_dimensions()` | `RenderableDimensions` | `physical_top`, `viewport_rows`, `cols` — needed to compute the scrollback range |
| `get_current_working_dir(policy)` | `Option<Url>` | CWD; `CachePolicy::AllowStale` is fast, `FetchImmediate` is accurate |
| `get_foreground_process_name(policy)` | `Option<String>` | Executable path of the foreground process |
| `get_foreground_process_info(policy)` | `Option<LocalProcessInfo>` | Full process tree |
| `get_semantic_zones()` | `Vec<SemanticZone>` | Shell-integrated zones (prompt, input, output) if the shell emits OSC 133 |
| `get_cursor_position()` | `StableCursorPosition` | Current cursor row/col |
| `copy_user_vars()` | `HashMap<String, String>` | User variables set via OSC 1337 |

There is no direct "get last exit code" on the `Pane` trait. Exit code is available only on `LocalPane` after the child process dies, via its `ProcessState::Running { child_waiter }` channel — not accessible from the trait surface. The semantic zone approach (OSC 133) is the practical substitute: zone type `SemanticType::Output` marks output regions; exit code can be read from the zone's exit code annotation if the shell provides it.

### How the Lua API reads pane content

Source file: `/Users/lgbarn/Personal/arcterm/lua-api-crates/mux/src/pane.rs`

`MuxPane` wraps a `PaneId` and resolves to `Arc<dyn Pane>` via `Mux::get_pane()`. The Lua methods map directly to the `Pane` trait:

- `get_lines_as_text(n_lines?)` — calls `pane.get_lines()` and extracts visible cell text, trims trailing whitespace. This is the simplest path to read the last N lines of a sibling pane as a `String`.
- `get_logical_lines_as_text(n_lines?)` — same but re-joins wrapped lines.
- `get_lines_as_escapes(n_lines?)` — returns ANSI escape sequences via `termwiz_funcs::lines_to_escapes`.
- `get_semantic_zones(of_type?)` + `get_text_from_semantic_zone(zone)` — structured zone reading.
- `get_current_working_dir()`, `get_foreground_process_name()`, `get_foreground_process_info()` — process context.
- `inject_output(text)` — writes parsed escape sequences into the pane's terminal model; this is the mechanism to render streamed LLM output into a `TermWizTerminalPane`.

**Decision:** For the AI pane, read sibling pane content via `Pane::get_logical_lines()` + `Pane::get_current_working_dir()` + `Pane::get_foreground_process_name()`. These are all accessible from a `PaneId` resolved through `Mux::get_pane()`. No special privilege is required.

### How pane IDs and focus are tracked

Source file: `/Users/lgbarn/Personal/arcterm/mux/src/lib.rs`

- `Mux` maintains `panes: RwLock<HashMap<PaneId, Arc<dyn Pane>>>` (line 104).
- `Mux::resolve_pane_id(pane_id)` returns `(DomainId, WindowId, TabId)` — the full location of any pane (line 1071).
- `Tab::iter_panes()` returns `Vec<PositionedPane>` — all panes in a tab with their layout coordinates (`left`, `top`, `width`, `height`, `is_active`). This is the sibling enumeration path.
- Focus is tracked per `ClientId` via `Mux::record_focus_for_client()` / `Mux::resolve_focused_pane()`. The `MuxNotification::PaneFocused(PaneId)` event fires when focus changes (line 84).
- `Tab::get_active_pane()` returns the currently active pane within a tab.

**Decision:** To find "the pane the user is working in," subscribe to `MuxNotification::PaneFocused` or query `Tab::get_active_pane()` at request time. For the AI pane to know its sibling, it should store the `PaneId` of the pane it was spawned alongside at creation time.

---

## Topic 2: HTTP Client Options

### Existing HTTP dependencies

The workspace has **two** HTTP client libraries already in use:

| Library | Version | Where Used | Characteristics |
|---------|---------|------------|-----------------|
| `http_req` | 0.11 | `wezterm-gui/src/update.rs` (GitHub release check) | Synchronous, blocking, no async, minimal TLS via OpenSSL, ~40 kB. No streaming. |
| `reqwest` | 0.12 | `sync-color-schemes/` (offline tool) and `wezterm-char-props/codegen/` (build tool) | Async via Tokio runtime, supports streaming via `bytes_stream()`, full-featured. |

Evidence:
- `Cargo.toml` line 120: `http_req = "0.11"`
- `Cargo.toml` line 186: `reqwest = "0.12"`
- `wezterm-gui/Cargo.toml` line 57: `http_req.workspace = true`
- `sync-color-schemes/Cargo.toml` line 18: `reqwest.workspace = true`

**Critical finding:** `reqwest 0.12` is **not** a dependency of `wezterm-gui` or `mux`. It is only used in offline/build tooling. It requires a Tokio runtime. Pulling it into `arcterm-ai` would introduce Tokio as a runtime dependency inside the GUI process.

### Async runtime

Source: `/Users/lgbarn/Personal/arcterm/ARCHITECTURE.md` (observed), `promise/src/spawn.rs`, `wezterm-gui/src/termwindow/mod.rs` line 54.

- The GUI thread uses a **custom `SpawnQueue`** integrated with the platform native event loop (CF RunLoop on macOS, pipe-wakeup on X11/Wayland, Win32 event on Windows). No Tokio event loop runs on the main thread.
- Background threads use **`smol`** (`async-executor`). Evidence: `mux/Cargo.toml` line 41, `Cargo.toml` line 204.
- Tokio (v1.43) is a workspace dependency but is consumed only by `sync-color-schemes` with `rt-multi-thread`. It does not run inside the GUI process during normal operation.
- The overlay machinery (`start_overlay`, `start_overlay_pane`) runs its closure in a dedicated thread via `promise::spawn::spawn_into_new_thread`, which is a plain OS thread, not an async task. That thread can block.

### Streaming HTTP for Ollama (NDJSON streaming)

Ollama's API returns newline-delimited JSON (NDJSON). Each line is a complete JSON object with a `response` field. The client must stream the response body line by line and update the UI incrementally.

**Options evaluated:**

**Option A: `reqwest 0.12` with Tokio**
- Supports `response.bytes_stream()` returning a `Stream<Item=Bytes>`.
- Would require spinning up a `tokio::runtime::Runtime` inside the overlay thread or the `arcterm-ai` crate.
- Tokio is already a workspace dependency (v1.43) but not active in the GUI process.
- Risk: two async executors (smol + tokio) running simultaneously, which is technically safe but architecturally messy and increases binary size.

**Option B: `http_req 0.11` (already in `wezterm-gui`)**
- Synchronous only. No streaming support — it accumulates the full response body before returning.
- Not suitable for NDJSON streaming from Ollama; the response never terminates until the model finishes.

**Option C: `ureq 2.x` (new dependency)**
- Synchronous, blocking HTTP client with streaming via `response.into_reader()` (returns a `Read` impl).
- No runtime required. NDJSON streaming is implemented as a blocking `BufReader::lines()` loop.
- Runs cleanly inside `spawn_into_new_thread` (a plain OS thread), which is exactly the pattern used by overlay closures.
- License: MIT. Version 2.12 (March 2025). ~500 kB compiled, no async dependency.
- Source: https://crates.io/crates/ureq

**Decision: Use `ureq 2.x` for the AI HTTP client.**

Rationale: The overlay machinery already runs its work on a plain OS thread (via `spawn_into_new_thread`). `ureq` integrates with zero runtime overhead — just `BufReader<ureq::Response>` looped over lines. It avoids introducing Tokio into the GUI process. `http_req` is already present but lacks streaming. `reqwest` is the cleanest API but forces a second async runtime into the process.

The `smol` crate has an HTTP extension (`async-h1`, `surf`) but these are not present and would require additional dependencies without benefit over `ureq` for this blocking-thread model.

---

## Topic 3: Pane Creation and Split Mechanics

### How `Mux::split_pane` works

Source: `/Users/lgbarn/Personal/arcterm/mux/src/lib.rs` lines 1187–1246; `/Users/lgbarn/Personal/arcterm/mux/src/domain.rs` lines 74–100.

The public entry point is `Mux::split_pane(pane_id, request, source, domain)`:
1. Resolves the domain from `SpawnTabDomain`.
2. Calls `domain.split_pane(source, tab_id, pane_id, split_request)`.
3. The domain's `split_pane` default implementation calls `tab.split_and_insert(pane_index, split_request, new_pane)`.

`SplitSource` is an enum:
- `SplitSource::Spawn { command, command_dir }` — spawns a new PTY child.
- `SplitSource::MovePane(PaneId)` — moves an existing pane.

`SplitRequest` carries `direction` (Horizontal/Vertical), `target_is_second` (new pane is right/bottom), `top_level` (split the full tab, not just the active pane), and `size` (Cells or Percent).

### KeyAssignment for splits

Source: `/Users/lgbarn/Personal/arcterm/config/src/keyassignment.rs` lines 580–636.

Existing key assignments: `SplitHorizontal(SpawnCommand)`, `SplitVertical(SpawnCommand)`, `SplitPane(SplitPane)`. New AI-specific key assignments would be added to the `KeyAssignment` enum following the same pattern, e.g. `OpenAiPane` and `OpenCommandOverlay`.

### How to create a "virtual" pane (not backed by a PTY)

Source: `/Users/lgbarn/Personal/arcterm/mux/src/termwiztermtab.rs`.

The existing solution is `TermWizTerminalPane`. This is the **established, production-proven path**:

- `TermWizTerminalPane` implements `Pane` without a PTY child. Instead it uses:
  - `input_tx: Sender<InputEvent>` — receives keyboard/mouse from the GUI.
  - `writer: Mutex<Vec<u8>>` — receives `termwiz::surface::Change` objects rendered by a `TerminfoRenderer` into escape sequences, then stored for the mux to read.
  - `render_rx: FileDescriptor` — the other end of a pipe; the mux's `read_from_pane_pty` thread reads from here, allowing the normal rendering pipeline to work.
- `allocate(size, term_config)` creates a `(TermWizTerminal, Arc<TermWizTerminalPane>)` pair.
- The `TermWizTerminal` exposes a `termwiz::Terminal` trait implementation (with `render`, `poll_input`) that the overlay logic uses.

All existing overlays (launcher, selector, prompt, debug, copy/search) use exactly this mechanism. The overlay closure runs in a dedicated thread returned by `spawn_into_new_thread`, receives a `TermWizTerminal`, and drives it with a synchronous event loop.

**Decision for AI pane:** Implement `AiPane` as a new struct implementing the `Pane` trait, using the same `TermWizTerminalPane` infrastructure. Use `allocate()` to get the terminal/pane pair, spawn the AI interaction loop in a new thread (consistent with all existing overlay patterns), and add the resulting pane to the tab via `tab.split_and_insert()` directly (bypassing `SplitSource::Spawn` which requires a PTY command). The AI pane's domain would be `TermWizTerminalDomain`.

**Alternatively:** use `start_overlay` / `start_overlay_pane` from `wezterm-gui/src/overlay/mod.rs` which automates the `allocate` + `spawn_into_new_thread` + `schedule_cancel_overlay` lifecycle. This is the recommended path for the command overlay (transient). For the persistent AI pane (should survive focus changes), a direct split into the tab tree is more appropriate.

---

## Topic 4: Overlay/Popup Rendering

### How WezTerm renders overlays

Source: `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/overlay/mod.rs`; `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/termwindow/mod.rs`.

There are **two overlay scopes**:

| Scope | Function | What it covers | How dismissed |
|-------|----------|----------------|---------------|
| Tab-level overlay | `assign_overlay(tab_id, pane)` | Replaces the entire tab content | `cancel_overlay_for_tab(tab_id, pane_id)` |
| Pane-level overlay | `assign_overlay_for_pane(pane_id, pane)` | Replaces one pane | `cancel_overlay_for_pane(pane_id)` |

`OverlayState` is stored in `TabState` or `PaneState` respectively (both `RefCell<HashMap<_, _>>` inside `TermWindow`). During rendering, if `pane_state(pane_id).overlay.is_some()`, the overlay pane is rendered instead of the real pane.

`start_overlay(term_window, tab, func)`:
1. Calls `allocate(tab_size, term_config)` to get a `(TermWizTerminal, Arc<TermWizTerminalPane>)`.
2. Spawns `func` into a new OS thread via `promise::spawn::spawn_into_new_thread`.
3. When `func` returns, schedules `TermWindow::schedule_cancel_overlay(window, tab_id, overlay_pane_id)`.
4. Returns `(Arc<dyn Pane>, Pin<Box<dyn Future>>)` — the caller assigns the pane and polls the future.

`start_overlay_pane` is identical but covers a single pane instead of the full tab.

### UI primitives available in the overlay thread

Inside the `TermWizTerminal` closure:

- `term.render(&[Change::Text(...), Change::Attribute(...), Change::CursorPosition(...)])` — renders text/colors/cursor using `termwiz::surface::Change`. The `Change` enum covers text, SGR attributes (color, bold, italic), cursor movement, clear operations.
- `term.poll_input(timeout?)` — returns `Option<InputEvent>` (keyboard, mouse, resize). This is the event loop primitive.
- `termwiz::lineedit::LineEditor` — full readline-style line editor, used by `prompt.rs`. Handles history, completion callbacks, cursor movement.
- `termwiz::terminal::Terminal` trait — the overlay's view of its terminal. The renderer is `TerminfoRenderer` which emits ANSI sequences.

### How `prompt.rs` handles keyboard input

Source: `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/overlay/prompt.rs`

Pattern:
1. Render a description block via `term.render(&[Change::Text(text)])`.
2. Create `LineEditor::new(&mut term)` and call `editor.read_line_with_optional_initial_value(&mut host, initial)`.
3. `LineEditor` internally calls `term.poll_input()` in a loop.
4. On completion, use `promise::spawn::spawn_into_main_thread` to deliver the result back to the Lua event system.

For the AI command overlay, the same pattern applies: `LineEditor` captures the prompt text, the result string is passed to the HTTP call, and the streamed response is rendered back via `term.render()` in a loop while reading NDJSON lines.

### How the search/copy overlay handles continuous keyboard input

Source: `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/overlay/copy.rs`

`CopyOverlay` implements `Pane` directly (not via `TermWizTerminal`). It wraps a `delegate: Arc<dyn Pane>` and intercepts `key_down` calls, rendering a search bar at the bottom via its `Renderable` implementation. This is a more complex path — appropriate for the persistent AI pane where the rendering needs to be tightly integrated with the pane's own terminal model.

**Decision for command overlay:** Use `start_overlay` + `LineEditor` pattern (same as `prompt.rs`). The overlay covers the full tab, captures a prompt string, makes a blocking HTTP call to Ollama, streams the response back using `term.render()` in a loop, and exits. The user can press Ctrl-C / Escape to cancel.

**Decision for AI pane:** Use the `TermWizTerminalPane` approach via `start_overlay_pane` (or a direct split), with a custom event loop inside the thread. The loop alternates between `term.poll_input()` (user messages) and line-by-line reading from the Ollama HTTP response. Output is appended via `term.render(&[Change::Text(chunk)])`.

---

## Comparison Matrix

| Criteria | Topic 1: Pane Context | Topic 2: HTTP Client | Topic 3: Pane Creation | Topic 4: Overlay |
|----------|----------------------|---------------------|----------------------|-----------------|
| Established pattern in codebase | `get_lines`, `get_logical_lines`, `get_current_working_dir` on `Pane` trait | `http_req` (sync, no streaming), `reqwest` (tokio, not in GUI) | `TermWizTerminalPane` + `allocate()` | `start_overlay` / `start_overlay_pane` |
| Recommended approach | Read via `Pane` trait methods on sibling `PaneId` | `ureq 2.x` (new dep, blocking, streaming-safe) | `TermWizTerminalPane` split via `tab.split_and_insert()` | `start_overlay` (command overlay); `start_overlay_pane` (AI pane) |
| Alternatives rejected | N/A | `http_req` (no streaming); `reqwest` (forces Tokio into GUI) | Custom `Pane` impl without `TermWizTerminal` (far more code) | Custom `Pane` impl like `CopyOverlay` (only needed if tight render integration required) |
| Risk | Scrollback may be large; need size cap | New crate dependency | None; pattern already proven | None; pattern already proven |

---

## Recommendation Summary

**Selected approaches:**

1. **Pane context reading** — Use `Mux::get_pane(sibling_pane_id)` to obtain an `Arc<dyn Pane>`, then call `get_dimensions()` to find the scrollback bounds and `get_logical_lines(top..bottom)` to extract text. Call `get_current_working_dir(AllowStale)` and `get_foreground_process_name(AllowStale)` for process context. Store the sibling `PaneId` at split time. Subscribe to `MuxNotification::PaneFocused` to track focus changes if needed.

2. **HTTP client** — Add `ureq = "2"` to the `arcterm-ai` crate's `Cargo.toml`. Use `ureq::get(url).call()?.into_reader()` wrapped in `BufReader` with `lines()` iteration for NDJSON streaming. This runs cleanly in the overlay thread without any runtime.

3. **Pane creation** — For the AI pane: use `allocate(size, term_config)` from `mux::termwiztermtab` to get a `TermWizTerminalPane`, insert it into the tab tree via `tab.split_and_insert(pane_index, split_request, Arc::clone(&tw_tab))`, and run the AI event loop in a thread via `promise::spawn::spawn_into_new_thread`. For keybinding: add `OpenAiPane` and `OpenCommandOverlay` variants to the `KeyAssignment` enum and handle them in `TermWindow::perform_key_assignment`.

4. **Overlay rendering** — For the command overlay: use `start_overlay` with a closure that follows the `prompt.rs` pattern (`LineEditor` then `term.render()` loop). For the AI pane: use `start_overlay_pane` or directly assign the pane after calling `tab.split_and_insert`. Render streamed LLM tokens via `term.render(&[Change::Text(token)])` inside the loop.

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Scrollback read is slow for large panes | Med | Med | Cap context at last 200 logical lines; use `AllowStale` cache policy for CWD/process |
| `ureq` TLS support varies by platform | Low | Med | Confirm `ureq` feature `tls` uses `rustls` (not OpenSSL) to avoid linking conflicts with the existing `openssl` crate; alternatively use HTTP (not HTTPS) for local Ollama endpoints which is the default |
| Overlay thread blocks GUI paint during HTTP call | Low | High | The overlay thread is independent of the main thread; `promise::spawn::spawn_into_new_thread` launches a real OS thread. The GUI continues repainting normally. The risk is only if the AI pane attempts to render on the main thread — it must not. Always use `spawn_into_new_thread`. |
| `TermWizTerminalPane` pane ID leaks if not cleaned up | Med | Low | Follow the `schedule_cancel_overlay` pattern: always register the cleanup callback. For a persistent AI pane, register a `MuxNotification::PaneRemoved` subscriber to clean up the sibling reference. |
| Two async executors (smol + embedded Tokio from transitive reqwest dep) | Low (if using ureq) | Med | Avoided entirely by choosing `ureq`. If `reqwest` is needed in future, use `tokio::runtime::Handle::current()` only if Tokio is already running, otherwise `Runtime::new()` in a scoped background thread, never on the main thread. |
| Sibling pane closed while AI pane is open | Med | Med | Detect via `MuxNotification::PaneRemoved(sibling_pane_id)`. When received, either close the AI pane or switch it to a "no context" mode. |

---

## Implementation Considerations

### Integration points with existing code

- **`config/src/keyassignment.rs`** — Add `OpenAiPane` and `OpenCommandOverlay` to the `KeyAssignment` enum. These are `#[derive(FromDynamic, ToDynamic)]` enums; follow the pattern of `SplitPane(SplitPane)`.
- **`wezterm-gui/src/termwindow/mod.rs`** — Handle the new `KeyAssignment` variants in `perform_key_assignment`. Follow the pattern of the launcher (lines 511–517): call `start_overlay`, then `assign_overlay`.
- **`wezterm-gui/src/overlay/mod.rs`** — Add `pub mod ai_overlay;` and `pub mod ai_pane;` following the pattern of `mod prompt;` and `mod launcher;`.
- **`mux/src/lib.rs`** — No changes required for basic operation. Optional: add `MuxNotification::AiPaneOpened(PaneId)` if other subscribers need to react.

### Migration path

No existing functionality is replaced. This is additive.

### Testing strategy

- Unit test context extraction: construct a `FakePane` (already used in `mux/src/pane.rs` test module lines 551–665), call `get_logical_lines` with known content, verify text extraction.
- Integration test HTTP streaming: spin up a mock HTTP server in the test that returns NDJSON line-by-line, verify that the overlay loop emits the correct `Change::Text` values.
- Manual test: open two panes, invoke `OpenAiPane`, verify the AI pane appears as a split and can read the sibling's last 10 lines of output.

### Performance implications

- `get_logical_lines()` acquires the pane's internal `Mutex` lock for the duration of the scan. For 200 lines this is negligible (< 1 ms). Do not hold a reference to the pane across the HTTP call.
- `ureq` HTTP calls are blocking. They must only happen on the overlay thread, never on the main thread or inside a `MuxNotification` subscriber.
- Rendering streamed tokens: call `term.render()` per NDJSON line (not per character) to avoid excessive repaint cycles. Each `render()` call wakes the `TermWizTerminalPane`'s pipe, which triggers `MuxNotification::PaneOutput`, which schedules a repaint. At typical LLM token speeds (10–50 tokens/sec) this is well within the GPU repaint budget.

---

## Sources

1. `/Users/lgbarn/Personal/arcterm/mux/src/pane.rs` — `Pane` trait definition
2. `/Users/lgbarn/Personal/arcterm/lua-api-crates/mux/src/pane.rs` — Lua API pane methods
3. `/Users/lgbarn/Personal/arcterm/mux/src/tab.rs` — `Tab`, `PositionedPane`, `SplitRequest`, `iter_panes`
4. `/Users/lgbarn/Personal/arcterm/mux/src/lib.rs` — `Mux`, `MuxNotification`, `split_pane`, `subscribe`, `resolve_pane_id`
5. `/Users/lgbarn/Personal/arcterm/mux/src/localpane.rs` — `LocalPane`, `CachedLeaderInfo`, `ProcessState`
6. `/Users/lgbarn/Personal/arcterm/mux/src/termwiztermtab.rs` — `TermWizTerminalPane`, `TermWizTerminal`, `allocate`
7. `/Users/lgbarn/Personal/arcterm/mux/src/domain.rs` — `Domain::split_pane`, `SplitSource`
8. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/overlay/mod.rs` — `start_overlay`, `start_overlay_pane`
9. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/overlay/prompt.rs` — `LineEditor` pattern
10. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/overlay/copy.rs` — `CopyOverlay` custom `Pane` pattern
11. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/overlay/launcher.rs` — `LauncherArgs`, overlay lifecycle
12. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/overlay/selector.rs` — `nucleo_matcher`, fuzzy filter pattern
13. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/termwindow/mod.rs` — `assign_overlay`, `cancel_overlay_for_tab`, `OverlayState`, `PaneState`, `TabState`
14. `/Users/lgbarn/Personal/arcterm/wezterm-gui/src/update.rs` — `http_req` usage pattern
15. `/Users/lgbarn/Personal/arcterm/Cargo.toml` — workspace dependency declarations (`reqwest 0.12`, `http_req 0.11`, `tokio 1.43`, `smol`)
16. `/Users/lgbarn/Personal/arcterm/config/src/keyassignment.rs` — `KeyAssignment` enum, `SplitPane` struct
17. `/Users/lgbarn/Personal/arcterm/promise/src/spawn.rs` — `spawn_into_new_thread`, `spawn_into_main_thread`, scheduler architecture
18. `/Users/lgbarn/Personal/arcterm/.shipyard/codebase/ARCHITECTURE.md` — async runtime characterization
19. `/Users/lgbarn/Personal/arcterm/.shipyard/codebase/STACK.md` — dependency inventory
20. https://crates.io/crates/ureq — `ureq` crate metadata (version 2.12, MIT license)

---

## Uncertainty Flags

- **Exit code access:** The `Pane` trait has no `get_last_exit_code()` method. The only path is via `LocalPane` downcast (`pane.downcast_ref::<LocalPane>()`) or via OSC 133 semantic zone annotations. Whether the user's shell emits OSC 133 is not guaranteed. If exit code context is a requirement, this needs further design work.
- **`ureq` TLS backend:** `ureq 2.x` defaults to `native-tls` or `rustls` depending on features. The workspace already has `openssl 0.10.57`. It is unverified whether adding `ureq` with `rustls` would cause symbol conflicts or duplicate TLS libraries. The Ollama default endpoint is `http://localhost:11434` (plain HTTP), which avoids TLS entirely for local development.
- **`reqwest` features in workspace declaration:** The workspace `Cargo.toml` declares `reqwest = "0.12"` without explicit features. The Tokio runtime dependency is conditional on features like `json`, `stream`, `multipart`. It is unverified exactly which features are enabled transitively, and whether Tokio is actually initialized inside the GUI process at runtime. Further profiling would clarify this.
- **Persistent AI pane persistence across sessions:** WezTerm supports session attach/detach via the mux socket. Whether a `TermWizTerminalPane`-based AI pane survives a detach/reattach cycle is not verified. The answer is likely no — `TermWizTerminalPane` is documented as a transient, non-detachable domain.
