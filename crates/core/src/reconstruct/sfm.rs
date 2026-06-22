//! COLMAP Structure-from-Motion (CPU), through image undistortion — producing a
//! COLMAP scene folder ready for OpenMVS's `InterfaceCOLMAP`.

use crate::error::Result;
use crate::external::{self, path_str};
use crate::reconstruct::gates;
use std::path::{Path, PathBuf};

/// Run COLMAP feature-extraction -> matching -> mapping -> undistortion.
/// Returns the undistorted COLMAP scene directory (`images/` + `sparse/`).
pub fn run(images_dir: &Path, work_dir: &Path) -> Result<PathBuf> {
    let db = work_dir.join("database.db");
    let sparse = work_dir.join("sparse");
    let scene = work_dir.join("colmap_scene");
    std::fs::create_dir_all(&sparse)?;

    let db = path_str(&db)?;
    let images = path_str(images_dir)?;

    // CPU SIFT; single_camera = every photo shares one lens/intrinsics (true for
    // a single-phone capture, and a big robustness win for COLMAP).
    external::run(
        "colmap",
        &[
            "feature_extractor",
            "--database_path",
            db,
            "--image_path",
            images,
            "--ImageReader.single_camera",
            "1",
            // COLMAP 4.x renamed the SfM GPU flags (was SiftExtraction.use_gpu).
            "--FeatureExtraction.use_gpu",
            "0",
        ],
    )?;
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
