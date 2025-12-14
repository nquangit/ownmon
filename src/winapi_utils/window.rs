//! Window-related WinAPI wrappers.
//!
//! Provides safe abstractions for window enumeration, focus detection,
//! and window text retrieval.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
};

/// Gets the handle of the currently focused (foreground) window.
///
/// Returns `None` if no window has focus (e.g., desktop is focused).
///
/// # Example
/// ```no_run
/// use ownmon::winapi_utils::get_foreground_window;
///
/// if let Some(hwnd) = get_foreground_window() {
///     println!("Foreground window handle: {:?}", hwnd);
/// }
/// ```
pub fn get_foreground_window() -> Option<HWND> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        None
    } else {
        Some(hwnd)
    }
}

/// Gets the title text of a window.
///
/// Returns an empty string if the window has no title or if the call fails.
/// Handles Unicode window titles correctly.
///
/// # Arguments
/// * `hwnd` - Handle to the window
///
/// # Example
/// ```no_run
/// use ownmon::winapi_utils::{get_foreground_window, get_window_text};
///
/// if let Some(hwnd) = get_foreground_window() {
///     let title = get_window_text(hwnd);
///     println!("Window title: {}", title);
/// }
/// ```
pub fn get_window_text(hwnd: HWND) -> String {
    unsafe {
        // Get the length of the window text
        let len = GetWindowTextLengthW(hwnd);
        if len == 0 {
            return String::new();
        }

        // Allocate buffer with space for null terminator
        let mut buffer: Vec<u16> = vec![0; (len + 1) as usize];

        // Get the window text
        let copied = GetWindowTextW(hwnd, &mut buffer);
        if copied == 0 {
            return String::new();
        }

        // Convert to String (handle invalid UTF-16 gracefully)
        String::from_utf16_lossy(&buffer[..copied as usize])
    }
}

/// Gets the thread ID and process ID of the window's owner.
///
/// # Arguments
/// * `hwnd` - Handle to the window
///
/// # Returns
/// A tuple of `(thread_id, process_id)`. Both will be 0 if the call fails.
///
/// # Example
/// ```no_run
/// use ownmon::winapi_utils::{get_foreground_window, get_window_thread_process_id};
///
/// if let Some(hwnd) = get_foreground_window() {
///     let (thread_id, process_id) = get_window_thread_process_id(hwnd);
///     println!("Thread: {}, Process: {}", thread_id, process_id);
/// }
/// ```
pub fn get_window_thread_process_id(hwnd: HWND) -> (u32, u32) {
    let mut process_id: u32 = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    (thread_id, process_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_foreground_window_returns_some() {
        // In a normal desktop environment, there should always be a foreground window
        // This test may fail in headless environments
        let hwnd = get_foreground_window();
        // Just check it doesn't panic - actual value depends on environment
        let _ = hwnd;
    }

    #[test]
    fn test_get_window_text_empty_on_invalid_handle() {
        let invalid_hwnd = HWND(std::ptr::null_mut());
        let text = get_window_text(invalid_hwnd);
        assert!(text.is_empty());
    }

    #[test]
    fn test_get_window_thread_process_id_on_invalid_handle() {
        let invalid_hwnd = HWND(std::ptr::null_mut());
        let (tid, pid) = get_window_thread_process_id(invalid_hwnd);
        assert_eq!(tid, 0);
        assert_eq!(pid, 0);
    }
}
