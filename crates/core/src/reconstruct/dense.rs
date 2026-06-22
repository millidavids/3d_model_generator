//! OpenMVS dense reconstruction, meshing, and texturing (CPU).
//!
//! Each tool runs with `work_dir` as its cwd and takes **absolute** file paths,
//! so nothing is re-resolved against a `-w` working folder (passing both a `-w`
//! and a relative input doubles the path). `RefineMesh` is intentionally skipped
//! (the CUDA-heavy step, unnecessary for the lo-fi target).

use crate::error::Result;
use crate::external::{path_str, run_in};
use crate::reconstruct::gates;
use std::path::{Path, PathBuf};

/// Convert the COLMAP scene to OpenMVS, densify, mesh, and texture. Returns the
/// textured-mesh PLY (a sibling texture image is written alongside it).
pub fn run(colmap_scene: &Path, work_dir: &Path) -> Result<PathBuf> {
    let scene = work_dir.join("scene.mvs");
    let dense = work_dir.join("scene_dense.mvs");
    let mesh = work_dir.join("scene_mesh.ply");
    let textured = work_dir.join("scene_textured.ply");
    let images = colmap_scene.join("images");

    // COLMAP scene -> OpenMVS scene (records undistorted image paths).
    run_in(
        work_dir,
        "InterfaceCOLMAP",
        &[
            "-i",
            path_str(colmap_scene)?,
            "--image-folder",
            path_str(&images)?,
            "-o",
            path_str(&scene)?,
        ],
    )?;

    // Dense point cloud. `resolution-level` scales images down (much faster on
    // CPU, plenty of detail for lo-fi).
    run_in(
        work_dir,
        "DensifyPointCloud",
        &[
            "-i",
            path_str(&scene)?,
            "-o",
            path_str(&dense)?,
            "--resolution-level",
            "2",
            "--max-resolution",
            "1600",
        ],
    )?;

    // Surface mesh from the dense cloud.
    run_in(
        work_dir,
        "ReconstructMesh",
        &["-i", path_str(&dense)?, "-o", path_str(&mesh)?],
    )?;

    // Texture the mesh using the source images.
    run_in(
        work_dir,
        "TextureMesh",
        &[
            "-i",
            path_str(&dense)?,
            "-m",
            path_str(&mesh)?,
            "-o",
            path_str(&textured)?,
        ],
    )?;

    gates::require_nonempty("texture", &textured)?;
    Ok(textured)
}
