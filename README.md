# Arcterm

A GPU-accelerated, AI-native terminal emulator built in Rust.

Arcterm combines a fast wgpu-rendered terminal with a tmux-style multiplexer, native macOS menus, neovim-aware navigation, and built-in AI agent detection — all in a single binary.

## Features

- **GPU rendering** — wgpu backend with text (glyphon) and quad pipelines for smooth, high-FPS output
- **Terminal emulation** — powered by alacritty_terminal with full ANSI/VT support
- **Multiplexer** — splits, tabs, and pane navigation via Ctrl+a leader key (no tmux needed)
- **Native menu bar** — Shell, Edit, View, Window, Help with standard Cmd shortcuts
- **Neovim integration** — Ctrl+h/j/k/l seamlessly crosses arcterm/nvim split boundaries
- **Command palette** — fuzzy-searchable action list (Ctrl+Space or Cmd+Shift+P)
- **Workspaces** — save/restore layouts as TOML, auto-restore last session, fuzzy switcher
- **Cross-pane search** — regex search across all panes with match highlighting
- **AI awareness** — detects Claude, ChatGPT, and other agents running in panes
- **Inline images** — Kitty graphics protocol support
- **Plugin system** — WASM plugins via wasmtime with MCP tool integration
- **Configurable** — colors, fonts, scrollback, shell, keybindings

## Requirements

- macOS (primary platform)
- Rust 1.85+ (2024 edition)
- GPU with Vulkan, Metal, or DX12 support

## Building

```bash
cargo build --release -p arcterm-app
```

## Running

```bash
cargo run --release -p arcterm-app
```

Or after building:

```bash
./target/release/arcterm-app
```

### CLI Options

```bash
arcterm-app                          # Launch with default/last session
arcterm-app open <workspace>         # Restore a saved workspace
arcterm-app list-workspaces          # List available workspaces
arcterm-app plugin dev <path>        # Load a dev plugin
```

## Key Bindings

### Leader Key (Ctrl+a)

| Binding | Action |
|---------|--------|
| Leader + n | Split right |
| Leader + v | Split down |
| Leader + q | Close pane |
| Leader + z | Toggle zoom |
| Leader + t | New tab |
| Leader + 1-9 | Switch to tab N |
| Leader + W | Close tab |
| Leader + w | Workspace switcher |
| Leader + s | Save workspace |
| Leader + a | Jump to AI pane |
| Leader + p | Toggle plan status |
| Leader + o | Config overlay |
| Leader + / | Cross-pane search |
| Arrows | Resize pane |

### Direct Shortcuts

| Binding | Action |
|---------|--------|
| Ctrl+h/j/k/l | Navigate panes |
| Ctrl+Space | Command palette |

### Menu Shortcuts (macOS)

| Binding | Action |
|---------|--------|
| Cmd+N | New window |
| Cmd+T | New tab |
| Cmd+D | Split right |
| Cmd+Shift+D | Split down |
| Cmd+W | Close pane |
| Cmd+Shift+W | Close tab |
| Cmd+C / Cmd+V | Copy / Paste |
| Cmd+F | Find |
| Cmd+G / Cmd+Shift+G | Find next / previous |
| Cmd+K | Clear scrollback |
| Cmd+Shift+P | Command palette |
| Cmd+= / Cmd+- / Cmd+0 | Font size |
| Ctrl+Cmd+F | Toggle fullscreen |
| Cmd+M | Minimize |

## Configuration

Config file: `~/.config/arcterm/config.toml`

```toml
font_size = 14.0
scrollback_lines = 10000
color_scheme = "dark"
shell = "/bin/zsh"

[multiplexer]
leader_timeout_ms = 500
```

Workspaces are stored in `~/.config/arcterm/workspaces/`.

## Project Structure

```
arcterm/
  arcterm-app/      # Application entry point, event loop, UI overlays
  arcterm-render/   # GPU rendering (wgpu, glyphon, quad pipeline)
  arcterm-plugin/   # WASM plugin system (wasmtime, WIT interfaces)
```

## License

MIT
