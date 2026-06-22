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
    obj::read_obj(&out_obj)
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
