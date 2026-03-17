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

        // Helper closure — builds a MenuItem, registers it in id_map, and
        // returns it so it can be passed straight to append_items.
        let mut item = |label: &str, accel: Option<Accelerator>, action: KeyAction| -> MenuItem {
            let mi = MenuItem::new(label, true, accel);
            id_map.insert(mi.id().clone(), action);
            mi
        };

        // ----------------------------------------------------------------
        // Shell menu
        // ----------------------------------------------------------------
        let shell = Submenu::new("Shell", true);

        let new_window = item("New Window", Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyN)), KeyAction::NewWindow);
        let new_tab = item("New Tab", Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyT)), KeyAction::NewTab);
        let split_right = item("Split Right", Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyD)), KeyAction::Split(Axis::Horizontal));
        let split_down = item("Split Down", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyD)), KeyAction::Split(Axis::Vertical));
        let close_pane = item("Close Pane", Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyW)), KeyAction::ClosePane);
        let close_tab = item("Close Tab", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyW)), KeyAction::CloseTab);
        let reset_terminal = item("Reset Terminal", None, KeyAction::ResetTerminal);

        shell
            .append_items(&[
                &new_window,
                &new_tab,
                &PredefinedMenuItem::separator(),
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

        let copy = item("Copy", Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyC)), KeyAction::Copy);
        let paste = item("Paste", Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyV)), KeyAction::Paste);
        let select_all = item("Select All", Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyA)), KeyAction::SelectAll);
        let find = item("Find...", Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyF)), KeyAction::CrossPaneSearch);
        let find_next = item("Find Next", Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyG)), KeyAction::SearchNext);
        let find_prev = item("Find Previous", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyG)), KeyAction::SearchPrevious);
        let clear_scrollback = item("Clear Scrollback", Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyK)), KeyAction::ClearScrollback);
        let command_palette = item("Command Palette", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyP)), KeyAction::OpenPalette);

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

        let increase_font = item("Increase Font Size", Some(Accelerator::new(Some(Modifiers::SUPER), Code::Equal)), KeyAction::IncreaseFontSize);
        let decrease_font = item("Decrease Font Size", Some(Accelerator::new(Some(Modifiers::SUPER), Code::Minus)), KeyAction::DecreaseFontSize);
        let reset_font = item("Reset Font Size", Some(Accelerator::new(Some(Modifiers::SUPER), Code::Digit0)), KeyAction::ResetFontSize);
        let toggle_fullscreen = item("Toggle Full Screen", Some(Accelerator::new(Some(Modifiers::CONTROL | Modifiers::SUPER), Code::KeyF)), KeyAction::ToggleFullScreen);
        let config_overlay = item("Config Overlay", None, KeyAction::ReviewOverlay);
        let plan_status = item("Plan Status", None, KeyAction::TogglePlanView);

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

        let minimize = item("Minimize", Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyM)), KeyAction::Minimize);
        let zoom_split = item("Zoom Split", None, KeyAction::ToggleZoom);
        let select_above = item("Select Split Above", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::ArrowUp)), KeyAction::NavigatePane(Direction::Up));
        let select_below = item("Select Split Below", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::ArrowDown)), KeyAction::NavigatePane(Direction::Down));
        let select_left = item("Select Split Left", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::ArrowLeft)), KeyAction::NavigatePane(Direction::Left));
        let select_right = item("Select Split Right", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::ArrowRight)), KeyAction::NavigatePane(Direction::Right));
        let equalize = item("Equalize Splits", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::CONTROL), Code::Equal)), KeyAction::EqualizeSplits);
        let resize_up = item("Resize Split Up", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::CONTROL), Code::ArrowUp)), KeyAction::ResizePane(Direction::Up));
        let resize_down = item("Resize Split Down", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::CONTROL), Code::ArrowDown)), KeyAction::ResizePane(Direction::Down));
        let resize_left = item("Resize Split Left", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::CONTROL), Code::ArrowLeft)), KeyAction::ResizePane(Direction::Left));
        let resize_right = item("Resize Split Right", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::CONTROL), Code::ArrowRight)), KeyAction::ResizePane(Direction::Right));
        let next_tab = item("Next Tab", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::BracketRight)), KeyAction::NextTab);
        let prev_tab = item("Previous Tab", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::BracketLeft)), KeyAction::PreviousTab);
        let workspace_switcher = item("Workspace Switcher", None, KeyAction::OpenWorkspaceSwitcher);

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

        let arcterm_help = item("Arcterm Help", None, KeyAction::OpenHelp);
        let debug_info = item("Show Debug Info", Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::KeyI)), KeyAction::ShowDebugInfo);
        let report_issue = item("Report Issue", None, KeyAction::ReportIssue);

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
