//! Optional host-native Blender re-UV + texture rebake (the "quality" path).
//!
//! Runs on the **host**, where native Blender lives (Metal GPU on Apple Silicon;
//! there is no arm64-Linux Blender for the container). It exports the low-poly
//! mesh to OBJ, has Blender Smart-UV-unwrap it and bake the original texture
//! onto the clean new layout — fixing the uneven texel density of a decimated
//! kept-atlas mesh — then reads the result back. Headless:
//! `blender --background --python rebake.py -- in.obj out.obj <size>`.

mod obj;

use crate::error::{Error, Result};
use crate::external;
use crate::mesh::Mesh;
use std::path::{Path, PathBuf};

/// The Blender script, embedded so the binary is self-contained.
const REBAKE_SCRIPT: &str = include_str!("../../../../scripts/rebake.py");

/// Re-UV `mesh` and rebake its texture (to `tex_size` px) via native Blender at
/// `blender`. Returns a new mesh with a clean UV layout and a freshly-baked
/// texture; scratch files are written under `work_dir`.
pub fn rebake(mesh: &Mesh, work_dir: &Path, blender: &Path, tex_size: u32) -> Result<Mesh> {
    if mesh.texture.is_none() {
        return Err(err("mesh has no texture to rebake"));
    }
    std::fs::create_dir_all(work_dir)?;

    let in_obj = obj::write_obj(mesh, work_dir, "rebake_in")?;
    let out_obj = work_dir.join("rebake_out.obj");
    let script = work_dir.join("rebake.py");
    std::fs::write(&script, REBAKE_SCRIPT)?;

    let blender = external::path_str(blender)?;
    let (script, in_obj, out_obj_s) = (
        external::path_str(&script)?,
        external::path_str(&in_obj)?,
        external::path_str(&out_obj)?,
    );
    let size = tex_size.to_string();
    external::run(
        blender,
        &[
            "--background",
            "--python",
            script,
            "--",
            in_obj,
            out_obj_s,
            &size,
        ],
    )?;

    if !out_obj.exists() {
        return Err(err("Blender did not produce the rebaked OBJ"));
    }
    let mesh = obj::read_obj(&out_obj)?;
    if let Some(tex) = &mesh.texture {
        gate_baked_texture(tex)?;
    }
    Ok(mesh)
}

/// Catch a failed bake — e.g. the near-constant texture you get when Blender
/// doesn't pick up the bake-target node — before it ships inside an asset.
fn gate_baked_texture(path: &Path) -> Result<()> {
    let img = image::open(path)
        .map_err(|e| err(format!("baked texture unreadable: {e}")))?
        .to_rgb8();
    if distinct_surface_colors(&img) < HEALTHY_COLOR_COUNT {
        return Err(err(
            "baked texture looks degenerate (near-constant) — the Blender bake likely failed"
                .to_string(),
        ));
    }
    Ok(())
}

/// A healthy bake of a real object easily clears this many distinct colours; a
/// failed (constant) bake has 1–3.
const HEALTHY_COLOR_COUNT: usize = 16;

/// Count distinct non-black (non-atlas-background) colours, saturating at
/// [`HEALTHY_COLOR_COUNT`].
fn distinct_surface_colors(img: &image::RgbImage) -> usize {
    let mut colors = std::collections::HashSet::new();
    for px in img.pixels().step_by(7) {
        if px.0 != [0, 0, 0] {
            colors.insert(px.0);
            if colors.len() >= HEALTHY_COLOR_COUNT {
                break;
            }
        }
    }
    colors.len()
}

/// Locate a Blender binary: `$BLENDER`, then `PATH`, then the macOS app bundle.
pub fn find_blender() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BLENDER") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    if external::is_on_path("blender") {
        return Some(PathBuf::from("blender"));
    }
    let app = PathBuf::from("/Applications/Blender.app/Contents/MacOS/Blender");
    app.exists().then_some(app)
}

fn err(reason: impl Into<String>) -> Error {
    Error::GateFailed {
        stage: "rebake".into(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{HEALTHY_COLOR_COUNT, distinct_surface_colors};
    use image::{Rgb, RgbImage};

    #[test]
    fn flags_a_constant_bake_as_degenerate() {
        // All one colour (the orange-fill failure mode) -> 1 distinct colour.
        let solid = RgbImage::from_pixel(64, 64, Rgb([255, 127, 0]));
        assert!(distinct_surface_colors(&solid) < HEALTHY_COLOR_COUNT);

        // A varied texture clears the threshold.
        let mut varied = RgbImage::new(64, 64);
        for (i, px) in varied.pixels_mut().enumerate() {
            *px = Rgb([(i % 251) as u8, (i * 3 % 251) as u8, (i * 7 % 251) as u8]);
        }
        assert!(distinct_surface_colors(&varied) >= HEALTHY_COLOR_COUNT);
    }
}
