# Phase 6 Context — Design Decisions (autonomous defaults)

## WASM Interface Definition
**Decision:** wit-bindgen (Component Model standard)
**Rationale:** Industry direction (Zellij, Spin, Fermyon). Type-safe bindings for Rust/Go/Python/JS. Future-proof.

## Plugin UI Rendering
**Decision:** Text-based draw commands (styled text lines)
**Rationale:** Plugins emit styled lines rendered via existing TextRenderer. Simplest, covers 90% of plugin UIs. Canvas-style deferred to future.

## AI Plugin API (MCP)
**Decision:** Include MCP basics in Phase 6
**Rationale:** Plugins can register tool schemas. AI agents can discover tools. Full MCP orchestration in Phase 7.

## Plugin Storage
**Decision:** ~/.config/arcterm/plugins/ for installed plugins
**Dev mode:** arcterm plugin dev ./my-plugin loads from local path

## Permission Model
**Decision:** Capability-based sandbox declared in plugin.toml manifest
- filesystem: list of allowed paths
- network: bool
- panes: "none" | "read" | "write"
- ai: bool (register MCP tools)

## Event Bus
**Decision:** Pub/sub with typed events
- PaneOpened, PaneClosed, CommandExecuted, WorkspaceSwitched
- Plugins subscribe to events they care about via WIT interface
