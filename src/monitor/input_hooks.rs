//! Input hook callbacks and atomic counters.
//!
//! This module provides the low-level hook callbacks for keyboard and mouse
//! input tracking. Uses atomic counters for lock-free incrementing to ensure
//! minimal latency in the input pipeline.
//!
//! # Performance Critical
//!
//! The hook callbacks in this module execute synchronously in the Windows
//! input pipeline. Any delay here causes system-wide input lag. The callbacks
//! must:
//! - Use only atomic operations (no locks)
//! - Never allocate memory
//! - Never perform I/O
//! - Always call `CallNextHookEx`

use std::sync::atomic::{AtomicU64, Ordering};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_MOUSEWHEEL,
    WM_RBUTTONDOWN, WM_SYSKEYDOWN,
};

// ============================================================================
// Global Atomic Counters (Lock-Free)
// ============================================================================

/// Total keystroke count since last flush.
pub static KEYSTROKE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Left mouse button click count since last flush.
pub static LEFT_CLICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Right mouse button click count since last flush.
pub static RIGHT_CLICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Middle mouse button click count since last flush.
pub static MIDDLE_CLICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Mouse scroll event count since last flush.
pub static SCROLL_COUNT: AtomicU64 = AtomicU64::new(0);

// ============================================================================
// Hook Callbacks
// ============================================================================

/// Low-level keyboard hook callback.
///
/// Counts WM_KEYDOWN and WM_SYSKEYDOWN events (key presses).
/// WM_KEYUP events are ignored to avoid double-counting.
///
/// # Safety
/// This function is called by Windows from the message pump thread.
/// It must be extremely fast and never block.
pub unsafe extern "system" fn keyboard_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 {
        let msg = wparam.0 as u32;

        // Only count key-down events
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            // Optional: access the key info if needed in the future
            // let _kb_struct = &*(lparam.0 as *const KBDLLHOOKSTRUCT);

            KEYSTROKE_COUNT.fetch_add(1, Ordering::Relaxed);
        }
    }

    // CRITICAL: Always call next hook in chain
    CallNextHookEx(None, code, wparam, lparam)
}

/// Low-level mouse hook callback.
///
/// Counts mouse button clicks (left, right, middle) and scroll events.
/// Mouse movement events are ignored for performance.
///
/// # Safety
/// This function is called by Windows from the message pump thread.
/// It must be extremely fast and never block.
pub unsafe extern "system" fn mouse_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 {
        let msg = wparam.0 as u32;

        match msg {
            WM_LBUTTONDOWN => {
                LEFT_CLICK_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            WM_RBUTTONDOWN => {
                RIGHT_CLICK_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            WM_MBUTTONDOWN => {
                MIDDLE_CLICK_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            WM_MOUSEWHEEL => {
                SCROLL_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            _ => {
                // Ignore mouse movement and other events
            }
        }
    }

    // CRITICAL: Always call next hook in chain
    CallNextHookEx(None, code, wparam, lparam)
}

// ============================================================================
// Counter Access Functions
// ============================================================================

/// Atomically reads and resets the keystroke counter.
///
/// Returns the count accumulated since the last flush.
#[inline]
pub fn flush_keystroke_count() -> u64 {
    KEYSTROKE_COUNT.swap(0, Ordering::Relaxed)
}

/// Atomically reads and resets all click counters.
///
/// Returns (left_clicks, right_clicks, middle_clicks).
#[inline]
pub fn flush_click_counts() -> (u64, u64, u64) {
    (
        LEFT_CLICK_COUNT.swap(0, Ordering::Relaxed),
        RIGHT_CLICK_COUNT.swap(0, Ordering::Relaxed),
        MIDDLE_CLICK_COUNT.swap(0, Ordering::Relaxed),
    )
}

/// Atomically reads and resets the scroll counter.
#[inline]
pub fn flush_scroll_count() -> u64 {
    SCROLL_COUNT.swap(0, Ordering::Relaxed)
}

/// Reads current counter values without resetting them.
///
/// Useful for debugging or status display.
/// Returns (keystrokes, left_clicks, right_clicks, middle_clicks, scrolls).
pub fn peek_all_counts() -> (u64, u64, u64, u64, u64) {
    (
        KEYSTROKE_COUNT.load(Ordering::Relaxed),
        LEFT_CLICK_COUNT.load(Ordering::Relaxed),
        RIGHT_CLICK_COUNT.load(Ordering::Relaxed),
        MIDDLE_CLICK_COUNT.load(Ordering::Relaxed),
        SCROLL_COUNT.load(Ordering::Relaxed),
    )
}

/// Resets all counters to zero.
pub fn reset_all_counts() {
    KEYSTROKE_COUNT.store(0, Ordering::Relaxed);
    LEFT_CLICK_COUNT.store(0, Ordering::Relaxed);
    RIGHT_CLICK_COUNT.store(0, Ordering::Relaxed);
    MIDDLE_CLICK_COUNT.store(0, Ordering::Relaxed);
    SCROLL_COUNT.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_counters_initial_zero() {
        // Note: Tests may interfere with each other if run in parallel
        // due to shared globals. Use reset first.
        reset_all_counts();

        let (keys, left, right, middle, scroll) = peek_all_counts();
        assert_eq!(keys, 0);
        assert_eq!(left, 0);
        assert_eq!(right, 0);
        assert_eq!(middle, 0);
        assert_eq!(scroll, 0);
    }

    #[test]
    fn test_flush_keystroke_count() {
        reset_all_counts();

        KEYSTROKE_COUNT.store(42, Ordering::Relaxed);
        let count = flush_keystroke_count();

        assert_eq!(count, 42);
        assert_eq!(KEYSTROKE_COUNT.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_flush_click_counts() {
        reset_all_counts();

        LEFT_CLICK_COUNT.store(10, Ordering::Relaxed);
        RIGHT_CLICK_COUNT.store(20, Ordering::Relaxed);
        MIDDLE_CLICK_COUNT.store(5, Ordering::Relaxed);

        let (left, right, middle) = flush_click_counts();

        assert_eq!(left, 10);
        assert_eq!(right, 20);
        assert_eq!(middle, 5);

        // All should be reset
        assert_eq!(LEFT_CLICK_COUNT.load(Ordering::Relaxed), 0);
        assert_eq!(RIGHT_CLICK_COUNT.load(Ordering::Relaxed), 0);
        assert_eq!(MIDDLE_CLICK_COUNT.load(Ordering::Relaxed), 0);
    }
}
