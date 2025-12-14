# OwnMon - Windows Activity Monitor

A high-performance, lightweight Windows activity monitoring application written in Rust. Tracks window focus, keystrokes, and mouse clicks with minimal resource usage.

## Features

- 🖥️ **Window Focus Tracking** - Monitors which application has focus and for how long
- ⌨️ **Keystroke Counting** - Tracks keystrokes per application (no content logging)
- 🖱️ **Mouse Click Counting** - Counts left, right, and middle button clicks
- 🎵 **Media Tracking** - Detects currently playing music/videos (Spotify, YouTube, etc.)
- 💾 **SQLite Persistence** - Crash-safe data storage at `%APPDATA%\ownmon\`
- 📊 **Real-time Statistics** - View activity summaries at any time
- 🔵 **System Tray Integration** - Runs silently in the background
- ⚡ **High Performance** - Near-zero CPU usage, minimal memory footprint

## Requirements

- Windows 10/11 (64-bit)
- Rust 1.70+ (for building)

## Building

```bash
# Clone the repository
git clone https://github.com/yourusername/ownmon.git
cd ownmon

# Build in debug mode
cargo build

# Build in release mode (optimized)
cargo build --release
```

## Usage

```bash
# Run the application
cargo run

# Or run the release binary directly
./target/release/ownmon.exe
```

Once running:
- Look for the blue circular icon in your system tray
- Right-click for options:
  - **Show Statistics** - Displays current activity summary
  - **Exit** - Gracefully shuts down the application
- Press **Ctrl+C** in the terminal to exit

## Architecture

```
ownmon/
├── src/
│   ├── main.rs           # Application entry point
│   ├── lib.rs            # Library root
│   ├── config.rs         # Configuration management
│   ├── media.rs          # Media tracking (GSMTC API)
│   ├── winapi_utils/     # Windows API wrappers
│   │   ├── hooks.rs      # Hook RAII guards
│   │   ├── message_loop.rs
│   │   ├── process.rs    # Process info
│   │   └── window.rs     # Window info
│   ├── store/            # Data storage
│   │   ├── types.rs      # Data structures
│   │   ├── activity_store.rs
│   │   └── aggregator.rs # Statistics
│   ├── monitor/          # Activity monitoring
│   │   ├── input_hooks.rs
│   │   └── window_poller.rs
│   └── tray/             # System tray
│       ├── icon.rs
│       └── menu.rs
```

## How It Works

1. **Input Hooks** - Low-level keyboard and mouse hooks capture input events
2. **Atomic Counters** - Lock-free counting for minimal latency
3. **Window Polling** - Periodic checks for foreground window changes
4. **Session Management** - Tracks focus duration per window
5. **Media Detection** - Uses Windows GSMTC API to detect playing media
6. **Aggregation** - Computes statistics by application

## Performance

- **CPU Usage**: Near 0% (event-driven design)
- **Memory Usage**: < 10MB typical
- **Input Latency**: Imperceptible (< 1ms hook processing)

## Privacy

OwnMon does **not** log:
- ❌ Actual keystrokes or text content
- ❌ Window contents or screenshots
- ❌ URLs or document contents

OwnMon **only** tracks:
- ✅ Which application has focus
- ✅ How long each application is focused
- ✅ Count of keystrokes (not content)
- ✅ Count of mouse clicks

All data is stored in memory and discarded on exit.

## Future Plans

- [ ] HTTP API for external queries
- [ ] Persistent storage (SQLite)
- [ ] Daily/weekly reports
- [ ] Export to JSON/CSV
- [ ] Customizable tracking rules

## License

MIT License - See LICENSE file for details.

## Acknowledgments

Built with:
- [windows-rs](https://github.com/microsoft/windows-rs) - Windows API bindings
- [tray-icon](https://github.com/tauri-apps/tray-icon) - System tray support
- [chrono](https://github.com/chronotope/chrono) - Date/time handling
- [serde](https://github.com/serde-rs/serde) - Serialization
- [tracing](https://github.com/tokio-rs/tracing) - Logging
