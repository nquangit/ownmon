# OwnMon: Windows Activity Monitor - Master Blueprint

> **Document Version:** 1.0  
> **Target Platform:** Windows 10/11 (x64)  
> **Language:** Rust (2021 Edition)  
> **Classification:** Technical Architecture & Implementation Guide

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Architectural Overview](#2-architectural-overview)
3. [Tech Stack & Dependencies](#3-tech-stack--dependencies)
4. [Module Structure](#4-module-structure)
5. [Implementation Guidelines](#5-implementation-guidelines)
6. [Step-by-Step Development Plan](#6-step-by-step-development-plan)
7. [Appendix](#7-appendix)

---

## 1. Executive Summary

### 1.1 Project Objective

OwnMon is a **high-performance, low-footprint Windows Activity Monitor** designed to:

- Track user activity across applications (window focus, duration)
- Aggregate input metrics (keystrokes, mouse clicks) per application context
- Run silently with a system tray interface
- Expose collected data for future HTTP API consumption

### 1.2 Key Design Principles

| Principle | Description |
|-----------|-------------|
| **Minimal Footprint** | CPU usage near 0% at idle; memory < 10MB |
| **Non-Blocking** | Never freeze the Windows message pump |
| **Thread-Safe** | All shared state accessible from multiple threads |
| **Modular** | Clean separation enabling incremental development |
| **Future-Ready** | Architecture supports HTTP server integration |

---

## 2. Architectural Overview

### 2.1 System Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              OWNMON ARCHITECTURE                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                         MAIN THREAD                                  │   │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐  │   │
│  │  │ Windows Message │  │   System Tray   │  │  Low-Level Hooks    │  │   │
│  │  │      Pump       │  │    Handler      │  │  (KB + Mouse LL)    │  │   │
│  │  │   (GetMessage)  │  │                 │  │                     │  │   │
│  │  └────────┬────────┘  └────────┬────────┘  └──────────┬──────────┘  │   │
│  │           │                    │                      │             │   │
│  │           └────────────────────┼──────────────────────┘             │   │
│  │                                │                                     │   │
│  └────────────────────────────────┼─────────────────────────────────────┘   │
│                                   │                                         │
│                                   ▼                                         │
│                    ┌──────────────────────────────┐                        │
│                    │     SHARED STATE (Arc<T>)    │                        │
│                    │  ┌────────────────────────┐  │                        │
│                    │  │  ActivityStore         │  │                        │
│                    │  │  - Window Sessions     │  │                        │
│                    │  │  - Input Aggregation   │  │                        │
│                    │  │  - Process Metadata    │  │                        │
│                    │  └────────────────────────┘  │                        │
│                    └──────────────────────────────┘                        │
│                                   ▲                                         │
│  ┌────────────────────────────────┼─────────────────────────────────────┐   │
│  │                        WORKER THREADS                                │   │
│  │                                │                                     │   │
│  │  ┌─────────────────┐  ┌───────┴─────────┐  ┌─────────────────────┐  │   │
│  │  │ Window Polling  │  │  Data Processor │  │  [Future] HTTP API  │  │   │
│  │  │    Thread       │  │     Thread      │  │      Thread         │  │   │
│  │  │ (100ms cycle)   │  │ (Aggregation)   │  │  (Actix/Axum)       │  │   │
│  │  └─────────────────┘  └─────────────────┘  └─────────────────────┘  │   │
│  │                                                                     │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Concurrency Model

#### 2.2.1 Threading Strategy

The application uses a **multi-threaded architecture** with clear separation of concerns:

| Thread | Responsibility | Blocking Behavior |
|--------|---------------|-------------------|
| **Main Thread** | Windows Message Pump, Hooks, System Tray | Blocks on `GetMessage()` |
| **Polling Thread** | Window focus detection, process enumeration | Sleeps between polls |
| **Data Processor Thread** | Session finalization, aggregation | Event-driven or periodic |
| **[Future] HTTP Thread** | REST API server (Actix/Axum) | Async runtime (Tokio) |

#### 2.2.2 Why Main Thread for Hooks?

> [!IMPORTANT]
> **Windows Low-Level Hooks (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`) REQUIRE a message loop on the installing thread.** If the message loop is blocked or too slow, Windows will automatically unhook after a timeout (~5 seconds).

**Implementation Rule:** The main thread must:
1. Install hooks via `SetWindowsHookExW`
2. Run a tight message loop (`GetMessage` → `TranslateMessage` → `DispatchMessage`)
3. Never perform blocking I/O or heavy computation

#### 2.2.3 Communication Pattern

```
┌─────────────┐    mpsc::channel()    ┌─────────────────┐
│    Hooks    │ ───────────────────▶  │  Data Processor │
│ (increment) │                       │   (aggregate)   │
└─────────────┘                       └─────────────────┘
       │                                      │
       │                                      │
       ▼                                      ▼
┌─────────────────────────────────────────────────────┐
│              Arc<RwLock<ActivityStore>>             │
│  - Current session: write lock (hooks)              │
│  - Historical data: read lock (HTTP server)         │
└─────────────────────────────────────────────────────┘
```

### 2.3 Data Strategy

#### 2.3.1 Core Data Structures

```rust
/// Represents a single window focus session
pub struct WindowSession {
    pub window_handle: isize,          // HWND as isize
    pub process_id: u32,
    pub process_name: String,          // e.g., "chrome.exe"
    pub window_title: String,          // e.g., "GitHub - Mozilla Firefox"
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub keystrokes: u64,
    pub mouse_clicks: u64,
    pub mouse_scrolls: u64,
}

/// Aggregated statistics per application
pub struct ApplicationStats {
    pub process_name: String,
    pub total_focus_time: Duration,
    pub total_keystrokes: u64,
    pub total_clicks: u64,
    pub session_count: u32,
}

/// The main store holding all activity data
pub struct ActivityStore {
    pub current_session: Option<WindowSession>,
    pub completed_sessions: Vec<WindowSession>,
    pub app_aggregates: HashMap<String, ApplicationStats>,
    pub last_poll_time: DateTime<Utc>,
}
```

#### 2.3.2 Thread-Safe Access Pattern

**Recommended Pattern:** `Arc<RwLock<ActivityStore>>`

```rust
use std::sync::{Arc, RwLock};
use once_cell::sync::Lazy;

pub static ACTIVITY_STORE: Lazy<Arc<RwLock<ActivityStore>>> = Lazy::new(|| {
    Arc::new(RwLock::new(ActivityStore::default()))
});
```

**Access Rules:**

| Operation | Lock Type | Thread |
|-----------|-----------|--------|
| Increment keystrokes/clicks | `write()` | Main (Hook callback) |
| Update current session | `write()` | Polling Thread |
| Read for HTTP response | `read()` | HTTP Thread |
| Finalize session | `write()` | Data Processor |

> [!CAUTION]
> **Hook callbacks must be extremely fast.** Use `try_write()` with a fallback counter if the lock is contended. Never block in a hook callback.

#### 2.3.3 Alternative: Channel-Based Approach

For higher throughput, consider using `std::sync::mpsc` or `crossbeam::channel`:

```rust
enum InputEvent {
    Keystroke { timestamp: Instant },
    MouseClick { button: MouseButton, timestamp: Instant },
    MouseScroll { delta: i32, timestamp: Instant },
}

// In hook callback (non-blocking)
let _ = event_sender.try_send(InputEvent::Keystroke { 
    timestamp: Instant::now() 
});

// In processor thread (blocking)
while let Ok(event) = event_receiver.recv() {
    process_input_event(event, &activity_store);
}
```

---

## 3. Tech Stack & Dependencies

### 3.1 Cargo.toml Configuration

```toml
[package]
name = "ownmon"
version = "0.1.0"
edition = "2021"

# Optimize for size in release builds
[profile.release]
opt-level = "z"     # Optimize for size
lto = true          # Link-time optimization
codegen-units = 1   # Single codegen unit for better optimization
strip = true        # Strip symbols

[dependencies]
# === Windows API ===
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_System_LibraryLoader",
    "Win32_System_Threading",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_Graphics_Gdi",
    "Win32_System_ProcessStatus",
]}

# === System Tray ===
tray-icon = "0.14"
winit = { version = "0.29", features = ["rwh_06"] }  # Event loop for tray

# === Serialization ===
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# === Date/Time ===
chrono = { version = "0.4", features = ["serde"] }

# === Lazy Initialization ===
once_cell = "1.19"

# === Logging ===
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# === Async Runtime (for future HTTP server) ===
tokio = { version = "1.0", features = ["rt-multi-thread", "sync"], optional = true }

# === HTTP Server (optional, for Phase 5+) ===
axum = { version = "0.7", optional = true }

[features]
default = []
http-server = ["tokio", "axum"]
```

### 3.2 Dependency Justification

| Crate | Purpose | Why This Choice |
|-------|---------|-----------------|
| `windows` | WinAPI bindings | Official Microsoft crate, safer than `winapi`, auto-generated from metadata |
| `tray-icon` | System tray | Modern, maintained, cross-platform design (works with custom event loops) |
| `winit` | Windowing/event loop | Robust, well-tested, integrates with `tray-icon` |
| `serde` | Serialization | Industry standard, required for JSON API responses |
| `chrono` | Date/Time | Full-featured, timezone-aware, Serde integration |
| `once_cell` | Lazy statics | Rust std-lib compatible, cleaner than `lazy_static!` |
| `tracing` | Logging | Structured logging, async-aware, production-ready |

> [!NOTE]
> **Why `windows` over `winapi`?**  
> The `windows` crate is officially maintained by Microsoft, provides better type safety, and generates bindings from Windows metadata. It's the recommended choice for new projects.

---

## 4. Module Structure

### 4.1 Directory Tree

```
ownmon/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── instruction/
│   └── main.md                 # This document
│
├── src/
│   ├── main.rs                 # Entry point, thread orchestration
│   │
│   ├── lib.rs                  # Library root (re-exports)
│   │
│   ├── config.rs               # Configuration constants & settings
│   │
│   ├── winapi_utils/           # Safe WinAPI wrappers
│   │   ├── mod.rs
│   │   ├── window.rs           # Window enumeration, focus detection
│   │   ├── process.rs          # Process info extraction
│   │   ├── hooks.rs            # SetWindowsHookEx wrappers
│   │   └── message_loop.rs     # Message pump utilities
│   │
│   ├── monitor/                # Core monitoring logic
│   │   ├── mod.rs
│   │   ├── input_hooks.rs      # Keyboard/Mouse LL hook handlers
│   │   ├── window_poller.rs    # Foreground window polling
│   │   └── session_manager.rs  # Session start/end logic
│   │
│   ├── store/                  # Data storage & aggregation
│   │   ├── mod.rs
│   │   ├── types.rs            # WindowSession, ApplicationStats
│   │   ├── activity_store.rs   # Main store implementation
│   │   └── aggregator.rs       # Statistics computation
│   │
│   ├── tray/                   # System tray functionality
│   │   ├── mod.rs
│   │   ├── icon.rs             # Icon loading & management
│   │   └── menu.rs             # Context menu handling
│   │
│   └── api/                    # [Future] HTTP API
│       ├── mod.rs
│       ├── routes.rs           # Endpoint definitions
│       └── handlers.rs         # Request handlers
│
├── assets/
│   ├── icon.ico                # System tray icon
│   └── icon.png                # Alternative format
│
└── tests/
    ├── integration/
    │   └── store_tests.rs
    └── fixtures/
```

### 4.2 Module Responsibilities

#### 4.2.1 `src/main.rs`

**Purpose:** Application entry point and thread orchestration.

**Responsibilities:**
- Initialize logging (`tracing_subscriber`)
- Initialize the shared `ActivityStore`
- Spawn the polling thread
- Set up system tray
- Install low-level hooks
- Run the Windows message loop
- Handle graceful shutdown

```rust
// Pseudocode structure
fn main() -> Result<()> {
    init_logging();
    
    let store = Arc::clone(&ACTIVITY_STORE);
    
    // Spawn worker threads
    let polling_handle = spawn_polling_thread(Arc::clone(&store));
    
    // Initialize system tray (before message loop)
    let tray = setup_system_tray()?;
    
    // Install hooks (must be on main thread)
    let _keyboard_hook = install_keyboard_hook()?;
    let _mouse_hook = install_mouse_hook()?;
    
    // Run message loop (blocks)
    run_message_loop(&tray)?;
    
    // Cleanup
    polling_handle.join()?;
    Ok(())
}
```

---

#### 4.2.2 `src/winapi_utils/`

**Purpose:** Provide safe Rust wrappers around `unsafe` Windows API calls.

##### `window.rs`

| Function | WinAPI Calls | Returns |
|----------|--------------|---------|
| `get_foreground_window()` | `GetForegroundWindow` | `Option<HWND>` |
| `get_window_text(hwnd)` | `GetWindowTextW`, `GetWindowTextLengthW` | `String` |
| `get_window_thread_process_id(hwnd)` | `GetWindowThreadProcessId` | `(u32, u32)` (thread_id, process_id) |
| `enum_windows()` | `EnumWindows` | `Vec<HWND>` |

##### `process.rs`

| Function | WinAPI Calls | Returns |
|----------|--------------|---------|
| `get_process_name(pid)` | `OpenProcess`, `GetModuleBaseNameW` | `Option<String>` |
| `get_process_path(pid)` | `OpenProcess`, `GetModuleFileNameExW` | `Option<PathBuf>` |

##### `hooks.rs`

| Function | WinAPI Calls | Returns |
|----------|--------------|---------|
| `set_windows_hook(hook_type, callback)` | `SetWindowsHookExW` | `Result<HookHandle>` |
| `unhook(handle)` | `UnhookWindowsHookEx` | `Result<()>` |
| `call_next_hook(...)` | `CallNextHookEx` | `LRESULT` |

##### `message_loop.rs`

| Function | WinAPI Calls | Purpose |
|----------|--------------|---------|
| `run_message_loop()` | `GetMessage`, `TranslateMessage`, `DispatchMessage` | Pump messages until `WM_QUIT` |
| `post_quit_message()` | `PostQuitMessage` | Signal loop termination |

---

#### 4.2.3 `src/monitor/`

**Purpose:** Core monitoring logic for input hooks and window tracking.

##### `input_hooks.rs`

Implements the hook callback functions:

```rust
/// Keyboard hook callback - MUST BE EXTREMELY FAST
pub extern "system" fn keyboard_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 {
        // Only count key-down events (avoid double-counting)
        if wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN {
            increment_keystroke_counter();
        }
    }
    
    // CRITICAL: Always call next hook
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}
```

> [!WARNING]
> **Performance Critical:** The hook callback executes synchronously in the input pipeline. Any delay here causes system-wide input lag. Keep operations under 1ms.

##### `window_poller.rs`

```rust
pub fn polling_loop(store: Arc<RwLock<ActivityStore>>, shutdown: Arc<AtomicBool>) {
    let poll_interval = Duration::from_millis(100);
    
    while !shutdown.load(Ordering::Relaxed) {
        if let Some(hwnd) = get_foreground_window() {
            let (_, pid) = get_window_thread_process_id(hwnd);
            let title = get_window_text(hwnd);
            let process_name = get_process_name(pid).unwrap_or_default();
            
            update_current_session(&store, hwnd, pid, &process_name, &title);
        }
        
        std::thread::sleep(poll_interval);
    }
}
```

---

#### 4.2.4 `src/store/`

**Purpose:** Thread-safe data storage and aggregation.

##### `types.rs`

Define all data structures with Serde derives:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSession {
    #[serde(skip)]
    pub window_handle: isize,
    pub process_id: u32,
    pub process_name: String,
    pub window_title: String,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub start_time: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub end_time: Option<DateTime<Utc>>,
    pub keystrokes: u64,
    pub mouse_clicks: u64,
}
```

##### `activity_store.rs`

```rust
impl ActivityStore {
    /// Called when window focus changes
    pub fn switch_session(&mut self, new_hwnd: isize, pid: u32, name: &str, title: &str) {
        // 1. Finalize current session
        if let Some(mut session) = self.current_session.take() {
            session.end_time = Some(Utc::now());
            self.completed_sessions.push(session);
        }
        
        // 2. Start new session
        self.current_session = Some(WindowSession {
            window_handle: new_hwnd,
            process_id: pid,
            process_name: name.to_string(),
            window_title: title.to_string(),
            start_time: Utc::now(),
            end_time: None,
            keystrokes: 0,
            mouse_clicks: 0,
        });
    }
    
    /// Increment keystroke counter (called from hook)
    pub fn increment_keystrokes(&mut self) {
        if let Some(session) = &mut self.current_session {
            session.keystrokes += 1;
        }
    }
}
```

---

#### 4.2.5 `src/tray/`

**Purpose:** System tray icon and context menu management.

##### Key Functionality

```rust
pub fn setup_system_tray() -> Result<TrayIcon> {
    let icon = load_icon_from_resource()?;
    
    let menu = Menu::new();
    menu.append(&MenuItem::new("Show Stats", true, None))?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&MenuItem::new("Exit", true, None))?;
    
    let tray = TrayIconBuilder::new()
        .with_tooltip("OwnMon - Activity Monitor")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .build()?;
    
    Ok(tray)
}
```

**Menu Events:** Handle via the `winit` event loop or custom message processing.

---

## 5. Implementation Guidelines

### 5.1 Low-Level Hooks: The "How-To"

#### 5.1.1 Hook Installation

```rust
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Foundation::*;

pub struct HookGuard {
    handle: HHOOK,
}

impl HookGuard {
    pub fn install_keyboard_hook() -> windows::core::Result<Self> {
        let handle = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,                    // Hook type
                Some(keyboard_hook_proc),          // Callback function
                None,                              // hInstance (None for global)
                0,                                 // Thread ID (0 = all threads)
            )?
        };
        
        Ok(Self { handle })
    }
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.handle);
        }
    }
}
```

#### 5.1.2 Hook Callback Implementation Rules

> [!CAUTION]
> **CRITICAL PERFORMANCE REQUIREMENTS**

| Rule | Explanation |
|------|-------------|
| **No heap allocation** | Use pre-allocated buffers or atomic counters |
| **No blocking operations** | No locks that might contend, no I/O |
| **No complex logic** | Only increment counters or send to channel |
| **Always call `CallNextHookEx`** | Failure breaks the hook chain |
| **Handle both key events** | `WM_KEYDOWN` and `WM_SYSKEYDOWN` for Alt+key |

**Recommended Pattern:**

```rust
use std::sync::atomic::{AtomicU64, Ordering};

// Global atomic counters (lock-free)
static KEYSTROKE_COUNT: AtomicU64 = AtomicU64::new(0);
static CLICK_COUNT: AtomicU64 = AtomicU64::new(0);

pub extern "system" fn keyboard_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 {
        let msg = wparam.0 as u32;
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            KEYSTROKE_COUNT.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}
```

**Counter Flush Strategy:**

The polling thread periodically transfers atomic counter values to the store:

```rust
fn flush_counters_to_store(store: &Arc<RwLock<ActivityStore>>) {
    let keystrokes = KEYSTROKE_COUNT.swap(0, Ordering::Relaxed);
    let clicks = CLICK_COUNT.swap(0, Ordering::Relaxed);
    
    if let Ok(mut store) = store.try_write() {
        if let Some(session) = &mut store.current_session {
            session.keystrokes += keystrokes;
            session.mouse_clicks += clicks;
        }
    }
}
```

---

### 5.2 Window Polling Strategy

#### 5.2.1 Polling Flow

```
┌─────────────────────────────────────────────────────────────┐
│                    POLLING LOOP (100ms)                     │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  1. GetForegroundWindow() -> HWND                          │
│           │                                                 │
│           ▼                                                 │
│  2. Compare with current_session.window_handle             │
│           │                                                 │
│       ┌───┴───┐                                            │
│       │ Same? │                                            │
│       └───┬───┘                                            │
│      Yes  │  No                                            │
│       │   │                                                │
│       ▼   ▼                                                │
│  3a. Flush   3b. GetWindowThreadProcessId() -> (tid, pid)  │
│      counters        │                                     │
│       │              ▼                                     │
│       │      3c. get_process_name(pid)                     │
│       │              │                                     │
│       │              ▼                                     │
│       │      3d. GetWindowTextW(hwnd)                      │
│       │              │                                     │
│       │              ▼                                     │
│       │      3e. store.switch_session(...)                 │
│       │              │                                     │
│       └──────────────┤                                     │
│                      ▼                                     │
│              4. thread::sleep(100ms)                       │
│                      │                                     │
│                      └──────────▶ Loop                     │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

#### 5.2.2 Optimal Polling Interval

| Interval | CPU Impact | Accuracy | Recommendation |
|----------|-----------|----------|----------------|
| 50ms | ~0.1% | High | Only if precision needed |
| **100ms** | **~0.05%** | **Good** | **Recommended default** |
| 250ms | ~0.02% | Moderate | Acceptable for most uses |
| 500ms | ~0.01% | Low | May miss quick switches |

> [!TIP]
> Start with 100ms and expose as a configuration option for power users.

---

### 5.3 Resource Optimization

#### 5.3.1 Memory Management

**Handle Cleanup:**

```rust
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::OpenProcess;

pub struct ProcessHandle(HANDLE);

impl ProcessHandle {
    pub fn open(pid: u32) -> Option<Self> {
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                pid,
            )
        }.ok()?;
        
        Some(Self(handle))
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
```

#### 5.3.2 String Buffer Reuse

```rust
// Reuse buffer across calls to GetWindowTextW
thread_local! {
    static WINDOW_TEXT_BUFFER: RefCell<Vec<u16>> = RefCell::new(vec![0u16; 512]);
}

pub fn get_window_text(hwnd: HWND) -> String {
    WINDOW_TEXT_BUFFER.with(|buf| {
        let mut buf = buf.borrow_mut();
        let len = unsafe { GetWindowTextW(hwnd, &mut buf[..]) };
        String::from_utf16_lossy(&buf[..len as usize])
    })
}
```

#### 5.3.3 Session Data Pruning

Implement periodic cleanup to prevent unbounded growth:

```rust
impl ActivityStore {
    pub fn prune_old_sessions(&mut self, max_age: Duration) {
        let cutoff = Utc::now() - max_age;
        self.completed_sessions.retain(|s| {
            s.start_time > cutoff
        });
    }
}
```

---

### 5.4 Error Handling Strategy

#### 5.4.1 Graceful Degradation

| Component | Failure Mode | Recovery Action |
|-----------|--------------|-----------------|
| Keyboard Hook | `SetWindowsHookExW` fails | Log error, continue without keyboard tracking |
| Mouse Hook | `SetWindowsHookExW` fails | Log error, continue without mouse tracking |
| Window Polling | `GetForegroundWindow` fails | Skip poll cycle, retry next interval |
| Process Name | `OpenProcess` fails | Use "Unknown" as process name |
| System Tray | Icon load fails | Use default Windows icon |

#### 5.4.2 Structured Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum OwnMonError {
    #[error("Failed to install {hook_type} hook: {source}")]
    HookInstallation {
        hook_type: &'static str,
        source: windows::core::Error,
    },
    
    #[error("Window API call failed: {0}")]
    WindowApi(#[from] windows::core::Error),
    
    #[error("System tray initialization failed: {0}")]
    TrayInit(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
}
```

---

## 6. Step-by-Step Development Plan

### Phase 1: Foundation & WinAPI Wrappers

**Duration:** 1-2 days  
**Goal:** Establish safe abstractions over Windows API

#### Deliverables

- [ ] **`src/winapi_utils/mod.rs`** - Module definition
- [ ] **`src/winapi_utils/window.rs`** - Window functions
  - `get_foreground_window() -> Option<HWND>`
  - `get_window_text(hwnd: HWND) -> String`
  - `get_window_thread_process_id(hwnd: HWND) -> (u32, u32)`
- [ ] **`src/winapi_utils/process.rs`** - Process functions
  - `get_process_name(pid: u32) -> Option<String>`
- [ ] **`src/winapi_utils/message_loop.rs`** - Message pump
  - `run_message_loop()`

#### Verification

```rust
// Test in main.rs
fn main() {
    if let Some(hwnd) = get_foreground_window() {
        let title = get_window_text(hwnd);
        let (_, pid) = get_window_thread_process_id(hwnd);
        let name = get_process_name(pid);
        println!("Current: {} - {} (PID: {})", name.unwrap_or_default(), title, pid);
    }
}
```

---

### Phase 2: Data Store & Types

**Duration:** 1 day  
**Goal:** Implement thread-safe data structures

#### Deliverables

- [ ] **`src/store/types.rs`** - Data types with Serde
  - `WindowSession`
  - `ApplicationStats`
- [ ] **`src/store/activity_store.rs`** - Main store
  - `ActivityStore` struct
  - `switch_session()`, `increment_keystrokes()`, `increment_clicks()`
- [ ] **`src/store/mod.rs`** - Global lazy static

#### Verification

```rust
#[test]
fn test_session_switch() {
    let store = Arc::new(RwLock::new(ActivityStore::default()));
    
    {
        let mut s = store.write().unwrap();
        s.switch_session(1, 100, "chrome.exe", "Google");
        s.increment_keystrokes();
        s.increment_keystrokes();
    }
    
    let s = store.read().unwrap();
    assert_eq!(s.current_session.as_ref().unwrap().keystrokes, 2);
}
```

---

### Phase 3: Input Hooks

**Duration:** 2 days  
**Goal:** Implement low-level keyboard and mouse hooks

#### Deliverables

- [ ] **`src/winapi_utils/hooks.rs`** - Hook infrastructure
  - `HookGuard` struct with RAII cleanup
  - `install_keyboard_hook()`, `install_mouse_hook()`
- [ ] **`src/monitor/input_hooks.rs`** - Hook callbacks
  - `keyboard_hook_proc`
  - `mouse_hook_proc`
  - Atomic counters for lock-free counting

#### Critical Tests

1. Install hooks and verify no input lag
2. Verify keystroke counting accuracy
3. Verify mouse click detection (left, right, middle)
4. Ensure hooks are properly unhooked on exit

---

### Phase 4: Window Polling & Session Management

**Duration:** 1-2 days  
**Goal:** Integrate all components for continuous monitoring

#### Deliverables

- [ ] **`src/monitor/window_poller.rs`** - Polling loop
  - `spawn_polling_thread()`
  - Focus change detection
  - Counter flushing
- [ ] **`src/monitor/session_manager.rs`** - Session logic
  - Session transition handling
  - Title change detection (same window, different page)

#### Integration Test

Run the application for 5 minutes:
- Switch between 3-4 applications
- Verify session boundaries are correct
- Verify keystroke/click counts are attributed correctly

---

### Phase 5: System Tray Integration

**Duration:** 1 day  
**Goal:** Add system tray with basic menu

#### Deliverables

- [ ] **`assets/icon.ico`** - Application icon
- [ ] **`src/tray/icon.rs`** - Icon loading
- [ ] **`src/tray/menu.rs`** - Context menu
  - "Show Stats" menu item (print to console for now)
  - "Exit" menu item (clean shutdown)
- [ ] **`src/main.rs`** - Full orchestration

#### Verification

1. Icon appears in system tray
2. Tooltip shows "OwnMon - Activity Monitor"
3. Right-click shows context menu
4. "Exit" cleanly shuts down all threads
5. No orphaned processes after exit

---

### Phase 6: Polish & HTTP Server Preparation

**Duration:** 2 days  
**Goal:** Finalize MVP and prepare for API

#### Deliverables

- [ ] **Aggregation logic** - `src/store/aggregator.rs`
  - `compute_application_stats()`
  - `get_today_summary()`
- [ ] **Configuration** - `src/config.rs`
  - Poll interval
  - Data retention period
  - Log level
- [ ] **Logging** - Comprehensive tracing
- [ ] **Documentation** - README.md with usage

#### [Optional] Phase 6b: HTTP Server

- [ ] Enable `http-server` feature
- [ ] **`src/api/routes.rs`** - Endpoint definitions
  - `GET /api/sessions` - List recent sessions
  - `GET /api/stats` - Aggregated statistics
  - `GET /api/current` - Current active window
- [ ] **`src/api/handlers.rs`** - Request handlers

---

## 7. Appendix

### 7.1 Code Snippet Reference

#### Complete Hook Installation Example

```rust
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Foundation::*;
use std::sync::atomic::{AtomicU64, Ordering};

static KEYSTROKE_COUNT: AtomicU64 = AtomicU64::new(0);

pub extern "system" fn keyboard_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code as u32 == HC_ACTION {
        match wparam.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                KEYSTROKE_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

pub fn install_keyboard_hook() -> windows::core::Result<HHOOK> {
    unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_hook_proc),
            None,
            0,
        )
    }
}
```

### 7.2 Common Pitfalls

| Pitfall | Consequence | Prevention |
|---------|-------------|------------|
| Blocking in hook callback | System-wide input lag | Use atomics, never lock |
| Forgetting `CallNextHookEx` | Broken hook chain | Always call, even on error |
| Not running message loop | Hooks stop working | Ensure main thread pumps messages |
| Leaking process handles | Handle exhaustion | Use RAII wrappers |
| Unbounded session storage | Memory growth | Implement pruning |

### 7.3 Testing Checklist

#### Unit Tests
- [ ] Window text extraction handles Unicode
- [ ] Process name extraction handles access denied
- [ ] Store session switching is correct
- [ ] Atomic counter operations are correct

#### Integration Tests
- [ ] Application runs without elevated privileges
- [ ] Memory usage stays under 20MB after 1 hour
- [ ] CPU usage stays under 1% on idle
- [ ] Graceful shutdown completes in < 2 seconds
- [ ] No input lag when hooks active

#### Edge Cases
- [ ] Handle rapid window switching (< 100ms between switches)
- [ ] Handle windows with extremely long titles (> 1024 chars)
- [ ] Handle process that exits while being monitored
- [ ] Handle system sleep/wake cycles

---

## Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2024-12-13 | System Architect | Initial release |

---

> **End of Master Blueprint**
