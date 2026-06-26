//! COLMAP Structure-from-Motion (CPU), through image undistortion — producing a
//! COLMAP scene folder ready for OpenMVS's `InterfaceCOLMAP`.

use crate::error::Result;
use crate::external::{self, path_str};
use crate::quality::Quality;
use crate::reconstruct::gates;
use std::path::{Path, PathBuf};

/// Conservative ceiling on COLMAP's internal SIFT image size, independent of the
/// quality preset. CPU feature extraction holds the image pyramid per thread, so
/// at full resolution on every core a high-`max_edge` capture OOMs a memory-limited
/// container (COLMAP crashes with status -1). 1600 extracts fine on all cores in
/// the default ~8GB dev container; it bounds *image size*, not COLMAP's thread
/// count (the other half of the memory product), so a much smaller VM may still
/// need `--FeatureExtraction.num_threads` lowered. Poses don't need full
/// resolution, and `image_undistorter` re-derives the undistorted images the dense
/// step consumes from the originals (its own size cap defaults to off), so dense
/// detail is unaffected.
const SIFT_MAX_IMAGE_SIZE: &str = "1600";

/// `robust_sfm` feature count (up from COLMAP's default 8192). More keypoints help
/// register low-texture / smooth surfaces. Memory-validated against the SIFT cap.
const ROBUST_MAX_FEATURES: &str = "16384";

/// Warn when fewer than this percent of input images register — a weak/partial
/// reconstruction (poor overlap, blur, or low texture) that yields a worse mesh.
const REGISTRATION_WARN_PCT: u64 = 60;

/// Run COLMAP feature-extraction -> matching -> mapping -> undistortion.
/// `quality` selects fast vs robust feature extraction. Returns the undistorted
/// COLMAP scene directory (`images/` + `sparse/`).
pub fn run(images_dir: &Path, work_dir: &Path, quality: Quality) -> Result<PathBuf> {
    let db = work_dir.join("database.db");
    let sparse = work_dir.join("sparse");
    let scene = work_dir.join("colmap_scene");
    std::fs::create_dir_all(&sparse)?;

    let db = path_str(&db)?;
    let images = path_str(images_dir)?;

    // CPU SIFT (see sift_args for the fast vs robust flag set).
    let sift = sift_args(db, images, quality);
    let sift_refs: Vec<&str> = sift.iter().map(String::as_str).collect();
    external::run("colmap", &sift_refs)?;
    // Exhaustive matching is fine for tens of images (CPU).
    external::run(
        "colmap",
        &[
            "exhaustive_matcher",
            "--database_path",
            db,
            "--FeatureMatching.use_gpu",
            "0",
        ],
    )?;
    // Incremental mapping -> sparse/0, sparse/1, ...
    external::run(
        "colmap",
        &[
            "mapper",
            "--database_path",
            db,
            "--image_path",
            images,
            "--output_path",
            path_str(&sparse)?,
        ],
    )?;

    // The mapper can yield several disconnected sub-models; take the largest.
    let best = gates::pick_largest_submodel(&sparse)?;

    // Registration report: how many input images the solve actually placed. A low
    // ratio means a weak reconstruction — surface it early, before the slow dense step.
    let total = count_images(images_dir);
    let registered = gates::registered_image_count(&best).unwrap_or(0);
    tracing::info!(registered, total, "SfM registered images");
    if total > 0 && registered * 100 < total as u64 * REGISTRATION_WARN_PCT {
        tracing::warn!(
            registered,
            total,
            "only {registered}/{total} images registered — weak/partial reconstruction \
             (poor overlap, blur, or low texture?)"
        );
    }

    // Undistort into a COLMAP-format folder OpenMVS ingests directly.
    std::fs::create_dir_all(&scene)?;
    external::run(
        "colmap",
        &[
            "image_undistorter",
            "--image_path",
            images,
            "--input_path",
            path_str(&best)?,
            "--output_path",
            path_str(&scene)?,
            "--output_type",
            "COLMAP",
        ],
    )?;
    Ok(scene)
}

/// Count image files in `dir` (the SfM input set), for the registration ratio.
/// These are the pipeline's own downscaled/masked work dirs, so every regular file
/// is an input image.
fn count_images(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .count()
        })
        .unwrap_or(0)
}

/// Build the COLMAP `feature_extractor` argument list. Pure (owned `String`s, no
/// I/O) so the fast-vs-robust flag set is unit-testable without invoking COLMAP.
///
/// The fast path (`draft`/`balanced`) is exactly the historical args. `robust_sfm`
/// (`high`) adds a richer lens model, more keypoints, and affine-invariant SIFT
/// (`estimate_affine_shape` + `domain_size_pooling`) — slower, but it registers
/// low-texture / smooth surfaces that the fast path misses.
fn sift_args(db: &str, images: &str, quality: Quality) -> Vec<String> {
    let mut args = vec![
        "feature_extractor".to_string(),
        "--database_path".to_string(),
        db.to_string(),
        "--image_path".to_string(),
        images.to_string(),
        "--ImageReader.single_camera".to_string(),
        "1".to_string(),
        // COLMAP 4.x renamed the SfM GPU flags (was SiftExtraction.use_gpu).
        "--FeatureExtraction.use_gpu".to_string(),
        "0".to_string(),
        // Bound CPU SIFT memory so high-resolution inputs don't OOM (SIFT_MAX_IMAGE_SIZE).
        "--FeatureExtraction.max_image_size".to_string(),
        SIFT_MAX_IMAGE_SIZE.to_string(),
    ];
    if quality.robust_sfm() {
        args.extend([
            "--ImageReader.camera_model".to_string(),
            "OPENCV".to_string(),
            "--SiftExtraction.max_num_features".to_string(),
            ROBUST_MAX_FEATURES.to_string(),
            "--SiftExtraction.estimate_affine_shape".to_string(),
            "1".to_string(),
            "--SiftExtraction.domain_size_pooling".to_string(),
            "1".to_string(),
        ]);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::sift_args;
    use crate::quality::Quality;

    /// The historical (pre-robust-SfM) feature_extractor args — the fast path must
    /// stay byte-identical to this so `draft`/`balanced` enter no new code path.
    fn historical(db: &str, images: &str) -> Vec<String> {
        [
            "feature_extractor",
            "--database_path",
            db,
            "--image_path",
            images,
            "--ImageReader.single_camera",
            "1",
            "--FeatureExtraction.use_gpu",
            "0",
            "--FeatureExtraction.max_image_size",
            "1600",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    #[test]
    fn fast_path_matches_the_historical_args_exactly() {
        // Command-equality backward-compat guarantee (no new behavior for defaults).
        assert_eq!(
            sift_args("db", "imgs", Quality::Balanced),
            historical("db", "imgs")
        );
        assert_eq!(
            sift_args("db", "imgs", Quality::Draft),
            historical("db", "imgs")
        );
    }

    #[test]
    fn robust_path_adds_camera_model_and_affine_sift() {
        let a = sift_args("db", "imgs", Quality::High);
        // The fast args are still a prefix...
        assert!(a.starts_with(&historical("db", "imgs")));
        // ...plus the robust extras.
        for flag in [
            "--ImageReader.camera_model",
            "--SiftExtraction.max_num_features",
            "--SiftExtraction.estimate_affine_shape",
            "--SiftExtraction.domain_size_pooling",
        ] {
            assert!(a.iter().any(|s| s == flag), "missing {flag}");
        }
        let i = a
            .iter()
            .position(|s| s == "--ImageReader.camera_model")
            .unwrap();
        assert_eq!(a[i + 1], "OPENCV");
    }
}
