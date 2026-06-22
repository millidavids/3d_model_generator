//! Downscale input images to a maximum longest-edge.
//!
//! Full-resolution phone photos make CPU dense reconstruction far slower for no
//! benefit at the lo-fi target, so cap the longest edge before feeding COLMAP.

use crate::error::Result;
use std::path::Path;

/// Downscale every image in `src_dir` so its longest edge is at most `max_edge`
/// pixels, writing results into `dst_dir` (created if missing). Images already
/// within the limit are saved unchanged. Returns the number of images written.
pub fn downscale_images(src_dir: &Path, dst_dir: &Path, max_edge: u32) -> Result<usize> {
    std::fs::create_dir_all(dst_dir)?;
    let mut count = 0usize;
    for entry in std::fs::read_dir(src_dir)? {
        let path = entry?.path();
        if !is_image(&path) {
            continue;
        }
        let Some(name) = path.file_name() else {
            continue;
        };
        match image::open(&path) {
            Ok(img) => {
                let out = if img.width().max(img.height()) > max_edge {
                    img.resize(max_edge, max_edge, image::imageops::FilterType::Lanczos3)
                } else {
                    img
                };
                out.save(dst_dir.join(name))
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                count += 1;
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping unreadable image");
            }
        }
    }
    Ok(count)
}

/// True for file extensions the `image` crate can decode (HEIC is unsupported).
fn is_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "tif" | "tiff" | "bmp" | "webp")
    )
}
