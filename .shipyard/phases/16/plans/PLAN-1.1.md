# Plan 1.1: Foundation — Config, Ollama Client, Context Extension

> **Wave 1** — No dependencies on other plans.

**Goal:** Build the non-UI plumbing: `[ai]` config section, Ollama HTTP client, and scrollback context extension.

## Task 1: Add `[ai]` config section

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
git commit -m "shipyard(phase-16): add [ai] config section with endpoint and model fields"
```

---

## Task 2: Create Ollama HTTP client module

**Files:**
- Create: `arcterm-app/src/ollama.rs`
- Modify: `arcterm-app/src/main.rs:197` (add `mod ollama;`)
- Modify: `arcterm-app/Cargo.toml` (add reqwest dependency)
- Test: `arcterm-app/src/ollama.rs` (inline tests)

**Step 1: Create `arcterm-app/src/ollama.rs` with full module**

The module provides types (ChatMessage, ChatRequest, ChatChunk, GenerateRequest, GenerateChunk) and an OllamaClient struct wrapping reqwest. See the implementation plan at `docs/plans/2026-03-17-local-llm-implementation.md` Task 2 for the complete source code.

Key details:
- `OllamaClient::new(endpoint, model)` creates the client
- `OllamaClient::url(path)` builds full URL, stripping trailing slashes
- `OllamaClient::chat(messages)` sends POST to `/api/chat` with `stream: true`
- `OllamaClient::generate(prompt, system)` sends POST to `/api/generate` with `stream: false`

**Step 2: Add reqwest to Cargo.toml**

Add to `arcterm-app/Cargo.toml` under `[dependencies]`:
```toml
reqwest = { version = "0.12", features = ["json", "stream"] }
```

**Step 3: Register the module**

Add `mod ollama;` to `arcterm-app/src/main.rs` after `mod workspace;` (line ~197).

**Step 4: Run tests**

Run: `cargo test --package arcterm-app --lib ollama::tests`
Expected: ALL PASS (7 tests: chat_message_serializes, chat_request_serializes_with_stream, chat_chunk_deserializes, chat_chunk_done_deserializes, generate_request_serializes, generate_chunk_deserializes, client_url_building, client_url_strips_trailing_slash)

**Step 5: Commit**

```bash
git add arcterm-app/src/ollama.rs arcterm-app/src/main.rs arcterm-app/Cargo.toml Cargo.lock
git commit -m "shipyard(phase-16): add Ollama REST API client with chat and generate endpoints"
```

---

## Task 3: Extend context with scrollback field

**Files:**
- Modify: `arcterm-app/src/context.rs:36-45` (SiblingContext struct)
- Modify: `arcterm-app/src/context.rs:158-176` (collect_sibling_contexts)
- Modify: `arcterm-app/src/context.rs:187-220` (format_context_osc7770)
- Test: `arcterm-app/src/context.rs` (inline tests)

**Step 1: Write the failing test**

Add to the existing `mod tests` in `context.rs`:

```rust
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

1. Add `pub scrollback: Vec<String>` field to `SiblingContext`
2. Update `collect_sibling_contexts` to populate scrollback using `Terminal::all_text_rows()` (last 30 lines)
3. Update `format_context_osc7770` to include `"scrollback":[...]` in JSON output
4. Fix all existing tests that construct `SiblingContext` by adding `scrollback: vec![]`

**Step 4: Run tests**

Run: `cargo test --package arcterm-app --lib context::tests`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add arcterm-app/src/context.rs
git commit -m "shipyard(phase-16): add scrollback field to SiblingContext (last 30 lines)"
```
