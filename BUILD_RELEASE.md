# Building OwnMon Release Without Console

## What Changed

Added to `src/main.rs`:
```rust
// Hide console window in release builds on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
```

This tells Windows to:
- **Debug builds** (`cargo run`): Show console ✅ (for development)
- **Release builds** (`cargo build --release`): Hide console ✅ (for distribution)

## Building Release Version

### 1. Build Release
```bash
cargo build --release
```

The executable will be at:
```
target/release/ownmon.exe
```

### 2. Test Release Build
```bash
# Run the release build
./target/release/ownmon.exe

# No console window will appear!
# Look for the blue icon in system tray
```

### 3. Distribute
The release build is:
- ✅ Optimized (smaller, faster)
- ✅ No console window
- ✅ Ready for users

## Key Differences

| Mode | Command | Console | Speed | Size |
|------|---------|---------|-------|------|
| Debug | `cargo run` | ✅ Shows | Slower | Larger |
| Release | `cargo build --release` | ❌ Hidden | Faster | Smaller |

## Troubleshooting

### Can't See Logs?
Since there's no console in release mode, check logs via:
1. System tray icon → "Show Statistics"
2. Or use a logging library to file

### Need Console in Release?
Temporarily remove the attribute:
```rust
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
```

## GitHub Actions

The release workflow automatically builds with `--release`, so releases will have no console! ✅
