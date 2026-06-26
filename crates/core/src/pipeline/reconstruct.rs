//! Front-half orchestration: photos → textured mesh (preprocess + reconstruct).

use crate::error::Result;
use crate::preprocess;
use crate::quality::Quality;
use std::path::{Path, PathBuf};

/// Front-half settings: input downscaling + optional background masking.
#[derive(Debug, Clone)]
pub struct ReconstructConfig {
    /// Downscale inputs to `max_edge` first, to keep CPU reconstruction tractable.
    pub downscale: bool,
    /// Remove the background (rembg) so the reconstructed mesh is object-only.
    pub mask: bool,
    /// Resolved longest-edge pixel cap for the inputs (defaults to `quality`'s
    /// value unless the caller overrides it). Single source of truth: it drives
    /// both the input downscale and the dense step's `--max-resolution` ceiling, so
    /// the two can't drift.
    pub max_edge: u32,
    /// After a successful run, delete every intermediate in the work dir and
    /// keep only the final `.glb`.
    pub clean: bool,
    /// Detail-vs-speed preset for the dense + refinement stages.
    pub quality: Quality,
    /// Exclude soft/blurry input frames (within guards) instead of only warning.
    pub drop_blurry: bool,
    /// rembg model name for `--mask` (e.g. `u2net`, `u2net_human_seg`).
    pub mask_model: String,
}

/// Default rembg segmentation model (open license, general-purpose).
pub const DEFAULT_MASK_MODEL: &str = "u2net";

impl Default for ReconstructConfig {
    fn default() -> Self {
        let quality = Quality::default();
        Self {
            downscale: true,
            mask: false,
            max_edge: quality.max_edge(),
            clean: false,
            quality,
            drop_blurry: false,
            mask_model: DEFAULT_MASK_MODEL.to_string(),
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

    // Input QC: warn on (and optionally drop) blurry frames before masking/SfM, so
    // the dropped set propagates to every later stage.
    let qc_input = preprocess::sharpness_qc(&downscaled, cfg.drop_blurry, work)?;

    let input = if cfg.mask {
        let out = work.join("masked");
        let n = preprocess::mask_images(&qc_input, &out, &work.join("masks"), &cfg.mask_model)?;
        tracing::info!(count = n, model = %cfg.mask_model, "masked images (background removed)");
        out
    } else {
        qc_input
    };

    // The dense step's resolution ceiling tracks the resolved input cap (`max_edge`),
    // not the preset, so a `--max-edge` override is honored end-to-end.
    let mesh =
        crate::reconstruct::run(&input, work, cfg.mask, cfg.quality, cfg.max_edge)?.textured_mesh;

    if cfg.clean {
        clean_intermediates(work, &mesh)?;
        tracing::info!(kept = %mesh.display(), "cleaned intermediates");
    }
    Ok(mesh)
}

/// Delete everything in `work` except the final artifact `keep` (the `.glb`),
/// reclaiming the reconstruction's intermediates — downscaled/masked images,
/// the dense cloud, per-view depth maps, the COLMAP database, and OpenMVS
/// scene files — which together can run to hundreds of MB per object.
///
/// Only invoked after a successful run (the `.glb` exists), so a failed run
/// keeps its intermediates for debugging.
fn clean_intermediates(work: &Path, keep: &Path) -> Result<()> {
    let keep_name = keep.file_name();
    for entry in std::fs::read_dir(work)? {
        let path = entry?.path();
        if path.file_name() == keep_name {
            continue;
        }
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::clean_intermediates;

    #[test]
    fn keeps_only_the_glb() {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path();
        let glb = work.join("scene_textured.glb");
        std::fs::write(&glb, b"GLB").unwrap();
        std::fs::write(work.join("scene_dense.ply"), b"junk").unwrap(); // a file
        std::fs::create_dir(work.join("images")).unwrap(); // a dir
        std::fs::write(work.join("images/a.jpg"), b"img").unwrap();

        clean_intermediates(work, &glb).unwrap();

        let left: Vec<_> = std::fs::read_dir(work)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["scene_textured.glb"]);
    }
}
