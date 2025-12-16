# 🚀 OwnMon Beta Release Guide

## Step 1: Update Version in Cargo.toml

Open `Cargo.toml` and update the version:

```toml
[package]
name = "ownmon"
version = "0.1.0-beta.1"  # Change to beta version
```

## Step 2: Commit Your Changes

```bash
# Stage all changes
git add .

# Commit with a clear message
git commit -m "Release v0.1.0-beta.1 - First beta release"
```

## Step 3: Create and Push the Tag

```bash
# Create an annotated tag for the beta release
git tag -a v0.1.0-beta.1 -m "v0.1.0-beta.1 - First beta release

Features:
- Window focus tracking with AFK detection
- Session splitting on idle/resume
- REST API with WebSocket support
- Database persistence with SQLite
- Configurable minimum session duration
- System tray integration"

# Push the commit first
git push origin master

# Push the tag to trigger release workflow
git push origin v0.1.0-beta.1
```

## Step 4: Monitor the Release

1. Go to your GitHub repository
2. Click **Actions** tab
3. Watch the "Release" workflow run
4. It will:
   - ✅ Build the release binary
   - ✅ Create GitHub release
   - ✅ Upload `ownmon-v0.1.0-beta.1-windows-x64.zip`
   - ✅ Generate changelog

## Step 5: Verify the Release

1. Go to **Releases** on GitHub
2. You should see "Release v0.1.0-beta.1"
3. Download the ZIP and test it
4. Mark as pre-release if needed (optional)

## Future Beta Updates

For subsequent betas:
```bash
# Update Cargo.toml to 0.1.0-beta.2, 0.1.0-beta.3, etc.
git commit -am "Release v0.1.0-beta.2"
git tag -a v0.1.0-beta.2 -m "v0.1.0-beta.2 - Bug fixes"
git push origin master
git push origin v0.1.0-beta.2
```

## Full Release (When Ready)

```bash
# Update Cargo.toml to 1.0.0
git commit -am "Release v1.0.0"
git tag -a v1.0.0 -m "v1.0.0 - First stable release"
git push origin master
git push origin v1.0.0
```

---

**Note:** Make sure all tests pass before releasing:
```bash
cargo test --lib
cargo clippy --all-targets -- -D warnings
```

Good luck with your first beta! 🎉
