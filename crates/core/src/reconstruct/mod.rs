//! Reconstruction front-half: photos -> textured mesh.
//!
//! COLMAP performs Structure-from-Motion (camera poses) and image undistortion;
//! OpenMVS then densifies, meshes, and textures. Every step runs as a CPU
//! subprocess (see [`crate::external`]). Inter-stage [`gates`] fail fast and
//! clearly on degenerate output (e.g. too few images registered).

mod dense;
mod embed;
mod gates;
mod ignore_mask;
mod sfm;

use crate::error::Result;
use crate::quality::Quality;
use std::path::{Path, PathBuf};

/// Artifacts produced by a reconstruction run.
#[derive(Debug)]
pub struct Reconstruction {
    /// The textured mesh — a self-contained glTF `.glb` (geometry + UVs +
    /// embedded texture).
    pub textured_mesh: PathBuf,
}

/// Run the reconstruction front-half on a directory of (already preprocessed)
/// images, writing all artifacts under `work_dir`. Returns the textured mesh.
///
/// `masked` indicates the inputs were background-removed onto black, which lets
/// the dense step ignore that background (see [`dense::run`]). `quality` sets the
/// dense working resolution and whether the refinement pass runs. `max_resolution`
/// is the resolved input downscale (so the dense `--max-resolution` cap tracks the
/// images actually present, including a `--max-edge` override).
pub fn run(
    images_dir: &Path,
    work_dir: &Path,
    masked: bool,
    quality: Quality,
    max_resolution: u32,
) -> Result<Reconstruction> {
    std::fs::create_dir_all(work_dir)?;
    // Use an absolute work dir: the OpenMVS tools run with it as their cwd and
    // take absolute file paths, so nothing is re-resolved relative to a working
    // folder (which otherwise doubles the path).
    let work_dir = std::fs::canonicalize(work_dir)?;
    // COLMAP SfM + undistortion -> a COLMAP scene OpenMVS can ingest.
    let colmap_scene = sfm::run(images_dir, &work_dir, quality)?;
    // OpenMVS: COLMAP scene -> dense cloud -> (refine) -> mesh -> textured mesh.
    let textured_mesh = dense::run(&colmap_scene, &work_dir, masked, quality, max_resolution)?;
    Ok(Reconstruction { textured_mesh })
}
