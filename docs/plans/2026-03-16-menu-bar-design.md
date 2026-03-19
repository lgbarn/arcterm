# Arcterm Menu Bar Design

## Overview

Add a native menu bar to arcterm using the `muda` crate, surfacing existing functionality (splits, tabs, search, command palette, workspaces) through standard macOS menus. This makes arcterm discoverable for new users while keeping Leader-chord power-user shortcuts intact.

## Implementation

Use `muda` for cross-platform native menu construction, integrated with the existing winit 0.30 event loop. Menu item activations map to existing `KeyAction` variants or new ones where needed.

## Menu Structure

### Shell

| Item | Shortcut | Maps to |
|---|---|---|
| New Window | Cmd+N | *new* — spawn new OS window |
| New Tab | Cmd+T | `KeyAction::NewTab` |
| --- | | |
| Split Right | Cmd+D | `KeyAction::Split(Horizontal)` |
| Split Down | Cmd+Shift+D | `KeyAction::Split(Vertical)` |
| --- | | |
| Close Pane | Cmd+W | `KeyAction::ClosePane` |
| Close Tab | Cmd+Shift+W | `KeyAction::CloseTab` |
| Close Window | Cmd+Q | *new* — close OS window |
| --- | | |
| Reset Terminal | — | *new* — reset terminal emulation state |

### Edit

| Item | Shortcut | Maps to |
|---|---|---|
| Copy | Cmd+C | clipboard copy (existing) |
| Paste | Cmd+V | clipboard paste (existing) |
| Select All | Cmd+A | *new* — select all scrollback + visible |
| --- | | |
| Find... | Cmd+F | `KeyAction::SearchOpen` |
| Find Next | Cmd+G | *new* — search next |
| Find Previous | Cmd+Shift+G | *new* — search previous |
| --- | | |
| Clear Scrollback | Cmd+K | *new* — clear scrollback buffer |
| --- | | |
| Command Palette | Cmd+Shift+P | `KeyAction::PaletteOpen` |

### View

| Item | Shortcut | Maps to |
|---|---|---|
| Increase Font Size | Cmd+= | *new* — runtime font size change |
| Decrease Font Size | Cmd+- | *new* — runtime font size change |
| Reset Font Size | Cmd+0 | *new* — reset to config default |
| --- | | |
| Toggle Full Screen | Ctrl+Cmd+F | *new* — native fullscreen toggle |
| --- | | |
| Config Overlay | Leader+o | `KeyAction::OverlayOpen` |
| Plan Status | Leader+p | `KeyAction::PlanToggle` |

### Window

| Item | Shortcut | Maps to |
|---|---|---|
| Minimize | Cmd+M | native minimize |
| --- | | |
| Zoom Split | — | `KeyAction::ToggleZoom` |
| Select Split Above | Cmd+Opt+Up | `KeyAction::NavigatePane(Up)` |
| Select Split Below | Cmd+Opt+Down | `KeyAction::NavigatePane(Down)` |
| Select Split Left | Cmd+Opt+Left | `KeyAction::NavigatePane(Left)` |
| Select Split Right | Cmd+Opt+Right | `KeyAction::NavigatePane(Right)` |
| --- | | |
| Equalize Splits | Cmd+Ctrl+= | *new* — reset all split ratios to 0.5 |
| Resize Split Up | Cmd+Ctrl+Up | *new* — adjust split divider |
| Resize Split Down | Cmd+Ctrl+Down | *new* — adjust split divider |
| Resize Split Left | Cmd+Ctrl+Left | *new* — adjust split divider |
| Resize Split Right | Cmd+Ctrl+Right | *new* — adjust split divider |
| --- | | |
| Next Tab | Cmd+Shift+] | `KeyAction::NextTab` |
| Previous Tab | Cmd+Shift+[ | `KeyAction::PrevTab` |
| --- | | |
| Workspace Switcher | Leader+w | `KeyAction::WorkspaceSwitcherOpen` |

### Help

| Item | Shortcut | Maps to |
|---|---|---|
| Arcterm Help | — | *new* — open docs URL or built-in help |
| --- | | |
| Show Debug Info | Cmd+Opt+I | *new* — display version, GPU adapter, config path, pane count |
| Report Issue | — | *new* — open GitHub issues URL |

## Technical Approach

### Crate: `muda`

- Purpose-built menu library for winit apps
- Native NSMenu on macOS, native menus on Windows/Linux
- Add `muda = "0.15"` (or latest) to `arcterm-app/Cargo.toml`

### Integration Points

1. **Menu construction** — Build the `Menu` and all `Submenu`/`MenuItem` instances at startup in `resumed()`, after window creation.

2. **Event handling** — `muda` emits `MenuEvent`s on its own channel. Poll `MenuEvent::receiver()` each frame in `about_to_wait()` or via a winit `EventLoopProxy`. Map each menu item's ID to the corresponding `KeyAction` and dispatch through the existing action handler.

3. **macOS menu bar attachment** — Call `menu.init_for_nsapp()` to set as the application menu bar. For per-window menus (if needed later): `menu.init_for_hwnd(hwnd)`.

4. **Accelerator keys** — `muda` supports `Accelerator` structs for keyboard shortcut display. Define these to match the shortcuts shown above so they render in the menu.

5. **State sync** — For toggle items (Zoom Split, Full Screen), update the menu item's checked state when the underlying state changes.

### New Actions Needed

These features don't exist yet and need implementation beyond just the menu wiring:

- **New Window** — spawn a second OS window (multi-window support)
- **Reset Terminal** — reset terminal emulation state to defaults
- **Select All** — select entire scrollback + visible buffer
- **Find Next / Find Previous** — extend search overlay with directional navigation
- **Clear Scrollback** — clear the scrollback buffer
- **Font size adjustment** — runtime font size increase/decrease/reset
- **Toggle Full Screen** — native macOS fullscreen
- **Equalize Splits** — reset all split ratios to 0.5
- **Resize Splits** — programmatic split divider adjustment
- **Show Debug Info** — display version, GPU info, config path
- **Report Issue / Arcterm Help** — open URLs in default browser

### File Changes

| File | Change |
|---|---|
| `arcterm-app/Cargo.toml` | Add `muda` dependency |
| `arcterm-app/src/menu.rs` | *new* — menu construction, ID-to-action mapping, event dispatch |
| `arcterm-app/src/main.rs` | Import menu module, build menu in `resumed()`, poll menu events in `about_to_wait()` |
| `arcterm-app/src/keymap.rs` | Add new `KeyAction` variants for menu-only actions |

## Design Decisions

- **Leader chords stay** — menus complement, not replace, the Leader key system. Power users keep their muscle memory.
- **Cmd shortcuts for menus** — standard macOS Cmd-based shortcuts for menu items, separate from Leader chords. No conflicts.
- **`muda` over raw Cocoa** — avoids unsafe FFI, cross-platform ready, actively maintained, designed for winit.
- **Single module** — all menu logic in one `menu.rs` file to keep it contained.
