//! Native menu bar construction and menu-ID-to-KeyAction mapping.
//!
//! # Design
//!
//! [`AppMenu`] owns the top-level [`muda::Menu`] and a lookup table that maps
//! each [`muda::MenuId`] to the [`crate::keymap::KeyAction`] it represents.
//! The event-loop integration only needs to call [`AppMenu::action_for_id`]
//! when a [`muda::MenuEvent`] arrives.

use std::collections::HashMap;

use muda::{
    accelerator::{Accelerator, Code, Modifiers},
    Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};

use crate::keymap::KeyAction;
use crate::layout::{Axis, Direction};

// ---------------------------------------------------------------------------
// AppMenu
// ---------------------------------------------------------------------------

/// Holds the native menu bar and the ID→action lookup table.
pub struct AppMenu {
    /// The top-level menu bar installed on the application.
    pub menu: Menu,
    /// Maps each [`MenuId`] to the action it should dispatch.
    id_map: HashMap<MenuId, KeyAction>,
}

impl AppMenu {
    /// Build the full menu bar with all submenus and register every item in
    /// the `id_map`.
    pub fn new() -> Self {
        let menu = Menu::new();
        let mut id_map: HashMap<MenuId, KeyAction> = HashMap::new();

        // Helper closure — registers the item's id and returns a reference to
        // the item so it can be appended immediately.
        //
        // We can't return the item itself from a closure and then use it again,
        // so we use a macro-like pattern: build, register, then append in-line.

        // ----------------------------------------------------------------
        // Shell menu
        // ----------------------------------------------------------------
        let shell = Submenu::new("Shell", true);

        let new_tab = MenuItem::new(
            "New Tab",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyT)),
        );
        let split_right = MenuItem::new(
            "Split Right",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyD)),
        );
        let split_down = MenuItem::new(
            "Split Down",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::KeyD,
            )),
        );
        let close_pane = MenuItem::new(
            "Close Pane",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyW)),
        );
        let close_tab = MenuItem::new(
            "Close Tab",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::KeyW,
            )),
        );
        let reset_terminal = MenuItem::new("Reset Terminal", true, None);

        id_map.insert(new_tab.id().clone(), KeyAction::NewTab);
        id_map.insert(split_right.id().clone(), KeyAction::Split(Axis::Horizontal));
        id_map.insert(split_down.id().clone(), KeyAction::Split(Axis::Vertical));
        id_map.insert(close_pane.id().clone(), KeyAction::ClosePane);
        id_map.insert(close_tab.id().clone(), KeyAction::CloseTab);
        id_map.insert(reset_terminal.id().clone(), KeyAction::ResetTerminal);

        shell
            .append_items(&[
                &new_tab,
                &split_right,
                &split_down,
                &PredefinedMenuItem::separator(),
                &close_pane,
                &close_tab,
                &PredefinedMenuItem::separator(),
                &reset_terminal,
            ])
            .expect("shell menu append_items failed");

        // ----------------------------------------------------------------
        // Edit menu
        // ----------------------------------------------------------------
        let edit = Submenu::new("Edit", true);

        let copy = MenuItem::new(
            "Copy",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyC)),
        );
        let paste = MenuItem::new(
            "Paste",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyV)),
        );
        let select_all = MenuItem::new(
            "Select All",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyA)),
        );
        let find = MenuItem::new(
            "Find...",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyF)),
        );
        let find_next = MenuItem::new(
            "Find Next",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyG)),
        );
        let find_prev = MenuItem::new(
            "Find Previous",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::KeyG,
            )),
        );
        let clear_scrollback = MenuItem::new(
            "Clear Scrollback",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyK)),
        );
        let command_palette = MenuItem::new(
            "Command Palette",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::KeyP,
            )),
        );

        id_map.insert(copy.id().clone(), KeyAction::Copy);
        id_map.insert(paste.id().clone(), KeyAction::Paste);
        id_map.insert(select_all.id().clone(), KeyAction::SelectAll);
        id_map.insert(find.id().clone(), KeyAction::CrossPaneSearch);
        id_map.insert(find_next.id().clone(), KeyAction::SearchNext);
        id_map.insert(find_prev.id().clone(), KeyAction::SearchPrevious);
        id_map.insert(clear_scrollback.id().clone(), KeyAction::ClearScrollback);
        id_map.insert(command_palette.id().clone(), KeyAction::OpenPalette);

        edit.append_items(&[
            &copy,
            &paste,
            &PredefinedMenuItem::separator(),
            &select_all,
            &PredefinedMenuItem::separator(),
            &find,
            &find_next,
            &find_prev,
            &PredefinedMenuItem::separator(),
            &clear_scrollback,
            &PredefinedMenuItem::separator(),
            &command_palette,
        ])
        .expect("edit menu append_items failed");

        // ----------------------------------------------------------------
        // View menu
        // ----------------------------------------------------------------
        let view = Submenu::new("View", true);

        let increase_font = MenuItem::new(
            "Increase Font Size",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::Equal)),
        );
        let decrease_font = MenuItem::new(
            "Decrease Font Size",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::Minus)),
        );
        let reset_font = MenuItem::new(
            "Reset Font Size",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::Digit0)),
        );
        let toggle_fullscreen = MenuItem::new(
            "Toggle Full Screen",
            true,
            Some(Accelerator::new(
                Some(Modifiers::CONTROL | Modifiers::SUPER),
                Code::KeyF,
            )),
        );
        let config_overlay = MenuItem::new("Config Overlay", true, None);
        let plan_status = MenuItem::new("Plan Status", true, None);

        id_map.insert(increase_font.id().clone(), KeyAction::IncreaseFontSize);
        id_map.insert(decrease_font.id().clone(), KeyAction::DecreaseFontSize);
        id_map.insert(reset_font.id().clone(), KeyAction::ResetFontSize);
        id_map.insert(toggle_fullscreen.id().clone(), KeyAction::ToggleFullScreen);
        id_map.insert(config_overlay.id().clone(), KeyAction::ReviewOverlay);
        id_map.insert(plan_status.id().clone(), KeyAction::TogglePlanView);

        view.append_items(&[
            &increase_font,
            &decrease_font,
            &reset_font,
            &PredefinedMenuItem::separator(),
            &toggle_fullscreen,
            &PredefinedMenuItem::separator(),
            &config_overlay,
            &plan_status,
        ])
        .expect("view menu append_items failed");

        // ----------------------------------------------------------------
        // Window menu
        // ----------------------------------------------------------------
        let window = Submenu::new("Window", true);

        let minimize = MenuItem::new(
            "Minimize",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyM)),
        );
        let zoom_split = MenuItem::new("Zoom Split", true, None);

        let select_above = MenuItem::new(
            "Select Split Above",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::ALT),
                Code::ArrowUp,
            )),
        );
        let select_below = MenuItem::new(
            "Select Split Below",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::ALT),
                Code::ArrowDown,
            )),
        );
        let select_left = MenuItem::new(
            "Select Split Left",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::ALT),
                Code::ArrowLeft,
            )),
        );
        let select_right = MenuItem::new(
            "Select Split Right",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::ALT),
                Code::ArrowRight,
            )),
        );

        let equalize = MenuItem::new(
            "Equalize Splits",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::CONTROL),
                Code::Equal,
            )),
        );

        let resize_up = MenuItem::new(
            "Resize Split Up",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::CONTROL),
                Code::ArrowUp,
            )),
        );
        let resize_down = MenuItem::new(
            "Resize Split Down",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::CONTROL),
                Code::ArrowDown,
            )),
        );
        let resize_left = MenuItem::new(
            "Resize Split Left",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::CONTROL),
                Code::ArrowLeft,
            )),
        );
        let resize_right = MenuItem::new(
            "Resize Split Right",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::CONTROL),
                Code::ArrowRight,
            )),
        );

        let next_tab = MenuItem::new(
            "Next Tab",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::BracketRight,
            )),
        );
        let prev_tab = MenuItem::new(
            "Previous Tab",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::BracketLeft,
            )),
        );
        let workspace_switcher = MenuItem::new("Workspace Switcher", true, None);

        id_map.insert(minimize.id().clone(), KeyAction::Minimize);
        id_map.insert(zoom_split.id().clone(), KeyAction::ToggleZoom);
        id_map.insert(
            select_above.id().clone(),
            KeyAction::NavigatePane(Direction::Up),
        );
        id_map.insert(
            select_below.id().clone(),
            KeyAction::NavigatePane(Direction::Down),
        );
        id_map.insert(
            select_left.id().clone(),
            KeyAction::NavigatePane(Direction::Left),
        );
        id_map.insert(
            select_right.id().clone(),
            KeyAction::NavigatePane(Direction::Right),
        );
        id_map.insert(equalize.id().clone(), KeyAction::EqualizeSplits);
        id_map.insert(
            resize_up.id().clone(),
            KeyAction::ResizePane(Direction::Up),
        );
        id_map.insert(
            resize_down.id().clone(),
            KeyAction::ResizePane(Direction::Down),
        );
        id_map.insert(
            resize_left.id().clone(),
            KeyAction::ResizePane(Direction::Left),
        );
        id_map.insert(
            resize_right.id().clone(),
            KeyAction::ResizePane(Direction::Right),
        );
        id_map.insert(next_tab.id().clone(), KeyAction::NextTab);
        id_map.insert(prev_tab.id().clone(), KeyAction::PreviousTab);
        id_map.insert(
            workspace_switcher.id().clone(),
            KeyAction::OpenWorkspaceSwitcher,
        );

        window
            .append_items(&[
                &minimize,
                &zoom_split,
                &PredefinedMenuItem::separator(),
                &select_above,
                &select_below,
                &select_left,
                &select_right,
                &PredefinedMenuItem::separator(),
                &equalize,
                &PredefinedMenuItem::separator(),
                &resize_up,
                &resize_down,
                &resize_left,
                &resize_right,
                &PredefinedMenuItem::separator(),
                &next_tab,
                &prev_tab,
                &PredefinedMenuItem::separator(),
                &workspace_switcher,
            ])
            .expect("window menu append_items failed");

        // ----------------------------------------------------------------
        // Help menu
        // ----------------------------------------------------------------
        let help = Submenu::new("Help", true);

        let arcterm_help = MenuItem::new("Arcterm Help", true, None);
        let debug_info = MenuItem::new(
            "Show Debug Info",
            true,
            Some(Accelerator::new(
                Some(Modifiers::SUPER | Modifiers::ALT),
                Code::KeyI,
            )),
        );
        let report_issue = MenuItem::new("Report Issue", true, None);

        id_map.insert(arcterm_help.id().clone(), KeyAction::OpenHelp);
        id_map.insert(debug_info.id().clone(), KeyAction::ShowDebugInfo);
        id_map.insert(report_issue.id().clone(), KeyAction::ReportIssue);

        help.append_items(&[&arcterm_help, &debug_info, &PredefinedMenuItem::separator(), &report_issue])
            .expect("help menu append_items failed");

        // ----------------------------------------------------------------
        // Assemble top-level bar
        // ----------------------------------------------------------------
        menu.append_items(&[&shell, &edit, &view, &window, &help])
            .expect("menu bar append_items failed");

        Self { menu, id_map }
    }

    /// Look up the [`KeyAction`] associated with a menu item ID.
    ///
    /// Returns `None` for separator items and predefined items that are not
    /// mapped (e.g. items appended by the OS).
    pub fn action_for_id(&self, id: &MenuId) -> Option<&KeyAction> {
        self.id_map.get(id)
    }
}

impl Default for AppMenu {
    fn default() -> Self {
        Self::new()
    }
}
