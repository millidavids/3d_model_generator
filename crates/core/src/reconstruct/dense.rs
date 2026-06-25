//! OpenMVS dense reconstruction, meshing, and texturing (CPU).
//!
//! Each tool runs with `work_dir` as its cwd and takes **absolute** file paths,
//! so nothing is re-resolved against a `-w` working folder (passing both a `-w`
//! and a relative input doubles the path). `RefineMesh` is intentionally skipped
//! (the CUDA-heavy step, unnecessary here).

use crate::error::Result;
use crate::external::{path_str, run_in};
use crate::reconstruct::gates;
use std::path::{Path, PathBuf};

/// Convert the COLMAP scene to OpenMVS, densify, mesh, and texture. Returns the
/// textured-mesh glTF (`.glb`, self-contained with an embedded texture) — DCC
/// tools like Blender import it cleanly, unlike OpenMVS's per-face-UV PLY.
///
/// When `masked` (the inputs were background-removed onto black), the dense step
/// is given a per-image ignore-mask so it skips the black background instead of
/// inventing a webbing membrane in the dark concavities between subject parts
/// (e.g. between a standing person's legs). See [`super::ignore_mask`].
pub fn run(colmap_scene: &Path, work_dir: &Path, masked: bool) -> Result<PathBuf> {
    let scene = work_dir.join("scene.mvs");
    let dense = work_dir.join("scene_dense.mvs");
    let mesh = work_dir.join("scene_mesh.ply");
    let textured = work_dir.join("scene_textured.glb");
    let images = colmap_scene.join("images");
    let mask_dir = work_dir.join("omvs_masks");

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
    let mut dense_args = vec![
        "-i",
        path_str(&scene)?,
        "-o",
        path_str(&dense)?,
        "--resolution-level",
        "2",
        "--max-resolution",
        "1600",
    ];
    // Skip the masked-out (black) background in stereo, so it isn't bridged into a
    // membrane across the gaps between subject parts.
    let mask_dir_str = path_str(&mask_dir)?;
    if masked {
        let n = super::ignore_mask::write_ignore_masks(&images, &mask_dir)?;
        tracing::info!(count = n, "wrote OpenMVS ignore-masks (skip background)");
        dense_args.extend_from_slice(&[
            "--mask-path",
            mask_dir_str,
            "--ignore-mask-label",
            super::ignore_mask::IGNORE_LABEL,
        ]);
    }
    run_in(work_dir, "DensifyPointCloud", &dense_args)?;

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
            // glTF (.glb) imports into Blender with its texture; OpenMVS's default
            // PLY uses per-face UVs that Blender's importer silently drops. (OBJ
            // export segfaults in OpenMVS v2.3.0; glb is self-contained anyway.)
            "--export-type",
            "glb",
        ],
    )?;

    gates::require_nonempty("texture", &textured)?;
    // OpenMVS references the texture as a sidecar PNG; embed it so the .glb is a
    // single self-contained file (and survives `--clean`).
    super::embed::embed_textures(&textured)?;
    Ok(textured)
}
