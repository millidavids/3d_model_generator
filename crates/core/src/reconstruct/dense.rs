//! OpenMVS dense reconstruction, meshing, and texturing (CPU).
//!
//! Each tool runs with `work_dir` as its cwd and takes **absolute** file paths,
//! so nothing is re-resolved against a `-w` working folder (passing both a `-w`
//! and a relative input doubles the path). The optional `RefineMesh`
//! photoconsistency pass runs only at the `High` [`Quality`] preset — it is
//! CPU-bound here, so it is the slow part when enabled.

use crate::error::Result;
use crate::external::{path_str, run_in};
use crate::quality::Quality;
use crate::reconstruct::gates;
use std::path::{Path, PathBuf};

/// `RefineMesh --resolution-level` when refinement runs. Full resolution (0) is
/// impractically slow on CPU, so refine one scale down. Unlike `DensifyPointCloud`,
/// `RefineMesh` has no absolute `--max-resolution` ceiling, so this scale-down is
/// the only bound on its working resolution (and thus its memory).
const REFINE_RESOLUTION_LEVEL: &str = "1";

/// A refined mesh below this face count is treated as a degenerate/collapsed
/// `RefineMesh` result rather than a real surface (a stronger check than non-empty).
const MIN_REFINED_FACES: usize = 100;

/// Convert the COLMAP scene to OpenMVS, densify, (optionally refine,) mesh, and
/// texture. Returns the textured-mesh glTF (`.glb`, self-contained with an
/// embedded texture) — DCC tools like Blender import it cleanly, unlike OpenMVS's
/// per-face-UV PLY.
///
/// When `masked` (the inputs were background-removed onto black), the dense step
/// is given a per-image ignore-mask so it skips the black background instead of
/// inventing a webbing membrane in the dark concavities between subject parts
/// (e.g. between a standing person's legs). See [`super::ignore_mask`].
///
/// `quality` sets the dense step's `--resolution-level` and whether the
/// `RefineMesh` pass sharpens the surface against the images before texturing.
/// `max_resolution` is the dense step's absolute resolution ceiling — the caller
/// passes the *resolved* input downscale (`ReconstructConfig::max_edge`, which
/// `--max-edge` may override), so the cap always matches the images present.
pub fn run(
    colmap_scene: &Path,
    work_dir: &Path,
    masked: bool,
    quality: Quality,
    max_resolution: u32,
) -> Result<PathBuf> {
    let scene = work_dir.join("scene.mvs");
    let dense = work_dir.join("scene_dense.mvs");
    let mesh = work_dir.join("scene_mesh.ply");
    let refined = work_dir.join("scene_mesh_refined.ply");
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

    // Dense point cloud. `resolution-level` scales images down before densifying
    // (the `quality` preset trades that working resolution for speed);
    // `max-resolution` is the absolute ceiling, tracking the resolved input edge.
    // Skip the masked-out (black) background in stereo (`--mask-path`), so it
    // isn't bridged into a membrane across the gaps between subject parts.
    let ignore_mask_dir = if masked {
        let n = super::ignore_mask::write_ignore_masks(&images, &mask_dir)?;
        tracing::info!(count = n, "wrote OpenMVS ignore-masks (skip background)");
        Some(path_str(&mask_dir)?)
    } else {
        None
    };
    let args = dense_args(
        path_str(&scene)?,
        path_str(&dense)?,
        quality.dense_resolution_level(),
        max_resolution,
        ignore_mask_dir,
    );
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_in(work_dir, "DensifyPointCloud", &arg_refs)?;

    // Surface mesh from the dense cloud.
    run_in(
        work_dir,
        "ReconstructMesh",
        &["-i", path_str(&dense)?, "-o", path_str(&mesh)?],
    )?;

    // Optional photoconsistency refinement: deform the surface to better match the
    // images (sharper detail). CPU-bound, so it runs only at the High preset; the
    // refined mesh then replaces the raw one for texturing.
    let mesh_to_texture = if quality.refine() {
        run_in(
            work_dir,
            "RefineMesh",
            &[
                "-i",
                path_str(&dense)?,
                "-m",
                path_str(&mesh)?,
                "-o",
                path_str(&refined)?,
                "--resolution-level",
                REFINE_RESOLUTION_LEVEL,
            ],
        )?;
        // RefineMesh can emit a non-empty-but-collapsed PLY on hard objects; require
        // real geometry so a degenerate refine fails here, not silently in texturing.
        gates::require_ply_faces("refine", &refined, MIN_REFINED_FACES)?;
        &refined
    } else {
        &mesh
    };

    // Texture the mesh using the source images.
    run_in(
        work_dir,
        "TextureMesh",
        &[
            "-i",
            path_str(&dense)?,
            "-m",
            path_str(mesh_to_texture)?,
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

/// Build the `DensifyPointCloud` argument list. Pure (owned `String`s, no I/O) so
/// the resolution + ignore-mask wiring is unit-testable without invoking OpenMVS.
fn dense_args(
    scene: &str,
    dense: &str,
    resolution_level: u32,
    max_resolution: u32,
    ignore_mask_dir: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "-i".to_string(),
        scene.to_string(),
        "-o".to_string(),
        dense.to_string(),
        "--resolution-level".to_string(),
        resolution_level.to_string(),
        "--max-resolution".to_string(),
        max_resolution.to_string(),
    ];
    if let Some(dir) = ignore_mask_dir {
        args.extend([
            "--mask-path".to_string(),
            dir.to_string(),
            "--ignore-mask-label".to_string(),
            super::ignore_mask::IGNORE_LABEL.to_string(),
        ]);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::dense_args;

    /// The value just after `flag` in the arg list (if present).
    fn val<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    }

    #[test]
    fn dense_args_thread_resolution_and_max_resolution() {
        // max_resolution is the *caller's* resolved value, not derived from a preset
        // — this is what keeps a `--max-edge` override in step with the dense cap.
        let a = dense_args("s.mvs", "d.mvs", 1, 2400, None);
        assert_eq!(val(&a, "--resolution-level"), Some("1"));
        assert_eq!(val(&a, "--max-resolution"), Some("2400"));
        assert_eq!(val(&a, "-i"), Some("s.mvs"));
        assert_eq!(val(&a, "-o"), Some("d.mvs"));
        assert!(
            !a.iter().any(|s| s == "--mask-path"),
            "no mask args unmasked"
        );
    }

    #[test]
    fn dense_args_add_ignore_mask_when_masked() {
        let a = dense_args("s.mvs", "d.mvs", 2, 1600, Some("/work/omvs_masks"));
        assert_eq!(val(&a, "--max-resolution"), Some("1600"));
        assert_eq!(val(&a, "--mask-path"), Some("/work/omvs_masks"));
        assert_eq!(val(&a, "--ignore-mask-label"), Some("0"));
    }
}
