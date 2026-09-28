//! Saving frames as PNG evidence for an agent (or a human) who cannot watch the game run.
use crate::Result;
use macroquad::prelude::get_screen_data;
use std::path::Path;

/// Save the current frame as a PNG and return its actual size in pixels.
///
/// The size can differ from the requested window size: `window_config` asks for a high-DPI window, and
/// the OS may clamp a window to the desktop. Report it (`{"width":..,"height":..}`) instead of assuming.
pub fn save_frame(path: &Path) -> Result<(u32, u32)> {
    let image = get_screen_data();
    let size = (u32::from(image.width), u32::from(image.height));
    image.export_png(path.to_str().ok_or("Invalid capture path")?);
    Ok(size)
}
