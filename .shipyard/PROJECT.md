# Arcterm — AI Command Center Terminal

## Vision

A high-performance, Rust-based terminal emulator where AI tools are first-class citizens. Panes are the universal primitive — shells, AI agents, plan viewers, and plugins all compose naturally through a vim-native multiplexer. Open source, no login required, no cloud dependency.

## Core Identity

- **Category:** AI-native terminal emulator
- **Language:** Rust
- **Renderer:** wgpu (WebGPU — Metal/Vulkan/DX12)
- **Plugin Runtime:** wasmtime (WASM with AI-native plugin API)
- **Config:** TOML base + auditable AI overlays
- **Platforms:** macOS (primary polish target), Linux, Windows — all via wgpu
- **License:** Open source (TBD: MIT or Apache 2.0)

## Differentiators

No terminal in the current landscape combines high-performance native rendering with deep AI integration. Warp has AI but is proprietary/closed. Ghostty, Kitty, and Alacritty are fast and open but have zero AI capabilities. Wave is open but Electron-based. arcterm fills this gap.

1. **Structured Output Protocol** — custom OSC sequences (OSC 7770) for typed content
2. **Cross-pane AI intelligence** — AI panes read context from sibling panes
3. **WASM plugins with AI Plugin API** — MCP-compatible tool discovery by AI agents
4. **Project-aware workspaces** — full layout/environment/context restoration per project
5. **Config overlay system** — transparent, diffable, reversible AI configuration
6. **Ambient plan awareness** — persistent status strip with expandable plan viewer

## Architecture

### Pane-Native AI Model

Everything is a pane. AI agents, shells, plan viewers, and plugin UIs are all panes. The multiplexer is the AI interface.

Pane types:
- `shell` — standard shell session
- `ai-agent` — detected AI CLI tool (Claude Code, Codex, Gemini CLI, etc.)
- `plan-viewer` — structured plan/roadmap display
- `plugin` — WASM plugin rendering surface

Cross-pane intelligence (enabled for `ai-agent` panes):
- **Context sharing** — AI pane reads output from sibling panes via structured protocol
- **Error bridging** — build failures surface as structured context to AI panes
- **Command awareness** — working directory, last command, exit code tracked per shell pane

AI tool detection:
1. Process name matching (`claude`, `codex`, `gemini`, `aider`, etc.)
2. Structured output protocol handshake (self-identification)
3. Heuristic fallback (output pattern analysis)

### Structured Output Protocol

Custom OSC escape sequences that AI tools emit to tell arcterm what they're sending:

```
ESC ] 7770 ; start ; type=<content_type> [; key=value]* ST
  <content>
ESC ] 7770 ; end ST
```

Supported content types (v1):
- `code_block` — syntax highlighted, copyable, foldable (lang= attribute)
- `diff` — side-by-side or inline with file context
- `plan` — interactive task list with status tracking
- `markdown` — rendered markdown with headings, lists, links
- `json` — collapsible, searchable tree view
- `error` — structured error with file, line, message, suggested fix
- `progress` — progress bars, spinners, status indicators
- `image` — inline image rendering (Kitty graphics protocol)

Fallback auto-detection for non-protocol tools:
- Fenced code blocks → `code_block`
- Unified diff format → `diff`
- JSON blobs → `json`
- Markdown patterns → `markdown`
- ANSI output untouched — zero interference with existing tools

Design rule: protocol is purely additive. Non-aware tools work identically to any other terminal.

### Workspace Manager

Project-aware workspaces defined in TOML:

```toml
[workspace]
name = "kubernetes"
directory = "~/projects/k8s-infra"

[[panes]]
command = "nvim ."
position = "left"
width = "60%"

[[panes]]
command = "kubectl logs -f deployment/api"
position = "top-right"

[[panes]]
command = "claude"
position = "bottom-right"
type = "ai-agent"
context_from = ["all"]

[environment]
KUBECONFIG = "~/.kube/prod-config"
```

Features:
- `arcterm open <workspace>` restores full layout, commands, environment, scrollback
- Session persistence survives crashes and reboots
- AI can generate workspace TOML files
- Workspace files are git-committable for team sharing

### Vim Navigation Model

| Key | Action |
|---|---|
| `Ctrl+h/j/k/l` | Move between panes (Neovim-aware — traverses nvim splits first) |
| `Leader+w` | Workspace switcher (fuzzy) |
| `Leader+n` | New pane (split current) |
| `Leader+q` | Close pane |
| `Leader+z` | Zoom pane (fullscreen toggle) |
| `Leader+/` | Search across all pane output |
| `Leader+a` | Jump to active AI pane |
| `Leader+p` | Toggle plan status layer |
| `Leader+o` | Open AI config overlay diff |
| `Ctrl+Space` | Command palette |

Leader key: configurable, defaults to `Space`.

Neovim-aware pane crossing: arcterm detects Neovim and coordinates directional navigation — keys traverse Neovim splits first, then cross to adjacent arcterm panes. No plugin hacks needed.

### WASM Plugin System

Plugins compile to WASM from any language. Sandboxed with capability-based permissions.

Plugin manifest:
```toml
[plugin]
name = "k8s-dashboard"
version = "0.1.0"
description = "Kubernetes cluster overview panel"

[permissions]
network = true
filesystem = ["~/.kube"]
panes = "read"
ai = true

[ai]
mcp_tools = [
  { name = "get_pods", description = "List pods in namespace" },
  { name = "get_logs", description = "Tail logs for a pod" },
]
```

Plugin capabilities:
- Render UI in a pane
- Read sibling pane output (with permission)
- Register MCP-compatible tools discoverable by AI agents
- Subscribe to events (pane opened, command executed, error detected, workspace switched)
- Modify config overlays (with user approval)

AI Plugin API: AI agents running in arcterm can query installed plugins, discover MCP tool schemas, invoke plugin tools, and receive structured responses. Install a plugin → AI agents can use it automatically.

Plugin distribution:
- Registry: `arcterm plugin install k8s-dashboard`
- Git: `arcterm plugin install github:user/plugin`
- Local dev: `arcterm plugin dev ./my-plugin`

### Plan Status Layer

Persistent status strip showing current phase, task progress, and next action:

```
📋 arcterm · Phase 2/5 · Task 3/4 · ██████░░ 75%
```

`Leader+p` expands to full plan view:
- Rendered from plan files (PLAN.md, STATE.json, roadmap)
- Collapsible phase tree with task status
- Current task highlighted, verification commands inline
- Auto-updates on file change
- Keyboard navigable (j/k, Enter, q)

Plan detection: `.shipyard/`, `PLAN.md`, `TODO.md`, or custom workspace config paths. Shipyard-native, extensible via plugins.

### Config Overlay System

```
~/.config/arcterm/config.toml          ← base (hand-edited)
~/.config/arcterm/overlays/            ← AI-applied layers
  ├── workspace-kubernetes.toml       ← workspace-specific
  ├── agent-suggestion-001.toml       ← pending AI suggestion
  └── accepted/                       ← approved overlays
      └── dark-log-theme.toml
```

Flow:
1. AI suggests config change → writes pending overlay
2. `Leader+o` shows overlay diff
3. User accepts (a), rejects (x), or edits (e)
4. Stack priority: base → accepted overlays → workspace overlay
5. `arcterm config flatten` exports resolved config as single TOML

## Technical Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Renderer | wgpu | WebGPU future-proof, cross-platform from one codebase |
| VT parsing | Custom in Rust | Alacritty's vte crate as reference, extended for structured protocol |
| Async runtime | tokio | AI communication, plugin IPC, events all async |
| Plugin runtime | wasmtime | Production-grade WASM, capability-based security |
| Font rendering | swash + cosmic-text | Pure Rust, no system dependencies |
| Config parsing | toml crate | Standard, well-maintained |
| Image protocol | Kitty graphics | Most adopted modern protocol, Sixel later |
| Terminal protocol | Full VT100/VT220 + xterm extensions | Compatibility non-negotiable |
| Serialization | serde + MessagePack | Structured protocol and plugin IPC |

## Performance Targets

| Metric | Target |
|---|---|
| Key-to-screen latency | < 5ms |
| Frame rate (text scrolling) | 120+ FPS |
| Cold start | < 100ms |
| Memory baseline | < 50MB |
| Memory per pane | < 10MB |
| WASM plugin load | < 50ms |
| Workspace restore | < 500ms |

## Explicit Non-Goals

- No built-in shell (uses system shell)
- No built-in AI model (uses external AI CLI tools)
- No web browser embedding (plugins can, core won't)
- No GUI settings panel (TOML + overlays is the interface)
- No proprietary cloud service or login requirement
- No SDK or library extraction (ship the terminal first)
- No protocol standardization effort (battle-test internally first)

## Target User

DevOps engineers and developers who:
- Live in the terminal (vim/neovim as primary editor)
- Use AI CLI tools daily (Claude Code, Codex, Gemini CLI)
- Work across multiple projects simultaneously
- Want AI to enhance their workflow without taking over
- Value open source, transparency, and control
