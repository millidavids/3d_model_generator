//! Reconstruction front-half: photos -> textured mesh.
//!
//! COLMAP performs Structure-from-Motion (camera poses) and image undistortion;
//! OpenMVS then densifies, meshes, and textures. Every step runs as a CPU
//! subprocess (see [`crate::external`]). Inter-stage [`gates`] fail fast and
//! clearly on degenerate output (e.g. too few images registered).

mod dense;
mod embed;
mod gates;
mod sfm;

use crate::error::Result;
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
pub fn run(images_dir: &Path, work_dir: &Path) -> Result<Reconstruction> {
    std::fs::create_dir_all(work_dir)?;
    // Use an absolute work dir: the OpenMVS tools run with it as their cwd and
    // take absolute file paths, so nothing is re-resolved relative to a working
    // folder (which otherwise doubles the path).
    let work_dir = std::fs::canonicalize(work_dir)?;
    // COLMAP SfM + undistortion -> a COLMAP scene OpenMVS can ingest.
    let colmap_scene = sfm::run(images_dir, &work_dir)?;
    // OpenMVS: COLMAP scene -> dense cloud -> mesh -> textured mesh.
    let textured_mesh = dense::run(&colmap_scene, &work_dir)?;
    Ok(Reconstruction { textured_mesh })
}
