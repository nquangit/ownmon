//! OwnMon - Windows Activity Monitor
//!
//! Phase 5: Full application with system tray integration.
//! The application runs silently with a system tray icon.

use ownmon::monitor::*;
use ownmon::store::ACTIVITY_STORE;
use ownmon::tray::setup_tray;
use ownmon::winapi_utils::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("ownmon=info")),
        )
        .init();

    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║              OwnMon - Activity Monitor                     ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Initialize database early
    println!("🔧 Initializing database...");
    let _ = &*ownmon::store::DATABASE; // Trigger lazy init
    println!("   ✓ Database ready");

    // Start HTTP server
    println!("🔧 Starting HTTP server...");
    let broadcast_tx = ownmon::server::start_server();
    // Store broadcast sender globally for poller to use
    let _ = ownmon::store::BROADCAST_TX.set(broadcast_tx);
    println!(
        "   ✓ HTTP server listening on http://127.0.0.1:{}",
        ownmon::server::DEFAULT_PORT
    );

    // Shutdown signal
    let shutdown = Arc::new(AtomicBool::new(false));

    // Setup system tray (before hooks to avoid issues with message loop)
    println!("🔧 Setting up system tray...");
    let _tray = match setup_tray(Arc::clone(&shutdown)) {
        Ok(tray) => {
            println!("   ✓ System tray icon created");
            Some(tray)
        }
        Err(e) => {
            println!("   ⚠ Failed to create system tray: {}", e);
            println!("   Continuing without tray...");
            None
        }
    };

    // Handle Ctrl+C as backup
    let shutdown_ctrlc = Arc::clone(&shutdown);
    ctrlc::set_handler(move || {
        println!("\n🛑 Shutdown signal received...");
        shutdown_ctrlc.store(true, Ordering::SeqCst);
        post_quit_message(0);
    })?;

    // Start polling thread
    println!("🔧 Starting window polling...");
    let shutdown_poller = Arc::clone(&shutdown);
    let polling_handle = spawn_polling_thread(shutdown_poller, PollerConfig::default());
    println!("   ✓ Polling thread started");

    // Install hooks
    println!("🔧 Installing input hooks...");
    let _keyboard_hook = HookGuard::install_keyboard_hook(Some(keyboard_hook_proc))?;
    let _mouse_hook = HookGuard::install_mouse_hook(Some(mouse_hook_proc))?;
    println!("   ✓ Keyboard and mouse hooks installed");

    println!();
    println!("════════════════════════════════════════════════════════════════");
    println!("🎯 OwnMon is now running in the system tray!");
    println!("   • Right-click the tray icon for options");
    println!("   • Select 'Show Statistics' to view activity");
    println!("   • Select 'Exit' or press Ctrl+C to quit");
    println!();
    println!(
        "🌐 API available at http://127.0.0.1:{}",
        ownmon::server::DEFAULT_PORT
    );
    println!("   • GET /api/stats    - Today's statistics");
    println!("   • GET /api/sessions - Recent sessions");
    println!("   • WS  /ws           - Real-time updates");
    println!("════════════════════════════════════════════════════════════════");
    println!();

    // Optional: spawn status display thread
    let shutdown_display = Arc::clone(&shutdown);
    thread::spawn(move || {
        let mut last_count = 0usize;
        while !shutdown_display.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_secs(30));

            if let Ok(store) = ACTIVITY_STORE.read() {
                let count = store.session_count();
                if count != last_count {
                    if let Some(session) = &store.current_session {
                        tracing::info!(
                            app = %session.process_name,
                            sessions = count,
                            keys = session.keystrokes,
                            "Activity update"
                        );
                    }
                    last_count = count;
                }
            }
        }
    });

    // Run the Windows message loop (required for hooks and tray)
    tracing::info!("Running message loop...");
    run_message_loop();

    // Cleanup
    println!("\n⏳ Shutting down...");
    shutdown.store(true, Ordering::SeqCst);
    polling_handle.join().expect("Polling thread panicked");

    // Save all pending data to database
    println!("💾 Saving data to database...");
    ownmon::store::finalize_and_save();

    // Print final summary
    print_summary();

    println!("\n👋 OwnMon has exited. Goodbye!");
    Ok(())
}

fn print_summary() {
    if let Ok(store) = ACTIVITY_STORE.read() {
        let summary = store.get_daily_summary();

        println!();
        println!("════════════════════════════════════════════════════════════════");
        println!("📊 Final Activity Summary");
        println!("════════════════════════════════════════════════════════════════");
        println!("   Sessions:      {}", summary.session_count);
        println!("   Unique Apps:   {}", summary.app_count);
        println!("   Keystrokes:    {}", summary.total_keystrokes);
        println!("   Mouse Clicks:  {}", summary.total_clicks);
        println!("   Focus Time:    {}s", summary.total_focus_time_secs);

        if !store.completed_sessions.is_empty() {
            println!();
            println!("Top Applications:");
            let stats = store.compute_application_stats();
            let mut sorted: Vec<_> = stats.into_iter().collect();
            sorted.sort_by(|a, b| {
                b.1.total_focus_duration_secs
                    .cmp(&a.1.total_focus_duration_secs)
            });

            for (i, (name, stat)) in sorted.iter().take(5).enumerate() {
                println!(
                    "   {}. {} - {}s, {} keys, {} clicks",
                    i + 1,
                    name,
                    stat.total_focus_duration_secs,
                    stat.total_keystrokes,
                    stat.total_clicks
                );
            }
        }

        // Media summary
        let media_time = store.total_media_time_secs();
        if media_time > 0 || store.current_media.is_some() || !store.media_history.is_empty() {
            println!();
            println!("🎵 Media Listened:");
            println!("   Total Time:    {}s", media_time);
            println!(
                "   Tracks:        {}",
                store.media_history.len() + if store.current_media.is_some() { 1 } else { 0 }
            );

            // Show current media
            if let Some(ref media) = store.current_media {
                println!();
                println!("   ▶ Now Playing:");
                println!(
                    "      {} - {}",
                    media.media_info.title, media.media_info.artist
                );
                if !media.media_info.album.is_empty() {
                    println!("      Album: {}", media.media_info.album);
                }
            }

            // Show recent media history
            let recent_media = store.get_media_summary();
            if !recent_media.is_empty() {
                println!();
                println!("   Recent Tracks:");
                for (i, media) in recent_media.iter().take(5).enumerate() {
                    println!(
                        "      {}. {} - {} ({}s)",
                        i + 1,
                        media.media_info.title,
                        media.media_info.artist,
                        media.duration_secs()
                    );
                }
            }
        }

        println!("════════════════════════════════════════════════════════════════");
    }
}
