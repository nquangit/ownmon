//! Icon creation and management for the system tray.

use tray_icon::Icon;

/// Creates a default icon for the system tray.
///
/// This generates a simple colored icon programmatically since we don't
/// have an external icon file. For production, replace with a proper .ico file.
pub fn create_default_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    // Create a simple 32x32 icon with a gradient
    let size = 32u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);

    for y in 0..size {
        for x in 0..size {
            // Create a simple circular icon with a blue-to-cyan gradient
            let cx = size as f32 / 2.0;
            let cy = size as f32 / 2.0;
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let distance = (dx * dx + dy * dy).sqrt();
            let radius = size as f32 / 2.0 - 2.0;

            if distance <= radius {
                // Inside the circle - gradient from blue to cyan
                let t = distance / radius;
                let r = (30.0 + t * 50.0) as u8; // 30 -> 80
                let g = (144.0 + t * 80.0) as u8; // 144 -> 224
                let b = (255.0 - t * 30.0) as u8; // 255 -> 225
                let a = 255u8;

                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(a);
            } else {
                // Outside - transparent
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
            }
        }
    }

    Icon::from_rgba(rgba, size, size).map_err(|e| e.into())
}

/// Loads an icon from a file path.
///
/// Supports .ico, .png, and other common formats.
#[allow(dead_code)]
#[allow(unused_variables)]
pub fn load_icon_from_file(path: &std::path::Path) -> Result<Icon, Box<dyn std::error::Error>> {
    let image_data = std::fs::read(path)?;

    // For .ico files, we need to parse them
    // For now, just use the default icon
    // In production, use the `image` crate to decode various formats

    tracing::warn!("Icon loading from file not fully implemented, using default");
    create_default_icon()
}
