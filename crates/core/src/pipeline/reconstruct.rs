//! Front-half orchestration: photos → textured mesh (preprocess + reconstruct).

use crate::error::Result;
use crate::preprocess;
use std::path::{Path, PathBuf};

/// Front-half settings: input downscaling + optional background masking.
#[derive(Debug, Clone)]
pub struct ReconstructConfig {
    /// Downscale inputs to `max_edge` first, to keep CPU reconstruction tractable.
    pub downscale: bool,
    /// Remove the background (rembg) so the reconstructed mesh is object-only.
    pub mask: bool,
    /// Longest-edge pixel cap when downscaling.
    pub max_edge: u32,
}

impl Default for ReconstructConfig {
    fn default() -> Self {
        Self {
            downscale: true,
            mask: false,
            max_edge: 1600,
        }
    }
}

/// Reconstruct a textured mesh from the photos in `photos`, writing intermediate
/// and output artifacts under `work`. Returns the textured-mesh path.
pub fn reconstruct(photos: &Path, work: &Path, cfg: &ReconstructConfig) -> Result<PathBuf> {
    crate::validate::require_image_dir(photos)?;

    let downscaled = if cfg.downscale {
        let out = work.join("images");
        let n = preprocess::downscale_images(photos, &out, cfg.max_edge)?;
        tracing::info!(count = n, "downscaled images");
        out
    } else {
        photos.to_path_buf()
    };

    let input = if cfg.mask {
        let out = work.join("masked");
        let n = preprocess::mask_images(&downscaled, &out, &work.join("masks"))?;
        tracing::info!(count = n, "masked images (background removed)");
        out
    } else {
        downscaled
    };

    Ok(crate::reconstruct::run(&input, work)?.textured_mesh)
}
