# Local LLM Integration — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use shipyard:shipyard-executing-plans to implement this plan task-by-task.

**Goal:** Add Ollama-backed AI assistant to arcterm via a persistent AI Pane (conversational) and a Command Overlay (one-shot command lookup).

**Architecture:** Two new modules (`ollama.rs` for the HTTP client, `command_overlay.rs` for the overlay UI) plus extensions to config, context, keymap, and AppState. The AI Pane reuses the existing pane infrastructure with a chat-mode flag rather than introducing a new pane type.

**Tech Stack:** reqwest (streaming HTTP), serde_json (Ollama API serialization), tokio (async), existing pulldown-cmark + syntect for Markdown rendering.

---

## Phase 1: Foundation (Config + Ollama Client + Context Extension)

These three tasks build the non-UI plumbing. No UI changes yet — all testable in isolation.

---

### Task 1: Add `[ai]` config section

**Files:**
- Modify: `arcterm-app/src/config.rs:22-70` (ArctermConfig struct + Default impl)
- Test: `arcterm-app/src/config.rs` (inline tests module)

**Step 1: Write the failing test**

Add to the existing `mod tests` block at the bottom of `config.rs`:

```rust
#[test]
fn ai_config_defaults() {
    let cfg = ArctermConfig::default();
    assert_eq!(cfg.ai.endpoint, "http://localhost:11434");
    assert_eq!(cfg.ai.model, "qwen2.5-coder:7b");
}

#[test]
fn ai_config_toml_overrides() {
    let toml = r#"
        [ai]
        endpoint = "http://localhost:9999"
        model = "llama3:8b"
    "#;
    let cfg: ArctermConfig = toml::from_str(toml).expect("valid TOML");
    assert_eq!(cfg.ai.endpoint, "http://localhost:9999");
    assert_eq!(cfg.ai.model, "llama3:8b");
}

#[test]
fn ai_config_omitted_uses_defaults() {
    let toml = r#"font_size = 16.0"#;
    let cfg: ArctermConfig = toml::from_str(toml).expect("valid TOML");
    assert_eq!(cfg.ai.endpoint, "http://localhost:11434");
    assert_eq!(cfg.ai.model, "qwen2.5-coder:7b");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --package arcterm-app --lib config::tests::ai_config`
Expected: FAIL — `no field named ai on type ArctermConfig`

**Step 3: Write the implementation**

Add `AiConfig` struct above `ArctermConfig` in `config.rs`:

```rust
/// AI assistant configuration (Ollama backend).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AiConfig {
    /// Ollama API endpoint URL.
    pub endpoint: String,
    /// Model name to use for completions.
    pub model: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder:7b".to_string(),
        }
    }
}
```

Add field to `ArctermConfig`:

```rust
/// AI assistant settings (Ollama backend).
#[serde(default)]
pub ai: AiConfig,
```

Add to `ArctermConfig::default()`:

```rust
ai: AiConfig::default(),
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --package arcterm-app --lib config::tests`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add arcterm-app/src/config.rs
git commit -m "feat(config): add [ai] section with endpoint and model fields"
```

---

### Task 2: Create Ollama HTTP client module

**Files:**
- Create: `arcterm-app/src/ollama.rs`
- Modify: `arcterm-app/src/main.rs:176` (add `mod ollama;`)
- Modify: `arcterm-app/Cargo.toml` (add reqwest dependency)
- Test: `arcterm-app/src/ollama.rs` (inline tests)

**Step 1: Write the failing test**

Create `arcterm-app/src/ollama.rs` with tests first:

```rust
//! Ollama REST API client for local LLM inference.
//!
//! Provides async streaming chat (`/api/chat`) and one-shot generation
//! (`/api/generate`) against a local Ollama instance.

use serde::{Deserialize, Serialize};

// -- Types --

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Request body for `POST /api/chat`.
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
}

/// A single streamed chunk from `/api/chat`.
#[derive(Debug, Deserialize)]
pub struct ChatChunk {
    pub message: Option<ChatMessage>,
    pub done: bool,
}

/// Request body for `POST /api/generate`.
#[derive(Debug, Serialize)]
pub struct GenerateRequest {
    pub model: String,
    pub prompt: String,
    pub system: Option<String>,
    pub stream: bool,
}

/// A single streamed chunk from `/api/generate`.
#[derive(Debug, Deserialize)]
pub struct GenerateChunk {
    pub response: String,
    pub done: bool,
}

/// Ollama client wrapping a reqwest HTTP client.
pub struct OllamaClient {
    pub endpoint: String,
    pub model: String,
    http: reqwest::Client,
}

impl OllamaClient {
    /// Create a new client pointing at the given Ollama endpoint.
    pub fn new(endpoint: String, model: String) -> Self {
        Self {
            endpoint,
            model,
            http: reqwest::Client::new(),
        }
    }

    /// Build the full URL for a given API path (e.g. "/api/chat").
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.endpoint.trim_end_matches('/'), path)
    }

    /// Send a chat request and return the response for streaming.
    ///
    /// Caller should read the response body line-by-line, deserializing
    /// each line as a `ChatChunk`.
    pub async fn chat(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let req = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: true,
        };
        self.http
            .post(&self.url("/api/chat"))
            .json(&req)
            .send()
            .await
    }

    /// Send a one-shot generate request (no conversation history).
    ///
    /// Used by the command overlay for single-response queries.
    pub async fn generate(
        &self,
        prompt: &str,
        system: Option<&str>,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let req = GenerateRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            system: system.map(|s| s.to_string()),
            stream: false,
        };
        self.http
            .post(&self.url("/api/generate"))
            .json(&req)
            .send()
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_message_serializes() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"hello\""));
    }

    #[test]
    fn chat_request_serializes_with_stream() {
        let req = ChatRequest {
            model: "qwen2.5-coder:7b".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "list files".to_string(),
            }],
            stream: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"stream\":true"));
        assert!(json.contains("\"model\":\"qwen2.5-coder:7b\""));
    }

    #[test]
    fn chat_chunk_deserializes() {
        let json = r#"{"message":{"role":"assistant","content":"hi"},"done":false}"#;
        let chunk: ChatChunk = serde_json::from_str(json).unwrap();
        assert!(!chunk.done);
        let msg = chunk.message.unwrap();
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "hi");
    }

    #[test]
    fn chat_chunk_done_deserializes() {
        let json = r#"{"message":{"role":"assistant","content":""},"done":true}"#;
        let chunk: ChatChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.done);
    }

    #[test]
    fn generate_request_serializes() {
        let req = GenerateRequest {
            model: "qwen2.5-coder:7b".to_string(),
            prompt: "how do I list pods".to_string(),
            system: Some("Return only a shell command.".to_string()),
            stream: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"stream\":false"));
        assert!(json.contains("\"system\":\"Return only a shell command.\""));
    }

    #[test]
    fn generate_chunk_deserializes() {
        let json = r#"{"response":"kubectl get pods","done":true}"#;
        let chunk: GenerateChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.done);
        assert_eq!(chunk.response, "kubectl get pods");
    }

    #[test]
    fn client_url_building() {
        let client = OllamaClient::new(
            "http://localhost:11434".to_string(),
            "test".to_string(),
        );
        assert_eq!(client.url("/api/chat"), "http://localhost:11434/api/chat");
    }

    #[test]
    fn client_url_strips_trailing_slash() {
        let client = OllamaClient::new(
            "http://localhost:11434/".to_string(),
            "test".to_string(),
        );
        assert_eq!(client.url("/api/chat"), "http://localhost:11434/api/chat");
    }
}
```

**Step 2: Add reqwest to Cargo.toml and register the module**

Add to `arcterm-app/Cargo.toml` under `[dependencies]`:
```toml
reqwest = { version = "0.12", features = ["json", "stream"] }
```

Add `mod ollama;` to `arcterm-app/src/main.rs` after line 196 (after `mod workspace;`):
```rust
mod ollama;
```

**Step 3: Run tests to verify they pass**

Run: `cargo test --package arcterm-app --lib ollama::tests`
Expected: ALL PASS (7 tests)

**Step 4: Commit**

```bash
git add arcterm-app/src/ollama.rs arcterm-app/src/main.rs arcterm-app/Cargo.toml Cargo.lock
git commit -m "feat(ollama): add Ollama REST API client with chat and generate endpoints"
```

---

### Task 3: Extend context with scrollback field

**Files:**
- Modify: `arcterm-app/src/context.rs:36-45` (SiblingContext struct)
- Modify: `arcterm-app/src/context.rs:158-176` (collect_sibling_contexts)
- Modify: `arcterm-app/src/context.rs:187-220` (format_context_osc7770)
- Test: `arcterm-app/src/context.rs` (inline tests)

**Step 1: Write the failing test**

Add to the existing `mod tests` in `context.rs`:

```rust
/// SiblingContext includes scrollback field.
#[test]
fn sibling_context_has_scrollback() {
    let sc = SiblingContext {
        pane_id: make_pane_id(1),
        cwd: None,
        last_command: None,
        exit_code: None,
        ai_type: None,
        scrollback: vec!["line1".to_string(), "line2".to_string()],
    };
    assert_eq!(sc.scrollback.len(), 2);
    assert_eq!(sc.scrollback[0], "line1");
}

/// format_context_osc7770 includes scrollback in JSON output.
#[test]
fn format_context_osc7770_includes_scrollback() {
    let siblings = vec![SiblingContext {
        pane_id: make_pane_id(1),
        cwd: Some(PathBuf::from("/tmp")),
        last_command: Some("cargo build".to_string()),
        exit_code: Some(1),
        ai_type: None,
        scrollback: vec![
            "error[E0308]: type mismatch".to_string(),
            "  --> src/main.rs:42".to_string(),
        ],
    }];
    let bytes = format_context_osc7770(&siblings);
    let s = String::from_utf8(bytes).unwrap();
    assert!(s.contains("\"scrollback\":"));
    assert!(s.contains("error[E0308]"));
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --package arcterm-app --lib context::tests::sibling_context_has_scrollback`
Expected: FAIL — `no field named scrollback`

**Step 3: Write the implementation**

Add `scrollback` field to `SiblingContext`:

```rust
pub struct SiblingContext {
    pub pane_id: PaneId,
    pub cwd: Option<PathBuf>,
    pub last_command: Option<String>,
    pub exit_code: Option<i32>,
    pub ai_type: Option<AiAgentKind>,
    /// Last N lines of terminal output from this pane.
    pub scrollback: Vec<String>,
}
```

Update `collect_sibling_contexts` to populate the field using `Terminal::all_text_rows()`:

```rust
pub fn collect_sibling_contexts(
    pane_contexts: &HashMap<PaneId, PaneContext>,
    panes: &HashMap<PaneId, Terminal>,
    exclude: PaneId,
) -> Vec<SiblingContext> {
    pane_contexts
        .iter()
        .filter(|&(&id, _)| id != exclude)
        .map(|(&id, ctx)| {
            let cwd = panes.get(&id).and_then(|t| t.cwd());
            // Grab last 30 lines of visible terminal output.
            let scrollback = panes
                .get(&id)
                .map(|t| {
                    let rows = t.all_text_rows();
                    let start = rows.len().saturating_sub(30);
                    rows[start..].to_vec()
                })
                .unwrap_or_default();
            SiblingContext {
                pane_id: id,
                cwd,
                last_command: ctx.last_command.clone(),
                exit_code: ctx.last_exit_code,
                ai_type: ctx.ai_type.clone(),
                scrollback,
            }
        })
        .collect()
}
```

Update `format_context_osc7770` to include the scrollback field in the JSON output. After the `ai_json` line, add:

```rust
let scrollback_json: String = {
    let escaped: Vec<String> = s.scrollback.iter()
        .map(|l| format!("\"{}\"", l.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    format!("[{}]", escaped.join(","))
};
```

And update the format string to include `\"scrollback\":{}` with `scrollback_json`.

Fix existing tests that construct `SiblingContext` by adding `scrollback: vec![]` to each.

**Step 4: Run tests to verify they pass**

Run: `cargo test --package arcterm-app --lib context::tests`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add arcterm-app/src/context.rs
git commit -m "feat(context): add scrollback field to SiblingContext (last 30 lines)"
```

---

## Phase 2: Command Overlay (the quick-invoke UI)

Phase 2 builds the simpler of the two components. This is the `Ctrl+Space → type question → get command → Enter to accept` flow.

---

### Task 1: Create CommandOverlay state machine

**Files:**
- Create: `arcterm-app/src/command_overlay.rs`
- Modify: `arcterm-app/src/main.rs:197` (add `mod command_overlay;`)
- Test: `arcterm-app/src/command_overlay.rs` (inline tests)

**Step 1: Write the failing test**

Create `arcterm-app/src/command_overlay.rs` with types and tests:

```rust
//! Command overlay: quick-invoke LLM query for one-shot shell commands.
//!
//! Triggered by Ctrl+Space. User types a question, Ollama returns a single
//! shell command. Enter accepts (pastes into active pane), Escape dismisses.

use winit::keyboard::{Key, NamedKey};

/// Actions produced by the command overlay's key handler.
#[derive(Debug, PartialEq)]
pub enum OverlayAction {
    /// Query string was updated (typing or backspace).
    UpdateQuery,
    /// User pressed Enter while typing — send to Ollama.
    Submit,
    /// User pressed Enter on the result — accept (paste into active pane).
    Accept(String),
    /// User pressed Escape — close the overlay.
    Close,
    /// Key consumed, no state change.
    Noop,
}

/// Which phase the overlay is in.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayPhase {
    /// User is typing their question.
    Input,
    /// Waiting for Ollama response.
    Loading,
    /// Showing the returned command.
    Result(String),
    /// Ollama returned an error.
    Error(String),
}

/// Runtime state for the command overlay.
pub struct CommandOverlayState {
    /// Current query string.
    pub query: String,
    /// Current phase.
    pub phase: OverlayPhase,
}

impl CommandOverlayState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            phase: OverlayPhase::Input,
        }
    }

    /// Handle a key press. Returns the action to take.
    pub fn handle_key(&mut self, logical_key: &Key) -> OverlayAction {
        match &self.phase {
            OverlayPhase::Input => match logical_key {
                Key::Named(NamedKey::Escape) => OverlayAction::Close,
                Key::Named(NamedKey::Enter) => {
                    if self.query.is_empty() {
                        OverlayAction::Noop
                    } else {
                        self.phase = OverlayPhase::Loading;
                        OverlayAction::Submit
                    }
                }
                Key::Named(NamedKey::Backspace) => {
                    self.query.pop();
                    OverlayAction::UpdateQuery
                }
                Key::Character(s) => {
                    self.query.push_str(s.as_str());
                    OverlayAction::UpdateQuery
                }
                _ => OverlayAction::Noop,
            },
            OverlayPhase::Loading => match logical_key {
                Key::Named(NamedKey::Escape) => OverlayAction::Close,
                _ => OverlayAction::Noop,
            },
            OverlayPhase::Result(cmd) => match logical_key {
                Key::Named(NamedKey::Escape) => OverlayAction::Close,
                Key::Named(NamedKey::Enter) => OverlayAction::Accept(cmd.clone()),
                _ => OverlayAction::Noop,
            },
            OverlayPhase::Error(_) => match logical_key {
                Key::Named(NamedKey::Escape) => OverlayAction::Close,
                _ => OverlayAction::Noop,
            },
        }
    }

    /// Set the result from Ollama.
    pub fn set_result(&mut self, command: String) {
        self.phase = OverlayPhase::Result(command);
    }

    /// Set an error message.
    pub fn set_error(&mut self, msg: String) {
        self.phase = OverlayPhase::Error(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_char(s: &str) -> Key {
        Key::Character(s.into())
    }
    fn key_named(k: NamedKey) -> Key {
        Key::Named(k)
    }

    #[test]
    fn new_overlay_starts_in_input_phase() {
        let state = CommandOverlayState::new();
        assert_eq!(state.phase, OverlayPhase::Input);
        assert!(state.query.is_empty());
    }

    #[test]
    fn typing_appends_to_query() {
        let mut state = CommandOverlayState::new();
        assert_eq!(state.handle_key(&key_char("h")), OverlayAction::UpdateQuery);
        assert_eq!(state.handle_key(&key_char("i")), OverlayAction::UpdateQuery);
        assert_eq!(state.query, "hi");
    }

    #[test]
    fn backspace_removes_last_char() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("a"));
        state.handle_key(&key_char("b"));
        state.handle_key(&key_named(NamedKey::Backspace));
        assert_eq!(state.query, "a");
    }

    #[test]
    fn enter_on_empty_query_is_noop() {
        let mut state = CommandOverlayState::new();
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Enter)),
            OverlayAction::Noop
        );
        assert_eq!(state.phase, OverlayPhase::Input);
    }

    #[test]
    fn enter_with_query_submits_and_transitions_to_loading() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("l"));
        state.handle_key(&key_char("s"));
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Enter)),
            OverlayAction::Submit
        );
        assert_eq!(state.phase, OverlayPhase::Loading);
    }

    #[test]
    fn escape_in_input_closes() {
        let mut state = CommandOverlayState::new();
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Escape)),
            OverlayAction::Close
        );
    }

    #[test]
    fn escape_in_loading_closes() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("x"));
        state.handle_key(&key_named(NamedKey::Enter));
        assert_eq!(state.phase, OverlayPhase::Loading);
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Escape)),
            OverlayAction::Close
        );
    }

    #[test]
    fn set_result_transitions_to_result_phase() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("q"));
        state.handle_key(&key_named(NamedKey::Enter));
        state.set_result("ls -la".to_string());
        assert_eq!(state.phase, OverlayPhase::Result("ls -la".to_string()));
    }

    #[test]
    fn enter_in_result_accepts_command() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("q"));
        state.handle_key(&key_named(NamedKey::Enter));
        state.set_result("ls -la".to_string());
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Enter)),
            OverlayAction::Accept("ls -la".to_string())
        );
    }

    #[test]
    fn escape_in_result_closes() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("q"));
        state.handle_key(&key_named(NamedKey::Enter));
        state.set_result("ls -la".to_string());
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Escape)),
            OverlayAction::Close
        );
    }

    #[test]
    fn set_error_transitions_to_error_phase() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("q"));
        state.handle_key(&key_named(NamedKey::Enter));
        state.set_error("connection refused".to_string());
        assert_eq!(
            state.phase,
            OverlayPhase::Error("connection refused".to_string())
        );
    }

    #[test]
    fn escape_in_error_closes() {
        let mut state = CommandOverlayState::new();
        state.handle_key(&key_char("q"));
        state.handle_key(&key_named(NamedKey::Enter));
        state.set_error("timeout".to_string());
        assert_eq!(
            state.handle_key(&key_named(NamedKey::Escape)),
            OverlayAction::Close
        );
    }
}
```

**Step 2: Register the module**

Add `mod command_overlay;` to `arcterm-app/src/main.rs` after the `mod ollama;` line.

**Step 3: Run tests to verify they pass**

Run: `cargo test --package arcterm-app --lib command_overlay::tests`
Expected: ALL PASS (12 tests)

**Step 4: Commit**

```bash
git add arcterm-app/src/command_overlay.rs arcterm-app/src/main.rs
git commit -m "feat(overlay): add CommandOverlay state machine with input/loading/result phases"
```

---

### Task 2: Wire Command Overlay into keymap and AppState

**Files:**
- Modify: `arcterm-app/src/keymap.rs:41-117` (add `OpenCommandOverlay` variant to `KeyAction`)
- Modify: `arcterm-app/src/keymap.rs:185+` (handle Ctrl+Space in Normal state)
- Modify: `arcterm-app/src/main.rs:540-676` (add `command_overlay` field to `AppState`)
- Modify: `arcterm-app/src/main.rs:1035+` (add `KeyAction::OpenCommandOverlay` dispatch)
- Test: manual — `Ctrl+Space` opens overlay, typing works, Escape closes

**Step 1: Add `OpenCommandOverlay` to `KeyAction` enum**

In `arcterm-app/src/keymap.rs`, add after `CrossPaneSearch` (around line 73):

```rust
/// Open the command overlay (Ctrl+Space).
OpenCommandOverlay,
```

**Step 2: Handle Ctrl+Space in the Normal state**

In `handle_logical_key_with_time`, inside the `KeymapState::Normal` match arm, add a clause for `Ctrl+Space` before the generic `Ctrl+<char>` handler:

```rust
// Ctrl+Space → command overlay
Key::Named(NamedKey::Space) if ctrl => {
    return KeyAction::OpenCommandOverlay;
}
```

**Step 3: Add `command_overlay` field to `AppState`**

In `AppState` struct (main.rs around line 642), add:

```rust
/// Command overlay state; `None` when the overlay is closed.
command_overlay: Option<command_overlay::CommandOverlayState>,
```

Initialize as `None` in the constructor.

**Step 4: Add dispatch for `KeyAction::OpenCommandOverlay`**

In the `dispatch_action` method (main.rs around line 1035), add a match arm:

```rust
KeyAction::OpenCommandOverlay => {
    if self.command_overlay.is_none() {
        self.command_overlay = Some(command_overlay::CommandOverlayState::new());
    }
    DispatchOutcome::Redraw
}
```

**Step 5: Route key events to overlay when open**

In the keyboard event handler (where the keymap is consulted), add an early check: if `self.command_overlay.is_some()`, route the key to `command_overlay.handle_key()` instead of the keymap. On `OverlayAction::Close`, set `self.command_overlay = None`. On `OverlayAction::Submit`, spawn a tokio task to call `ollama.generate()` and send the result back via a channel. On `OverlayAction::Accept(cmd)`, write `cmd + "\n"` to the active pane's PTY.

**Step 6: Verify manually**

Run: `cargo run --package arcterm-app`
- Press `Ctrl+Space` — overlay should open (may not render yet, but no crash)
- Press `Escape` — overlay should close
- Type characters, press Enter — should fire Ollama request (will fail gracefully if Ollama not running)

**Step 7: Commit**

```bash
git add arcterm-app/src/keymap.rs arcterm-app/src/main.rs
git commit -m "feat(overlay): wire Ctrl+Space command overlay into keymap and event loop"
```

---

### Task 3: Render Command Overlay

**Files:**
- Modify: `arcterm-app/src/main.rs` (build overlay quads in render path)
- Modify: `arcterm-render/src/lib.rs` (if needed — check if existing `OverlayQuad` infra is sufficient)
- Test: manual — overlay renders at top of screen with input text and result

**Step 1: Build overlay quads in the render snapshot**

In the `about_to_wait` / render snapshot assembly, when `self.command_overlay.is_some()`, construct overlay quads:

- A semi-transparent dark background bar at the top of the window (full width, ~60px height)
- The query text rendered in white
- A phase indicator: "..." during Loading, the command string during Result (green), error message during Error (red)
- During Result phase, show a hint: "Enter to accept · Esc to dismiss"

Follow the same pattern used by `search_overlay` and `palette_mode` for rendering overlay quads. The existing `OverlayQuad` type and overlay rendering pipeline in `arcterm-render` should be sufficient.

**Step 2: Verify manually**

Run: `cargo run --package arcterm-app`
- `Ctrl+Space` — dark bar appears at top with blinking cursor
- Type "list all docker containers" — text appears in the bar
- Enter — shows "..." then either the command or "LLM unavailable" error
- Escape — overlay disappears

**Step 3: Commit**

```bash
git add arcterm-app/src/main.rs arcterm-render/src/lib.rs
git commit -m "feat(overlay): render command overlay bar with input, loading, and result states"
```

---

## Phase 3: AI Pane (the persistent chat)

The AI pane reuses the existing terminal pane infrastructure. Rather than creating a whole new pane type, it opens a regular pane running a thin chat process that streams Ollama responses. This phase builds the bridge.

---

### Task 1: Create AI pane module with chat state

**Files:**
- Create: `arcterm-app/src/ai_pane.rs`
- Modify: `arcterm-app/src/main.rs` (add `mod ai_pane;`)
- Test: `arcterm-app/src/ai_pane.rs` (inline tests)

**Step 1: Write the types and tests**

Create `arcterm-app/src/ai_pane.rs`:

```rust
//! AI pane: persistent LLM chat session with sibling context awareness.
//!
//! The AI pane maintains a conversation history and injects sibling pane
//! context (scrollback, CWD, last command) into the system prompt.

use crate::ollama::ChatMessage;

/// System prompt for the AI pane.
pub const SYSTEM_PROMPT: &str = "\
You are a terminal assistant embedded in a GPU-accelerated terminal emulator. \
The user is a DevOps engineer. You have context from their active terminal pane \
including recent output, working directory, and last command with exit code.\n\n\
Be terse. Return shell commands directly when applicable. Prefer one-liners. \
Flag destructive operations (rm -rf, DROP TABLE, force push, etc.) before \
suggesting them. When explaining, keep it short.";

/// Per-pane AI chat state.
pub struct AiPaneState {
    /// Full conversation history (system + user + assistant messages).
    pub history: Vec<ChatMessage>,
    /// Whether a response is currently being streamed.
    pub streaming: bool,
    /// Accumulated response text for the current streaming response.
    pub pending_response: String,
}

impl AiPaneState {
    /// Create a new AI pane state with the system prompt.
    pub fn new() -> Self {
        Self {
            history: vec![ChatMessage {
                role: "system".to_string(),
                content: SYSTEM_PROMPT.to_string(),
            }],
            streaming: false,
            pending_response: String::new(),
        }
    }

    /// Inject sibling pane context into the conversation as a system message.
    pub fn inject_context(&mut self, cwd: Option<&str>, last_cmd: Option<&str>, exit_code: Option<i32>, scrollback: &[String]) {
        let mut parts = Vec::new();
        if let Some(cwd) = cwd {
            parts.push(format!("CWD: {cwd}"));
        }
        if let Some(cmd) = last_cmd {
            parts.push(format!("Last command: {cmd}"));
        }
        if let Some(code) = exit_code {
            parts.push(format!("Exit code: {code}"));
        }
        if !scrollback.is_empty() {
            let joined = scrollback.join("\n");
            parts.push(format!("Terminal output (last {} lines):\n{joined}", scrollback.len()));
        }
        if !parts.is_empty() {
            self.history.push(ChatMessage {
                role: "system".to_string(),
                content: format!("[Context from sibling pane]\n{}", parts.join("\n")),
            });
        }
    }

    /// Add a user message to the history.
    pub fn add_user_message(&mut self, content: String) {
        self.history.push(ChatMessage {
            role: "user".to_string(),
            content,
        });
        self.streaming = true;
        self.pending_response.clear();
    }

    /// Append a chunk of streamed response text.
    pub fn append_response_chunk(&mut self, chunk: &str) {
        self.pending_response.push_str(chunk);
    }

    /// Finalize the current streaming response.
    pub fn finalize_response(&mut self) {
        self.history.push(ChatMessage {
            role: "assistant".to_string(),
            content: self.pending_response.clone(),
        });
        self.streaming = false;
        self.pending_response.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_has_system_prompt() {
        let state = AiPaneState::new();
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].role, "system");
        assert!(state.history[0].content.contains("terminal assistant"));
    }

    #[test]
    fn inject_context_adds_system_message() {
        let mut state = AiPaneState::new();
        state.inject_context(
            Some("/home/user/project"),
            Some("cargo build"),
            Some(1),
            &["error[E0308]: type mismatch".to_string()],
        );
        assert_eq!(state.history.len(), 2);
        assert_eq!(state.history[1].role, "system");
        assert!(state.history[1].content.contains("CWD: /home/user/project"));
        assert!(state.history[1].content.contains("cargo build"));
        assert!(state.history[1].content.contains("Exit code: 1"));
        assert!(state.history[1].content.contains("error[E0308]"));
    }

    #[test]
    fn inject_empty_context_does_nothing() {
        let mut state = AiPaneState::new();
        state.inject_context(None, None, None, &[]);
        assert_eq!(state.history.len(), 1); // only system prompt
    }

    #[test]
    fn user_message_and_streaming_lifecycle() {
        let mut state = AiPaneState::new();
        state.add_user_message("what failed?".to_string());
        assert_eq!(state.history.len(), 2);
        assert!(state.streaming);

        state.append_response_chunk("The build ");
        state.append_response_chunk("failed because...");
        assert_eq!(state.pending_response, "The build failed because...");

        state.finalize_response();
        assert!(!state.streaming);
        assert_eq!(state.history.len(), 3);
        assert_eq!(state.history[2].role, "assistant");
        assert_eq!(state.history[2].content, "The build failed because...");
        assert!(state.pending_response.is_empty());
    }
}
```

**Step 2: Register the module**

Add `mod ai_pane;` to `arcterm-app/src/main.rs`.

**Step 3: Run tests**

Run: `cargo test --package arcterm-app --lib ai_pane::tests`
Expected: ALL PASS (4 tests)

**Step 4: Commit**

```bash
git add arcterm-app/src/ai_pane.rs arcterm-app/src/main.rs
git commit -m "feat(ai-pane): add AiPaneState with chat history, context injection, and streaming"
```

---

### Task 2: Wire AI Pane into keymap and AppState (Leader+i)

**Files:**
- Modify: `arcterm-app/src/keymap.rs` (add `OpenAiPane` and `RefreshAiContext` variants)
- Modify: `arcterm-app/src/main.rs` (add `ai_pane_states` map, handle Leader+i dispatch)
- Test: manual — Leader+i opens a new pane, Leader+c refreshes context

**Step 1: Add KeyAction variants**

In `keymap.rs`, add to the `KeyAction` enum:

```rust
/// Open a new AI chat pane (Leader+i).
OpenAiPane,
/// Refresh sibling context in the active AI pane (Leader+c).
RefreshAiContext,
```

**Step 2: Handle Leader+i and Leader+c in LeaderPending state**

In the `KeymapState::LeaderPending` match arm, add:

```rust
Key::Character(s) if s.as_str() == "i" => {
    self.state = KeymapState::Normal;
    return KeyAction::OpenAiPane;
}
Key::Character(s) if s.as_str() == "c" => {
    self.state = KeymapState::Normal;
    return KeyAction::RefreshAiContext;
}
```

**Step 3: Add AI pane state tracking to AppState**

Add to `AppState`:

```rust
/// Per-pane AI chat state; only populated for AI panes.
ai_pane_states: HashMap<PaneId, ai_pane::AiPaneState>,
```

**Step 4: Dispatch `OpenAiPane`**

In `dispatch_action`, add:

```rust
KeyAction::OpenAiPane => {
    // Split the active pane vertically, creating a new pane on the right.
    // Then mark the new pane as an AI pane by inserting an AiPaneState.
    // (Reuse the existing Split logic, then track the new pane ID.)
    // The AI pane runs a regular shell — the chat UI is rendered as an
    // overlay on top, and user input is intercepted before reaching the PTY.
    // Implementation: split right, get new pane ID, insert AiPaneState.
    DispatchOutcome::Redraw
}
```

The exact split+track pattern mirrors the existing `KeyAction::Split(Axis::Vertical)` code, followed by inserting an `AiPaneState::new()` for the new pane ID.

**Step 5: Dispatch `RefreshAiContext`**

```rust
KeyAction::RefreshAiContext => {
    // If the focused pane is an AI pane, collect sibling context and
    // call ai_state.inject_context().
    DispatchOutcome::Redraw
}
```

**Step 6: Verify manually**

Run: `cargo run --package arcterm-app`
- `Ctrl+a, i` — new pane opens on the right (AI pane)
- `Ctrl+a, c` — context refreshed (visible in logs)

**Step 7: Commit**

```bash
git add arcterm-app/src/keymap.rs arcterm-app/src/main.rs
git commit -m "feat(ai-pane): wire Leader+i to open AI pane, Leader+c to refresh context"
```

---

### Task 3: AI Pane chat rendering and Ollama streaming

**Files:**
- Modify: `arcterm-app/src/main.rs` (intercept input in AI panes, stream responses)
- Modify: `arcterm-app/src/ai_pane.rs` (add render helper for chat display)
- Test: manual — type in AI pane, see streamed response from Ollama

**Step 1: Intercept keyboard input in AI panes**

In the keyboard event handler, before forwarding to keymap, check if the focused pane has an `ai_pane_states` entry. If so:

- Printable characters and Backspace → append to / trim an input buffer in `AiPaneState`
- Enter → call `ai_state.add_user_message(input)`, spawn a tokio task:
  - Call `ollama_client.chat(ai_state.history.clone()).await`
  - Stream response chunks via a tokio channel back to the main thread
  - Each chunk: `ai_state.append_response_chunk(text)`, request redraw
  - On done: `ai_state.finalize_response()`
- Escape → close the AI pane (or switch focus away)

**Step 2: Render chat in AI pane**

Instead of rendering the raw terminal grid for AI panes, render the chat history:

- Each message as a styled block (user messages right-aligned or prefixed `>`, assistant messages left-aligned)
- Use existing `pulldown-cmark` + `syntect` for Markdown/code blocks in responses
- Show a typing indicator ("...") while streaming
- Show an input line at the bottom with the current user input

This should use the existing structured block rendering pipeline where possible.

**Step 3: On AI pane open, auto-inject sibling context**

When `OpenAiPane` is dispatched and the new AI pane is created, immediately call `inject_context()` with the context from the previously-focused sibling pane.

**Step 4: Verify manually**

Run: `cargo run --package arcterm-app` (with Ollama running + qwen2.5-coder:7b pulled)
- `Ctrl+a, i` — AI pane opens, shows system greeting or context summary
- Type "what does ls -la do" + Enter — response streams in
- Markdown code blocks are syntax-highlighted
- `Ctrl+a, c` — refreshes context from sibling pane

**Step 5: Commit**

```bash
git add arcterm-app/src/main.rs arcterm-app/src/ai_pane.rs
git commit -m "feat(ai-pane): chat rendering with Ollama streaming and auto context injection"
```
