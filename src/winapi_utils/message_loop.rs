//! Windows message loop utilities.
//!
//! Provides functions for running and controlling the Windows message pump,
//! which is required for low-level hooks and system tray functionality.

use std::sync::atomic::{AtomicU32, Ordering};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostThreadMessageW, TranslateMessage, MSG, WM_QUIT,
};

/// Stores the main thread ID for cross-thread quit signaling.
static MAIN_THREAD_ID: AtomicU32 = AtomicU32::new(0);

/// Runs the Windows message loop until a WM_QUIT message is received.
///
/// This function blocks the calling thread and pumps messages.
/// It should be called on the main thread after setting up hooks
/// and the system tray.
///
/// # Important
/// - Low-level hooks (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`) require a message loop
///   on the thread that installed them.
/// - If this loop is blocked or too slow, Windows will unhook the hooks.
///
/// # Example
/// ```no_run
/// use ownmon::winapi_utils::run_message_loop;
///
/// // Install hooks first, then run the message loop
/// run_message_loop();
/// // Loop exits when post_quit_message() is called
/// ```
pub fn run_message_loop() {
    // Store the current thread ID so other threads can post WM_QUIT to us
    let thread_id = unsafe { GetCurrentThreadId() };
    MAIN_THREAD_ID.store(thread_id, Ordering::SeqCst);

    tracing::debug!(thread_id, "Message loop starting");

    let mut msg = MSG::default();

    unsafe {
        // GetMessageW returns:
        // - Positive: message retrieved
        // - 0: WM_QUIT received
        // - -1: error occurred
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    tracing::debug!("Message loop exited");
}

/// Posts a WM_QUIT message to terminate the message loop.
///
/// This can be called from any thread - it will correctly post to the
/// main thread's message queue.
///
/// # Arguments
/// * `exit_code` - Exit code to pass with the quit message (typically 0)
///
/// # Example
/// ```no_run
/// use ownmon::winapi_utils::post_quit_message;
///
/// // In a shutdown handler or menu callback:
/// post_quit_message(0);
/// ```
pub fn post_quit_message(exit_code: i32) {
    let main_thread_id = MAIN_THREAD_ID.load(Ordering::SeqCst);

    if main_thread_id != 0 {
        // Post WM_QUIT to the main thread
        unsafe {
            let result = PostThreadMessageW(
                main_thread_id,
                WM_QUIT,
                windows::Win32::Foundation::WPARAM(exit_code as usize),
                windows::Win32::Foundation::LPARAM(0),
            );

            if let Err(e) = result {
                tracing::error!(?e, "Failed to post quit message to main thread");
            } else {
                tracing::debug!(
                    exit_code,
                    thread_id = main_thread_id,
                    "Posted quit message to main thread"
                );
            }
        }
    } else {
        tracing::warn!("Main thread ID not set, cannot post quit message");
    }
}

#[cfg(test)]
mod tests {
    // Message loop tests are difficult to unit test without
    // actually running a message loop. Integration tests would
    // be more appropriate for this module.
}
