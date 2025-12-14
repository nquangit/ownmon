//! Windows hook installation and management.
//!
//! Provides RAII wrappers for Windows low-level hooks to ensure
//! proper cleanup when hooks go out of scope.

use windows::Win32::Foundation::LRESULT;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, HOOKPROC, WH_KEYBOARD_LL,
    WH_MOUSE_LL, WINDOWS_HOOK_ID,
};

/// RAII guard for a Windows hook.
///
/// Automatically calls `UnhookWindowsHookEx` when dropped to prevent
/// hook leaks and ensure proper cleanup.
///
/// # Example
/// ```ignore
/// // Hook is automatically unhooked when guard goes out of scope
/// {
///     let guard = HookGuard::install_keyboard_hook(Some(my_callback))?;
///     // ... hook is active ...
/// } // UnhookWindowsHookEx called here
/// ```
pub struct HookGuard {
    handle: HHOOK,
    hook_type: &'static str,
}

impl HookGuard {
    /// Creates a new HookGuard from a raw handle.
    fn new(handle: HHOOK, hook_type: &'static str) -> Self {
        tracing::info!(hook_type, "Hook installed successfully");
        Self { handle, hook_type }
    }

    /// Returns the raw hook handle.
    pub fn handle(&self) -> HHOOK {
        self.handle
    }

    /// Installs a low-level keyboard hook.
    ///
    /// The callback function must have the signature:
    /// `unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT`
    ///
    /// # Important
    /// - The callback must be extremely fast (< 1ms)
    /// - Always call `CallNextHookEx` at the end of the callback
    /// - The installing thread must run a message pump
    pub fn install_keyboard_hook(callback: HOOKPROC) -> windows::core::Result<Self> {
        let handle = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, callback, None, 0)? };
        Ok(Self::new(handle, "keyboard_ll"))
    }

    /// Installs a low-level mouse hook.
    ///
    /// The callback function must have the signature:
    /// `unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT`
    ///
    /// # Important
    /// - The callback must be extremely fast (< 1ms)
    /// - Always call `CallNextHookEx` at the end of the callback
    /// - The installing thread must run a message pump
    pub fn install_mouse_hook(callback: HOOKPROC) -> windows::core::Result<Self> {
        let handle = unsafe { SetWindowsHookExW(WH_MOUSE_LL, callback, None, 0)? };
        Ok(Self::new(handle, "mouse_ll"))
    }

    /// Generic hook installation for any hook type.
    pub fn install(
        hook_id: WINDOWS_HOOK_ID,
        callback: HOOKPROC,
        hook_name: &'static str,
    ) -> windows::core::Result<Self> {
        let handle = unsafe { SetWindowsHookExW(hook_id, callback, None, 0)? };
        Ok(Self::new(handle, hook_name))
    }
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        let result = unsafe { UnhookWindowsHookEx(self.handle) };
        match result {
            Ok(_) => tracing::info!(hook_type = self.hook_type, "Hook uninstalled successfully"),
            Err(e) => tracing::error!(
                hook_type = self.hook_type,
                error = ?e,
                "Failed to unhook"
            ),
        }
    }
}

/// Calls the next hook in the hook chain.
///
/// This must be called at the end of every hook callback to ensure
/// other hooks receive the event. Failure to call this breaks the
/// hook chain for all applications.
///
/// # Safety
/// This function is safe to call from within a hook callback.
#[inline(always)]
pub fn call_next_hook(
    code: i32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> LRESULT {
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    // Hook tests require a running message loop and are better
    // suited for integration testing. Unit tests for hook
    // installation would hang waiting for messages.
}
