//! Process-related WinAPI wrappers.
//!
//! Provides safe abstractions for retrieving process information
//! such as executable names.

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};

/// RAII wrapper for Windows process handles.
///
/// Automatically closes the handle when dropped to prevent handle leaks.
struct ProcessHandle(HANDLE);

impl ProcessHandle {
    /// Opens a process with limited query and VM read permissions.
    ///
    /// Returns `None` if the process cannot be opened (e.g., access denied
    /// for system processes).
    fn open(pid: u32) -> Option<Self> {
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                false,
                pid,
            )
        };

        match handle {
            Ok(h) if !h.is_invalid() => Some(Self(h)),
            _ => None,
        }
    }

    fn as_raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Gets the executable name of a process by its process ID.
///
/// Returns `None` if:
/// - The process cannot be opened (access denied, process exited)
/// - The module name cannot be retrieved
///
/// # Arguments
/// * `pid` - Process ID
///
/// # Example
/// ```no_run
/// use ownmon::winapi_utils::get_process_name;
///
/// // Get the name of the current process
/// let pid = std::process::id();
/// if let Some(name) = get_process_name(pid) {
///     println!("Process name: {}", name);
/// }
/// ```
pub fn get_process_name(pid: u32) -> Option<String> {
    let handle = ProcessHandle::open(pid)?;

    // Buffer for module name (MAX_PATH = 260)
    let mut buffer: [u16; 260] = [0; 260];

    let len = unsafe { GetModuleBaseNameW(handle.as_raw(), None, &mut buffer) };

    if len == 0 {
        return None;
    }

    Some(String::from_utf16_lossy(&buffer[..len as usize]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_current_process_name() {
        let pid = std::process::id();
        let name = get_process_name(pid);

        // Should get our own process name
        assert!(name.is_some());
        let name = name.unwrap();
        // Should contain something related to our test binary
        assert!(!name.is_empty());
    }

    #[test]
    fn test_get_process_name_invalid_pid() {
        // PID 0 is the System Idle Process and typically inaccessible
        let name = get_process_name(0);
        assert!(name.is_none());
    }

    #[test]
    fn test_process_handle_drop() {
        // Just verify we can open and close without leaking
        let pid = std::process::id();
        {
            let _handle = ProcessHandle::open(pid);
            // Handle should be dropped here
        }
        // No crash means success
    }
}
