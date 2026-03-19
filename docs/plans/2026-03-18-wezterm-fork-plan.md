# Plan: Fork WezTerm as ArcTerm Base

## Context

Building arcterm from scratch (currently at Phase 16) has been valuable for learning but the terminal fundamentals — VT parsing, rendering fidelity, cross-platform support — are consuming effort better spent on the differentiating AI features. WezTerm is the best fork candidate: same language (Rust), MIT license, built-in multiplexer, excellent Lua plugin system, and proven daily-driver quality with 21k+ GitHub stars and 437 contributors.

The goal is to fork WezTerm, rebrand it as ArcTerm, and bolt on the three key innovations from the current arcterm project:
1. **WASM plugin system** — capability-based sandbox (Phase 6)
2. **AI integration layer** — cross-pane context, AI tool detection, Ollama/Claude integration (Phases 7, 16)
3. **Structured output rendering** — OSC 7770 protocol for rich content (Phase 4)

## Phase 1: Fork & Rebrand ✅

1. Fork `wez/wezterm` on GitHub as `lgbarn/arcterm`
2. Rebrand: rename binary, update `Cargo.toml` metadata, window title, about dialog
3. Verify it builds and runs on macOS (`cargo build --release`)
4. Set up CI (GitHub Actions) for the fork
5. Create a `CONTRIBUTING.md` noting the upstream relationship

**Key files:**
- `Cargo.toml` (workspace root)
- `wezterm-gui/src/main.rs` (binary entry point)
- `.github/workflows/` (CI)

## Phase 2: Port WASM Plugin System

Port the `arcterm-plugin` crate's WASM sandbox into WezTerm's architecture:

1. Study WezTerm's existing Lua plugin system (`config/src/lua/`) to understand extension points
2. Add `wasmtime` dependency to workspace
3. Create `arcterm-wasm-plugin` crate that provides:
   - WASM component model loading
   - Capability-based permissions (filesystem, network, terminal I/O)
   - Plugin lifecycle management (init, update, destroy)
4. Wire WASM plugins alongside Lua plugins — both should coexist
5. Expose terminal state to WASM plugins via a host API

**Key files from current arcterm to port:**
- `arcterm-plugin/src/` (existing WASM plugin implementation)

**Key WezTerm files to modify:**
- `config/src/lua/` (plugin loading infrastructure)
- `wezterm-gui/src/` (plugin UI integration)

## Phase 3: Port Structured Output (OSC 7770)

1. Add OSC 7770 sequence parsing to WezTerm's terminal parser
2. Implement rich content rendering in WezTerm's GPU renderer:
   - Syntax-highlighted code blocks
   - Side-by-side diffs
   - Collapsible JSON trees
   - Inline images
3. Create Lua API for plugins to emit structured content

**Key WezTerm files to modify:**
- `term/src/` (VT parser, OSC handling)
- `wezterm-gui/src/` (renderer)

## Phase 4: Port AI Integration Layer

1. Add AI pane type — a special pane that connects to Ollama/Claude API instead of a PTY
2. Implement cross-pane context sharing:
   - AI pane can read output from sibling panes
   - Command detection and error auto-explanation
3. Command suggestion overlay (triggered by hotkey or prefix)
4. Multi-model support: Ollama (local), Claude API, configurable endpoints
5. Lua API for AI features so plugins can leverage them

**Key files from current arcterm to port:**
- Phase 7 AI integration code
- Phase 16 Ollama integration design (`docs/plans/2026-03-17-local-llm-integration-design.md`)

## Phase 5: Polish & Differentiate

1. Custom keybinding defaults optimized for AI workflow
2. Workspace/session persistence with AI context
3. Performance profiling (carry forward samply workflow from current arcterm)
4. Documentation: user guide, plugin development guide

## Verification

- [x] Fork builds and runs on macOS with `cargo check`
- [ ] Full `cargo build --release` verification
- [ ] Existing WezTerm Lua plugins still work
- [ ] WASM plugins load and execute in sandbox
- [ ] OSC 7770 sequences render structured content
- [ ] AI pane connects to Ollama and responds to prompts
- [ ] Cross-pane context sharing works (AI pane sees sibling output)
- [ ] Command suggestion overlay triggers and shows suggestions
- [ ] CI passes on all pushes

## Risk Notes

- **Upstream drift**: WezTerm is actively developed. Plan to periodically merge upstream changes. Keep custom code in clearly separated modules/crates to minimize merge conflicts.
- **Lua + WASM coexistence**: Two plugin systems adds complexity. Consider whether Lua plugins can invoke WASM plugins and vice versa.
- **WezTerm architecture learning curve**: Need to deeply understand WezTerm's codebase before modifying it. Budget time for exploration.
