//! System tray module.
//!
//! Provides system tray icon and context menu functionality for
//! OwnMon to run silently in the background.

pub mod icon;
pub mod menu;

pub use icon::*;
pub use menu::*;

use crate::store::ACTIVITY_STORE;
use crate::winapi_utils::post_quit_message;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tray_icon::menu::MenuEvent;
use tray_icon::{TrayIcon, TrayIconBuilder};

/// Sets up the system tray icon and menu.
///
/// # Arguments
/// * `shutdown` - Atomic flag to signal application shutdown
///
/// # Returns
/// The `TrayIcon` instance. Keep this alive for the tray to remain visible.
pub fn setup_tray(shutdown: Arc<AtomicBool>) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let icon = create_default_icon()?;
    let menu = create_tray_menu();

    let tray = TrayIconBuilder::new()
        .with_tooltip("OwnMon - Activity Monitor")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .build()?;

    // Spawn menu event handler
    spawn_menu_handler(shutdown);

    tracing::info!("System tray initialized");
    Ok(tray)
}

/// Spawns a thread to handle menu events.
fn spawn_menu_handler(shutdown: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let receiver = MenuEvent::receiver();

        loop {
            if let Ok(event) = receiver.try_recv() {
                handle_menu_event(&event.id.0, &shutdown);
            }

            if shutdown.load(Ordering::Relaxed) {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
}

/// Handles a menu item click.
fn handle_menu_event(menu_id: &str, shutdown: &Arc<AtomicBool>) {
    match menu_id {
        "show_stats" => {
            show_stats();
        }
        "exit" => {
            tracing::info!("Exit requested from tray menu");
            shutdown.store(true, Ordering::SeqCst);
            post_quit_message(0);
        }
        _ => {
            tracing::debug!(menu_id, "Unknown menu event");
        }
    }
}

/// Shows current statistics.
fn show_stats() {
    if let Ok(store) = ACTIVITY_STORE.read() {
        let summary = store.get_daily_summary();

        println!();
        println!("════════════════════════════════════════");
        println!("📊 OwnMon Statistics");
        println!("════════════════════════════════════════");
        println!("Sessions:    {}", summary.session_count);
        println!("Unique Apps: {}", summary.app_count);
        println!("Keystrokes:  {}", summary.total_keystrokes);
        println!("Clicks:      {}", summary.total_clicks);
        println!("Focus Time:  {}s", summary.total_focus_time_secs);
        println!("════════════════════════════════════════");
        println!();
    }
}
