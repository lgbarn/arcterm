# Menu Bar Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use shipyard:shipyard-executing-plans to implement this plan task-by-task.

**Goal:** Add a native menu bar to arcterm using the `muda` crate, wiring menu items to existing KeyAction dispatch and adding new actions for menu-only features.

**Architecture:** A new `menu.rs` module owns all menu construction and ID-to-action mapping. Menu events are polled each frame in `about_to_wait()` and converted to `KeyAction` variants dispatched through the existing match block. New `KeyAction` variants cover menu-only features (font size, fullscreen, clear scrollback, etc.).

**Tech Stack:** Rust, muda 0.17, winit 0.30, wgpu

**Design doc:** `docs/plans/2026-03-16-menu-bar-design.md`

---

<task id="1" name="Add muda dependency and new KeyAction variants">
  <description>Add the muda crate to Cargo.toml and extend the KeyAction enum with all new variants needed by menu items that don't map to existing actions.</description>
  <files>
    <modify>arcterm-app/Cargo.toml:22-42</modify>
    <modify>arcterm-app/src/keymap.rs:41-76</modify>
  </files>
  <steps>
    <step>Add muda dependency to Cargo.toml</step>
    <step>Add new KeyAction variants to keymap.rs</step>
    <step>Verify it compiles</step>
    <step>Commit</step>
  </steps>
  <verification>
    <command>cd /Users/lgbarn/Personal/arcterm && cargo check -p arcterm-app 2>&1 | tail -5</command>
    <expected>Finished</expected>
  </verification>
</task>

### Task 1: Add muda dependency and new KeyAction variants

**Files:**
- Modify: `arcterm-app/Cargo.toml:22-42`
- Modify: `arcterm-app/src/keymap.rs:41-76`

**Step 1: Add muda dependency**

In `arcterm-app/Cargo.toml`, add to `[dependencies]`:

```toml
muda = "0.17"
```

**Step 2: Add new KeyAction variants**

In `arcterm-app/src/keymap.rs`, add these variants to the `KeyAction` enum after the existing ones (before `Consumed`):

```rust
    /// Open cross-pane search (Leader+/).
    CrossPaneSearch,
    // ---- Menu-only actions (no leader-key binding) ----
    /// Copy selected text to clipboard (Cmd+C).
    Copy,
    /// Paste from clipboard (Cmd+V).
    Paste,
    /// Select all text in the active pane's scrollback + visible buffer.
    SelectAll,
    /// Navigate to the next search match (Cmd+G).
    SearchNext,
    /// Navigate to the previous search match (Cmd+Shift+G).
    SearchPrevious,
    /// Clear the scrollback buffer of the active pane (Cmd+K).
    ClearScrollback,
    /// Increase font size by 1pt (Cmd+=).
    IncreaseFontSize,
    /// Decrease font size by 1pt (Cmd+-).
    DecreaseFontSize,
    /// Reset font size to config default (Cmd+0).
    ResetFontSize,
    /// Toggle native fullscreen (Ctrl+Cmd+F).
    ToggleFullScreen,
    /// Minimize the window (Cmd+M).
    Minimize,
    /// Reset all split ratios to 0.5.
    EqualizeSplits,
    /// Next tab (Cmd+Shift+]).
    NextTab,
    /// Previous tab (Cmd+Shift+[).
    PreviousTab,
    /// Reset terminal emulation state.
    ResetTerminal,
    /// Show debug info overlay (version, GPU, config path, pane count).
    ShowDebugInfo,
    /// Open Arcterm help URL in browser.
    OpenHelp,
    /// Open GitHub issues URL in browser.
    ReportIssue,
```

Note: Some of these overlap with functionality already handled inline (Copy/Paste are handled via raw Cmd+C/V in the event loop). The `KeyAction` variants give the menu system a way to trigger them through the same dispatch path.

Also update the exhaustive match in `main.rs` around line 3314 to include the new variants — add them to the catch-all arm:

```rust
        | KeyAction::Copy
        | KeyAction::Paste
        | KeyAction::SelectAll
        | KeyAction::SearchNext
        | KeyAction::SearchPrevious
        | KeyAction::ClearScrollback
        | KeyAction::IncreaseFontSize
        | KeyAction::DecreaseFontSize
        | KeyAction::ResetFontSize
        | KeyAction::ToggleFullScreen
        | KeyAction::Minimize
        | KeyAction::EqualizeSplits
        | KeyAction::NextTab
        | KeyAction::PreviousTab
        | KeyAction::ResetTerminal
        | KeyAction::ShowDebugInfo
        | KeyAction::OpenHelp
        | KeyAction::ReportIssue
```

**Step 3: Verify compilation**

```bash
cd /Users/lgbarn/Personal/arcterm && cargo check -p arcterm-app
```

Expected: compiles with no errors.

**Step 4: Commit**

```bash
git add arcterm-app/Cargo.toml arcterm-app/src/keymap.rs arcterm-app/src/main.rs
git commit -m "feat(menu): add muda dependency and new KeyAction variants for menu bar"
```

---

<task id="2" name="Create menu.rs — menu construction and ID mapping">
  <description>Create the menu.rs module that builds the full native menu bar using muda, assigns MenuIds to each item, and provides a function to map MenuId → KeyAction.</description>
  <files>
    <create>arcterm-app/src/menu.rs</create>
    <modify>arcterm-app/src/main.rs:176-196</modify>
  </files>
  <steps>
    <step>Create menu.rs with build_menu_bar() and menu_action() functions</step>
    <step>Add mod menu to main.rs module declarations</step>
    <step>Verify it compiles</step>
    <step>Commit</step>
  </steps>
  <verification>
    <command>cd /Users/lgbarn/Personal/arcterm && cargo check -p arcterm-app 2>&1 | tail -5</command>
    <expected>Finished</expected>
  </verification>
</task>

### Task 2: Create menu.rs — menu construction and ID mapping

**Files:**
- Create: `arcterm-app/src/menu.rs`
- Modify: `arcterm-app/src/main.rs:176-196` (add `mod menu;`)

**Step 1: Create `arcterm-app/src/menu.rs`**

```rust
//! Native menu bar construction and event mapping.
//!
//! Uses the `muda` crate to build a macOS-native menu bar. Each menu item
//! is assigned a stable string ID that maps to a [`KeyAction`].

use std::collections::HashMap;

use muda::{
    accelerator::{Accelerator, Code, Modifiers},
    Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};

use crate::keymap::KeyAction;
use crate::layout::Direction;

/// Holds the menu bar and the ID → action mapping.
pub struct AppMenu {
    pub menu: Menu,
    action_map: HashMap<MenuId, KeyAction>,
}

impl AppMenu {
    /// Build the full menu bar and return the `AppMenu`.
    pub fn new() -> Self {
        let menu = Menu::new();
        let mut action_map = HashMap::new();

        // ── Shell ──────────────────────────────────────────────────────
        let shell = Submenu::new("Shell", true);

        let new_tab = MenuItem::new(
            "New Tab",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyT)),
        );
        action_map.insert(new_tab.id().clone(), KeyAction::NewTab);

        let split_right = MenuItem::new(
            "Split Right",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyD)),
        );
        action_map.insert(
            split_right.id().clone(),
            KeyAction::Split(crate::layout::Axis::Horizontal),
        );

        let split_down = MenuItem::new(
            "Split Down",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::KeyD,
            )),
        );
        action_map.insert(
            split_down.id().clone(),
            KeyAction::Split(crate::layout::Axis::Vertical),
        );

        let close_pane = MenuItem::new(
            "Close Pane",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyW)),
        );
        action_map.insert(close_pane.id().clone(), KeyAction::ClosePane);

        let close_tab = MenuItem::new(
            "Close Tab",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::KeyW,
            )),
        );
        action_map.insert(close_tab.id().clone(), KeyAction::CloseTab);

        let reset_terminal = MenuItem::new("Reset Terminal", true, None);
        action_map.insert(reset_terminal.id().clone(), KeyAction::ResetTerminal);

        shell.append_items(&[
            &new_tab,
            &PredefinedMenuItem::separator(),
            &split_right,
            &split_down,
            &PredefinedMenuItem::separator(),
            &close_pane,
            &close_tab,
            &PredefinedMenuItem::separator(),
            &reset_terminal,
        ]).expect("failed to build Shell menu");

        // ── Edit ───────────────────────────────────────────────────────
        let edit = Submenu::new("Edit", true);

        let copy = MenuItem::new(
            "Copy",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyC)),
        );
        action_map.insert(copy.id().clone(), KeyAction::Copy);

        let paste = MenuItem::new(
            "Paste",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyV)),
        );
        action_map.insert(paste.id().clone(), KeyAction::Paste);

        let select_all = MenuItem::new(
            "Select All",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyA)),
        );
        action_map.insert(select_all.id().clone(), KeyAction::SelectAll);

        let find = MenuItem::new(
            "Find...",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyF)),
        );
        action_map.insert(find.id().clone(), KeyAction::CrossPaneSearch);

        let find_next = MenuItem::new(
            "Find Next",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyG)),
        );
        action_map.insert(find_next.id().clone(), KeyAction::SearchNext);

        let find_prev = MenuItem::new(
            "Find Previous",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::KeyG,
            )),
        );
        action_map.insert(find_prev.id().clone(), KeyAction::SearchPrevious);

        let clear_scrollback = MenuItem::new(
            "Clear Scrollback",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyK)),
        );
        action_map.insert(clear_scrollback.id().clone(), KeyAction::ClearScrollback);

        let palette = MenuItem::new(
            "Command Palette",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::KeyP,
            )),
        );
        action_map.insert(palette.id().clone(), KeyAction::OpenPalette);

        edit.append_items(&[
            &copy,
            &paste,
            &select_all,
            &PredefinedMenuItem::separator(),
            &find,
            &find_next,
            &find_prev,
            &PredefinedMenuItem::separator(),
            &clear_scrollback,
            &PredefinedMenuItem::separator(),
            &palette,
        ]).expect("failed to build Edit menu");

        // ── View ───────────────────────────────────────────────────────
        let view = Submenu::new("View", true);

        let font_inc = MenuItem::new(
            "Increase Font Size",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::Equal)),
        );
        action_map.insert(font_inc.id().clone(), KeyAction::IncreaseFontSize);

        let font_dec = MenuItem::new(
            "Decrease Font Size",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::Minus)),
        );
        action_map.insert(font_dec.id().clone(), KeyAction::DecreaseFontSize);

        let font_reset = MenuItem::new(
            "Reset Font Size",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::Digit0)),
        );
        action_map.insert(font_reset.id().clone(), KeyAction::ResetFontSize);

        let fullscreen = MenuItem::new(
            "Toggle Full Screen",
            true,
            Some(Accelerator::new(
                Some(Modifiers::CONTROL | Modifiers::SUPER),
                Code::KeyF,
            )),
        );
        action_map.insert(fullscreen.id().clone(), KeyAction::ToggleFullScreen);

        let config_overlay = MenuItem::new("Config Overlay", true, None);
        action_map.insert(config_overlay.id().clone(), KeyAction::ReviewOverlay);

        let plan_status = MenuItem::new("Plan Status", true, None);
        action_map.insert(plan_status.id().clone(), KeyAction::TogglePlanView);

        view.append_items(&[
            &font_inc,
            &font_dec,
            &font_reset,
            &PredefinedMenuItem::separator(),
            &fullscreen,
            &PredefinedMenuItem::separator(),
            &config_overlay,
            &plan_status,
        ]).expect("failed to build View menu");

        // ── Window ─────────────────────────────────────────────────────
        let window = Submenu::new("Window", true);

        let minimize = MenuItem::new(
            "Minimize",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyM)),
        );
        action_map.insert(minimize.id().clone(), KeyAction::Minimize);

        let zoom_split = MenuItem::new("Zoom Split", true, None);
        action_map.insert(zoom_split.id().clone(), KeyAction::ToggleZoom);

        let sel_above = MenuItem::new(
            "Select Split Above",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::ALT),
                Code::ArrowUp,
            )),
        );
        action_map.insert(sel_above.id().clone(), KeyAction::NavigatePane(Direction::Up));

        let sel_below = MenuItem::new(
            "Select Split Below",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::ALT),
                Code::ArrowDown,
            )),
        );
        action_map.insert(
            sel_below.id().clone(),
            KeyAction::NavigatePane(Direction::Down),
        );

        let sel_left = MenuItem::new(
            "Select Split Left",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::ALT),
                Code::ArrowLeft,
            )),
        );
        action_map.insert(
            sel_left.id().clone(),
            KeyAction::NavigatePane(Direction::Left),
        );

        let sel_right = MenuItem::new(
            "Select Split Right",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::ALT),
                Code::ArrowRight,
            )),
        );
        action_map.insert(
            sel_right.id().clone(),
            KeyAction::NavigatePane(Direction::Right),
        );

        let equalize = MenuItem::new(
            "Equalize Splits",
            true,
            Some(Accelerator::new(
                Some(Modifiers::CONTROL | Modifiers::SUPER),
                Code::Equal,
            )),
        );
        action_map.insert(equalize.id().clone(), KeyAction::EqualizeSplits);

        let resize_up = MenuItem::new(
            "Resize Split Up",
            true,
            Some(Accelerator::new(
                Some(Modifiers::CONTROL | Modifiers::SUPER),
                Code::ArrowUp,
            )),
        );
        action_map.insert(
            resize_up.id().clone(),
            KeyAction::ResizePane(Direction::Up),
        );

        let resize_down = MenuItem::new(
            "Resize Split Down",
            true,
            Some(Accelerator::new(
                Some(Modifiers::CONTROL | Modifiers::SUPER),
                Code::ArrowDown,
            )),
        );
        action_map.insert(
            resize_down.id().clone(),
            KeyAction::ResizePane(Direction::Down),
        );

        let resize_left = MenuItem::new(
            "Resize Split Left",
            true,
            Some(Accelerator::new(
                Some(Modifiers::CONTROL | Modifiers::SUPER),
                Code::ArrowLeft,
            )),
        );
        action_map.insert(
            resize_left.id().clone(),
            KeyAction::ResizePane(Direction::Left),
        );

        let resize_right = MenuItem::new(
            "Resize Split Right",
            true,
            Some(Accelerator::new(
                Some(Modifiers::CONTROL | Modifiers::SUPER),
                Code::ArrowRight,
            )),
        );
        action_map.insert(
            resize_right.id().clone(),
            KeyAction::ResizePane(Direction::Right),
        );

        let next_tab = MenuItem::new(
            "Next Tab",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::BracketRight,
            )),
        );
        action_map.insert(next_tab.id().clone(), KeyAction::NextTab);

        let prev_tab = MenuItem::new(
            "Previous Tab",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::BracketLeft,
            )),
        );
        action_map.insert(prev_tab.id().clone(), KeyAction::PreviousTab);

        let workspace = MenuItem::new("Workspace Switcher", true, None);
        action_map.insert(workspace.id().clone(), KeyAction::OpenWorkspaceSwitcher);

        window.append_items(&[
            &minimize,
            &PredefinedMenuItem::separator(),
            &zoom_split,
            &sel_above,
            &sel_below,
            &sel_left,
            &sel_right,
            &PredefinedMenuItem::separator(),
            &equalize,
            &resize_up,
            &resize_down,
            &resize_left,
            &resize_right,
            &PredefinedMenuItem::separator(),
            &next_tab,
            &prev_tab,
            &PredefinedMenuItem::separator(),
            &workspace,
        ]).expect("failed to build Window menu");

        // ── Help ───────────────────────────────────────────────────────
        let help = Submenu::new("Help", true);

        let help_item = MenuItem::new("Arcterm Help", true, None);
        action_map.insert(help_item.id().clone(), KeyAction::OpenHelp);

        let debug_info = MenuItem::new(
            "Show Debug Info",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::ALT),
                Code::KeyI,
            )),
        );
        action_map.insert(debug_info.id().clone(), KeyAction::ShowDebugInfo);

        let report_issue = MenuItem::new("Report Issue", true, None);
        action_map.insert(report_issue.id().clone(), KeyAction::ReportIssue);

        help.append_items(&[
            &help_item,
            &PredefinedMenuItem::separator(),
            &debug_info,
            &report_issue,
        ]).expect("failed to build Help menu");

        // ── Assemble top-level menu bar ────────────────────────────────
        menu.append_items(&[&shell, &edit, &view, &window, &help])
            .expect("failed to assemble menu bar");

        Self { menu, action_map }
    }

    /// Look up the `KeyAction` for a menu event ID.
    /// Returns `None` for predefined items (separator, etc.).
    pub fn action_for_id(&self, id: &MenuId) -> Option<&KeyAction> {
        self.action_map.get(id)
    }
}
```

**Step 2: Add `mod menu;` to main.rs**

In `arcterm-app/src/main.rs`, add after the existing module declarations (around line 196):

```rust
mod menu;
```

**Step 3: Verify compilation**

```bash
cd /Users/lgbarn/Personal/arcterm && cargo check -p arcterm-app
```

Expected: compiles. The menu module is declared but not yet called from resumed/about_to_wait.

**Step 4: Commit**

```bash
git add arcterm-app/src/menu.rs arcterm-app/src/main.rs
git commit -m "feat(menu): create menu.rs with full menu bar construction and ID-to-action mapping"
```

---

<task id="3" name="Integrate menu into resumed() and about_to_wait()">
  <description>Build the menu bar in resumed() after window creation, attach it to the macOS app, store it in AppState, and poll MenuEvents in about_to_wait() to dispatch KeyActions.</description>
  <files>
    <modify>arcterm-app/src/main.rs:1008-1273</modify>
    <modify>arcterm-app/src/main.rs:1275-1420</modify>
  </files>
  <steps>
    <step>Add app_menu field to AppState struct</step>
    <step>Build menu and attach in resumed() after window creation</step>
    <step>Poll MenuEvent in about_to_wait() and dispatch actions</step>
    <step>Verify it compiles</step>
    <step>Commit</step>
  </steps>
  <verification>
    <command>cd /Users/lgbarn/Personal/arcterm && cargo check -p arcterm-app 2>&1 | tail -5</command>
    <expected>Finished</expected>
  </verification>
</task>

### Task 3: Integrate menu into resumed() and about_to_wait()

**Files:**
- Modify: `arcterm-app/src/main.rs`

**Step 1: Add `app_menu` field to `AppState`**

Find the `AppState` struct and add:

```rust
    app_menu: menu::AppMenu,
```

**Step 2: Build menu in `resumed()`**

In the `resumed()` method, after `let window = ...` is created (around line 1034) and before `self.state = Some(AppState { ... })`, add:

```rust
        // Build native menu bar.
        let app_menu = menu::AppMenu::new();
        #[cfg(target_os = "macos")]
        {
            app_menu.menu.init_for_nsapp();
        }
```

Then add `app_menu` to the `AppState` initialization:

```rust
        self.state = Some(AppState {
            // ... existing fields ...
            app_menu,
        });
```

**Step 3: Poll `MenuEvent` in `about_to_wait()`**

At the top of `about_to_wait()`, after `let Some(state) = &mut self.state`, add:

```rust
        // ------------------------------------------------------------------
        // Poll native menu bar events.
        // ------------------------------------------------------------------
        if let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            if let Some(action) = state.app_menu.action_for_id(event.id()) {
                match action {
                    KeyAction::NewTab => {
                        let initial_size = {
                            let win_size = state.window.inner_size();
                            state.renderer.grid_size_for_window(
                                win_size.width,
                                win_size.height,
                                state.window.scale_factor(),
                            )
                        };
                        let new_id = state.spawn_pane(initial_size);
                        let tab_idx = state.tab_manager.tabs.len();
                        state.tab_layouts.push(PaneNode::Leaf { pane_id: new_id });
                        state.tab_manager.tabs.push(tab::Tab { focus: new_id, zoomed: None });
                        state.tab_manager.active = tab_idx;
                        state.window.request_redraw();
                    }
                    KeyAction::ClosePane => {
                        // Reuse the existing close-pane logic by setting a flag
                        // or directly calling the handler. For now, request redraw
                        // and handle via the existing KeyAction dispatch.
                        // (This will be wired in Task 4.)
                    }
                    // All other actions will be handled in Task 4.
                    _ => {
                        log::debug!("menu action: {:?}", action);
                    }
                }
                state.window.request_redraw();
            }
        }
```

Note: The full action dispatch will be extracted into a shared function in Task 4 so both keyboard and menu events use the same code path.

**Step 4: Verify compilation**

```bash
cd /Users/lgbarn/Personal/arcterm && cargo check -p arcterm-app
```

**Step 5: Commit**

```bash
git add arcterm-app/src/main.rs
git commit -m "feat(menu): integrate menu bar into resumed() and poll events in about_to_wait()"
```

---

<task id="4" name="Extract shared action dispatch and wire all existing KeyActions">
  <description>Extract the KeyAction match block from window_event into a shared method on AppState so both keyboard events and menu events dispatch through the same code path. Wire all existing KeyAction variants (NavigatePane, Split, ClosePane, ToggleZoom, ResizePane, OpenPalette, OpenWorkspaceSwitcher, CrossPaneSearch, ReviewOverlay, TogglePlanView).</description>
  <files>
    <modify>arcterm-app/src/main.rs:2713-3324</modify>
  </files>
  <steps>
    <step>Create a dispatch_action() method on AppState that handles all KeyAction variants</step>
    <step>Call dispatch_action() from both the keyboard event handler and the menu event poller</step>
    <step>Verify it compiles</step>
    <step>Commit</step>
  </steps>
  <verification>
    <command>cd /Users/lgbarn/Personal/arcterm && cargo check -p arcterm-app 2>&1 | tail -5</command>
    <expected>Finished</expected>
  </verification>
</task>

### Task 4: Extract shared action dispatch

**Files:**
- Modify: `arcterm-app/src/main.rs`

**Step 1: Create `dispatch_action()` on AppState**

Add a method to `AppState`:

```rust
impl AppState {
    /// Dispatch a KeyAction from either keyboard or menu event.
    /// Returns true if a redraw is needed.
    fn dispatch_action(&mut self, action: &KeyAction) -> bool {
        match action {
            // Wire each existing KeyAction variant using the same logic
            // currently in the window_event match block.
            // Move the match arms from window_event into this method.
            // For menu-only actions, add new handlers.
            _ => false,
        }
    }
}
```

The key insight: the existing keyboard dispatch in `window_event` at line ~2713 has a large match block. Factor the non-PTY-forward arms (NavigatePane, Split, ClosePane, ToggleZoom, ResizePane, NewTab, SwitchTab, CloseTab, OpenPalette, OpenWorkspaceSwitcher, SaveWorkspace, JumpToAiPane, TogglePlanView, CrossPaneSearch, ReviewOverlay) into `dispatch_action()`.

`KeyAction::Forward(bytes)` stays in the keyboard handler since it requires PTY write and the neovim-aware logic.

**Step 2: Call from both keyboard and menu event handlers**

In the keyboard handler, after matching `KeyAction::Forward`:
```rust
_ => { state.dispatch_action(&action); }
```

In the menu event poller in `about_to_wait()`:
```rust
if let Some(action) = state.app_menu.action_for_id(event.id()) {
    state.dispatch_action(action);
    state.window.request_redraw();
}
```

**Step 3: Verify compilation**

```bash
cd /Users/lgbarn/Personal/arcterm && cargo check -p arcterm-app
```

**Step 4: Commit**

```bash
git add arcterm-app/src/main.rs
git commit -m "refactor(menu): extract dispatch_action() for shared keyboard+menu action handling"
```

---

<task id="5" name="Implement new menu-only actions">
  <description>Implement the handlers for all new KeyAction variants: Copy, Paste, SelectAll, SearchNext, SearchPrevious, ClearScrollback, IncreaseFontSize, DecreaseFontSize, ResetFontSize, ToggleFullScreen, Minimize, EqualizeSplits, NextTab, PreviousTab, ResetTerminal, ShowDebugInfo, OpenHelp, ReportIssue.</description>
  <files>
    <modify>arcterm-app/src/main.rs</modify>
    <modify>arcterm-app/src/layout.rs</modify>
  </files>
  <steps>
    <step>Implement Copy/Paste/SelectAll in dispatch_action</step>
    <step>Implement SearchNext/SearchPrevious using existing search overlay methods</step>
    <step>Implement ClearScrollback</step>
    <step>Implement font size adjustment (IncreaseFontSize, DecreaseFontSize, ResetFontSize)</step>
    <step>Implement ToggleFullScreen and Minimize using winit window methods</step>
    <step>Add equalize_splits() to PaneNode in layout.rs</step>
    <step>Implement EqualizeSplits, NextTab, PreviousTab in dispatch_action</step>
    <step>Implement ResetTerminal</step>
    <step>Implement OpenHelp and ReportIssue (open URLs via open command)</step>
    <step>Implement ShowDebugInfo (log to terminal or simple overlay)</step>
    <step>Verify it compiles and test manually</step>
    <step>Commit</step>
  </steps>
  <verification>
    <command>cd /Users/lgbarn/Personal/arcterm && cargo build -p arcterm-app 2>&1 | tail -5</command>
    <expected>Finished</expected>
  </verification>
</task>

### Task 5: Implement new menu-only actions

**Files:**
- Modify: `arcterm-app/src/main.rs` (dispatch_action handlers)
- Modify: `arcterm-app/src/layout.rs` (equalize_splits)

**Step 1: Copy/Paste/SelectAll**

In `dispatch_action()`:

```rust
KeyAction::Copy => {
    if let Some(text) = self.selection.selected_text(&self.panes) {
        if let Some(ref mut cb) = self.clipboard {
            let _ = cb.set_text(&text);
        }
    }
    true
}
KeyAction::Paste => {
    if let Some(ref mut cb) = self.clipboard {
        if let Ok(text) = cb.get_text() {
            let focused = self.tab_manager.active_tab().focus;
            if let Some(terminal) = self.panes.get_mut(&focused) {
                terminal.write_input(text.as_bytes());
            }
        }
    }
    true
}
KeyAction::SelectAll => {
    // Select all is complex — for now, log and skip.
    log::info!("SelectAll: not yet implemented");
    false
}
```

**Step 2: SearchNext/SearchPrevious**

```rust
KeyAction::SearchNext => {
    if let Some(ref mut overlay) = self.search_overlay {
        overlay.next_match();
    }
    true
}
KeyAction::SearchPrevious => {
    if let Some(ref mut overlay) = self.search_overlay {
        overlay.prev_match();
    }
    true
}
```

**Step 3: ClearScrollback**

```rust
KeyAction::ClearScrollback => {
    let focused = self.tab_manager.active_tab().focus;
    if let Some(terminal) = self.panes.get_mut(&focused) {
        let mut term = terminal.lock_term();
        term.grid_mut().clear_history();
    }
    true
}
```

Note: Check the actual `alacritty_terminal` API — the method may be `clear_history()` or `reset()` on the grid. Adjust based on what's available.

**Step 4: Font size**

```rust
KeyAction::IncreaseFontSize => {
    self.config.font_size += 1.0;
    self.renderer.update_font_size(self.config.font_size);
    true
}
KeyAction::DecreaseFontSize => {
    self.config.font_size = (self.config.font_size - 1.0).max(6.0);
    self.renderer.update_font_size(self.config.font_size);
    true
}
KeyAction::ResetFontSize => {
    let default = config::ArctermConfig::load().font_size;
    self.config.font_size = default;
    self.renderer.update_font_size(self.config.font_size);
    true
}
```

Note: `renderer.update_font_size()` may not exist yet. If not, this will need a method added to the renderer that recreates the text atlas at the new size. This can be a follow-up if it's not trivial.

**Step 5: Fullscreen and Minimize**

```rust
KeyAction::ToggleFullScreen => {
    use winit::window::Fullscreen;
    if self.window.fullscreen().is_some() {
        self.window.set_fullscreen(None);
    } else {
        self.window.set_fullscreen(Some(Fullscreen::Borderless(None)));
    }
    true
}
KeyAction::Minimize => {
    self.window.set_minimized(true);
    false
}
```

**Step 6: Equalize splits — add to layout.rs**

Add to `PaneNode`:

```rust
    /// Reset all split ratios in this tree to 0.5.
    pub fn equalize(&mut self) {
        match self {
            PaneNode::Leaf { .. } => {}
            PaneNode::HSplit { ratio, left, right } => {
                *ratio = 0.5;
                left.equalize();
                right.equalize();
            }
            PaneNode::VSplit { ratio, top, bottom } => {
                *ratio = 0.5;
                top.equalize();
                bottom.equalize();
            }
        }
    }
```

**Step 7: EqualizeSplits, NextTab, PreviousTab**

```rust
KeyAction::EqualizeSplits => {
    let active = self.tab_manager.active;
    if let Some(layout) = self.tab_layouts.get_mut(active) {
        layout.equalize();
    }
    true
}
KeyAction::NextTab => {
    let count = self.tab_manager.tabs.len();
    if count > 1 {
        self.tab_manager.active = (self.tab_manager.active + 1) % count;
    }
    true
}
KeyAction::PreviousTab => {
    let count = self.tab_manager.tabs.len();
    if count > 1 {
        self.tab_manager.active = (self.tab_manager.active + count - 1) % count;
    }
    true
}
```

**Step 8: ResetTerminal**

```rust
KeyAction::ResetTerminal => {
    let focused = self.tab_manager.active_tab().focus;
    if let Some(terminal) = self.panes.get_mut(&focused) {
        let mut term = terminal.lock_term();
        term.reset_state();
    }
    true
}
```

**Step 9: OpenHelp, ReportIssue**

```rust
KeyAction::OpenHelp => {
    let _ = std::process::Command::new("open")
        .arg("https://github.com/user/arcterm")  // Replace with actual URL
        .spawn();
    false
}
KeyAction::ReportIssue => {
    let _ = std::process::Command::new("open")
        .arg("https://github.com/user/arcterm/issues")  // Replace with actual URL
        .spawn();
    false
}
```

**Step 10: ShowDebugInfo**

```rust
KeyAction::ShowDebugInfo => {
    let pane_count = self.panes.len();
    let tab_count = self.tab_manager.tabs.len();
    let gpu_info = self.renderer.adapter_info();
    log::info!(
        "Debug: panes={}, tabs={}, gpu={}, config={}",
        pane_count,
        tab_count,
        gpu_info,
        config::config_path().display(),
    );
    // For now just log it. A proper overlay can come later.
    false
}
```

**Step 11: Verify**

```bash
cd /Users/lgbarn/Personal/arcterm && cargo build -p arcterm-app
```

**Step 12: Commit**

```bash
git add arcterm-app/src/main.rs arcterm-app/src/layout.rs
git commit -m "feat(menu): implement all new menu-only action handlers"
```

---

<task id="6" name="Manual smoke test and polish">
  <description>Build and run arcterm, verify the menu bar appears with all 5 menus, test clicking menu items triggers the correct actions, verify accelerator keys display correctly in the menus.</description>
  <files>
    <modify>arcterm-app/src/menu.rs</modify>
    <modify>arcterm-app/src/main.rs</modify>
  </files>
  <steps>
    <step>cargo run -p arcterm-app</step>
    <step>Verify Shell menu: New Tab creates a tab, Split Right splits, Close Pane closes</step>
    <step>Verify Edit menu: Copy/Paste work, Find opens search overlay, Command Palette opens</step>
    <step>Verify View menu: Font size changes, Fullscreen toggles</step>
    <step>Verify Window menu: Split navigation, tab switching, workspace switcher</step>
    <step>Verify Help menu: items are present</step>
    <step>Fix any issues found during testing</step>
    <step>Commit final polish</step>
  </steps>
  <verification>
    <command>cd /Users/lgbarn/Personal/arcterm && cargo build -p arcterm-app 2>&1 | tail -5</command>
    <expected>Finished</expected>
  </verification>
</task>

### Task 6: Manual smoke test and polish

**Steps:**

1. `cargo run -p arcterm-app`
2. Verify each menu appears in the macOS menu bar: Shell, Edit, View, Window, Help
3. Click through each menu item and verify the expected action fires
4. Verify keyboard accelerators display correctly next to menu items (e.g., "Cmd+T" next to New Tab)
5. Test that Leader chords still work (Ctrl+a then n for split, etc.) — no conflicts with Cmd shortcuts
6. Fix any issues, commit

```bash
git add -A
git commit -m "feat(menu): polish menu bar after smoke testing"
```
