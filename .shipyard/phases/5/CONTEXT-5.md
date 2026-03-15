# Phase 5 Context — Design Decisions

## Workspace File Location
**Decision:** `~/.config/arcterm/workspaces/` (XDG-compliant)
**Rationale:** Alongside config.toml. Per-project `.arcterm/` deferred to future phase.

## Session Auto-Save
**Decision:** Auto-save on exit, auto-restore on launch.
**What's saved:** Pane tree layout (splits, ratios), tab state, per-pane working directories, per-pane shell commands, environment variables, window dimensions.
**What's NOT saved:** Scrollback history, running process state, PTY state.

## Scrollback Persistence
**Decision:** No — discard on exit.
**Rationale:** Simpler, avoids large files. Layout and commands restored but history starts fresh.

## Workspace TOML Schema
```toml
[workspace]
name = "my-project"
directory = "~/projects/my-project"

[[panes]]
command = "nvim ."
position = "left"
width = "60%"

[[panes]]
command = "cargo watch -x test"
position = "top-right"

[[panes]]
command = "claude"
position = "bottom-right"
type = "ai-agent"

[environment]
KUBECONFIG = "~/.kube/prod-config"
```

## CLI Subcommands
- `arcterm open <workspace>` — load workspace file, restore layout
- `arcterm save [name]` — save current session as workspace
- `arcterm list` — list available workspaces
- Default launch (no args): restore last session if auto-saved, else single pane

## Workspace Switcher
- `Leader+w` opens fuzzy overlay (reuses command palette UI pattern)
- Lists workspace files from ~/.config/arcterm/workspaces/
- Enter opens selected workspace (closes current tabs, opens new layout)
