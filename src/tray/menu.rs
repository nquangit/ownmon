//! Context menu for the system tray.

use tray_icon::menu::{Menu, MenuId, MenuItem, PredefinedMenuItem};

/// Menu item IDs
pub const MENU_ID_SHOW_STATS: &str = "show_stats";
pub const MENU_ID_EXIT: &str = "exit";

/// Creates the context menu for the system tray.
pub fn create_tray_menu() -> Menu {
    let menu = Menu::new();

    // Show Stats menu item
    let show_stats = MenuItem::with_id(
        MenuId::new(MENU_ID_SHOW_STATS),
        "Show Statistics",
        true,
        None,
    );

    // Separator
    let separator = PredefinedMenuItem::separator();

    // Exit menu item
    let exit = MenuItem::with_id(MenuId::new(MENU_ID_EXIT), "Exit", true, None);

    // Build menu
    let _ = menu.append(&show_stats);
    let _ = menu.append(&separator);
    let _ = menu.append(&exit);

    menu
}
