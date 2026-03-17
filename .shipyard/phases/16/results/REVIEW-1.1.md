# Review: Plan 1.1

## Verdict: MINOR_ISSUES

---

## Stage 1: Spec Compliance

**Verdict: PASS**

### Task 1: Add `[ai]` config section

- **Status: PASS**
- **Evidence:** `arcterm-app/src/config.rs` lines 19–36 contain `AiConfig` with `pub endpoint: String` and `pub model: String`, both `#[derive(Debug, Clone, Deserialize, Serialize)]`, with `#[serde(default)]` on the struct and a `Default` impl providing `"http://localhost:11434"` and `"qwen2.5-coder:7b"`. `ArctermConfig` gains `pub ai: AiConfig` at line 72 (with `#[serde(default)]`) and `ai: AiConfig::default()` in its `Default` impl at line 91. All three plan-specified tests (`ai_config_defaults`, `ai_config_toml_overrides`, `ai_config_omitted_uses_defaults`) are present at lines 806–830 and pass (`3 passed` confirmed by live test run).

### Task 2: Create Ollama HTTP client module

- **Status: PASS**
- **Evidence:** `arcterm-app/src/ollama.rs` is present (198 lines). All five types are defined: `ChatMessage` (line 12), `ChatRequest` (line 19), `ChatChunk` (line 27), `GenerateRequest` (line 34), `GenerateChunk` (line 43). `OllamaClient` (line 49) provides `new`, `url`, `chat`, and `generate`. `url()` trims trailing slashes via `trim_end_matches('/')` (line 67). `chat()` POSTs to `/api/chat` with `stream: true` (lines 84–88). `generate()` POSTs to `/api/generate` with `stream: false` (lines 104–108). `reqwest = { version = "0.12", features = ["json", "stream"] }` is present in `arcterm-app/Cargo.toml` at line 44. `mod ollama;` is registered in `main.rs` at line 198 (after `mod workspace;` at line 197). All 8 plan-specified tests pass (`8 passed` confirmed by live run).

### Task 3: Extend context with scrollback field

- **Status: PASS**
- **Evidence:** `arcterm-app/src/context.rs` line 46 adds `pub scrollback: Vec<String>` to `SiblingContext`. `collect_sibling_contexts` (lines 171–178) calls `t.all_text_rows()`, takes the last 30 lines via `saturating_sub(30)`, and assigns to `scrollback`. `format_context_osc7770` (lines 219–231) builds a `scrollback_json` array with per-element `\\` and `"` escaping, included in the JSON object at line 233. Both plan-specified tests (`sibling_context_has_scrollback`, `format_context_osc7770_includes_scrollback`) are present and pass. The two pre-existing tests that construct `SiblingContext` (`format_context_osc7770_valid_json`, lines 527 and 535) have `scrollback: vec![]` added as required. All 17 context tests pass.

---

## Stage 2: Code Quality

### Critical

None.

### Minor

- **`cwd` path value is not JSON-escaped in `format_context_osc7770`** — `arcterm-app/src/context.rs` line 204:
  ```rust
  Some(p) => format!("\"{}\"", p.display()),
  ```
  If a path contains a backslash (Windows paths) or a double-quote character, the emitted JSON will be malformed. The `last_command` field directly below it gets proper `\\` and `"` escaping (lines 208), but `cwd` does not. This is pre-existing behavior for `cwd`, not introduced by this plan — but the new `scrollback` field demonstrates correct escaping at lines 224–227, making the inconsistency easier to notice. Since this project runs on macOS/Linux and POSIX paths never contain `\` or `"`, the practical risk is low, but it is a latent correctness bug.
  - **Remediation:** Apply the same escaping to `cwd_json`:
    ```rust
    Some(p) => {
        let s = p.display().to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        format!("\"{}\"", s)
    }
    ```

- **`OllamaClient::endpoint` and `model` fields are `pub`** — `arcterm-app/src/ollama.rs` lines 50–51. Exposing the raw endpoint string directly invites callers to bypass `url()` and construct URLs manually, which would silently skip the trailing-slash normalization. The fields are currently unused outside the module (7 dead-code warnings), so there is no existing caller that depends on their visibility.
  - **Remediation:** Change `pub endpoint` and `pub model` to `pub(crate)` or provide accessor methods, keeping URL construction funneled through `url()`.

- **`generate()` hardcodes `stream: false` but returns `reqwest::Response`** — `arcterm-app/src/ollama.rs` lines 93–109. The doc comment says "one-shot … no conversation history," which is correct intent. However, the return type (`reqwest::Response`) makes this feel like streaming is possible. The Ollama `/api/generate` endpoint with `stream: false` returns a single JSON object; callers will need to call `.json::<GenerateChunk>()` (or a full-response type). This is a documentation gap rather than a correctness problem, but it will cause confusion when the method is wired up in Plan 1.2/1.3.
  - **Remediation:** Add a sentence to the doc comment clarifying that with `stream: false` the body is a single `GenerateChunk` JSON object, and callers should use `.json::<GenerateChunk>().await`.

### Suggestions

- **Test count discrepancy between plan and summary** — The plan (Task 2, Step 4) specifies 7 tests by name, but the summary reports 8 and the live run confirms 8. The plan omits `generate_chunk_deserializes` from its named list. This is harmless (more tests is better), but the plan's test inventory should be updated to stay accurate for future readers.

- **`OllamaClient` does not implement `Debug`** — `arcterm-app/src/ollama.rs` line 49. All surrounding types derive `Debug`, but `OllamaClient` does not because `reqwest::Client` does not implement `Debug` by default. This is understandable, but it means the client struct cannot be included in `{:?}` log output. A manual `Debug` impl that omits the `http` field (or uses `reqwest::ClientBuilder` with a named wrapper) would improve observability.
  - **Remediation:** Add a manual `impl Debug for OllamaClient` that prints `endpoint` and `model` and elides `http`.

- **No `timeout` configured on `reqwest::Client`** — `arcterm-app/src/ollama.rs` line 61: `reqwest::Client::new()`. A default client has no connection or read timeout. If the Ollama process is unresponsive, `chat()` and `generate()` will hang indefinitely, blocking the async executor.
  - **Remediation:** Use `reqwest::Client::builder().timeout(Duration::from_secs(30)).build().unwrap_or_default()` in `OllamaClient::new`, or accept a `timeout: Option<Duration>` parameter.
