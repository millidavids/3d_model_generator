//! Back-half orchestration: textured mesh → lo-fi glTF asset.
//!
//! import → heal (largest component) → decimate → [rebake] → normalize →
//! pixelate → export (unlit + nearest).

use crate::error::Result;
use crate::{export, mesh, rebake, texture, validate};
use std::path::{Path, PathBuf};

/// Back-half settings: the lo-fi "budgets" plus the optional Blender rebake.
#[derive(Debug, Clone)]
pub struct LofiConfig {
    /// Target triangle budget for decimation.
    pub target_triangles: u32,
    /// Decimate to the budget (false keeps full resolution).
    pub decimate: bool,
    /// Pixelated texture size (longest edge, px).
    pub texture_size: u32,
    /// Palette colour count for the texture.
    pub palette_colors: u16,
    /// Pixelate (downscale + palette-quantize) the texture.
    pub pixelate: bool,
    /// Keep only the largest connected component (drop floaters).
    pub cleanup: bool,
    /// Center on the origin and scale to a unit bounding box.
    pub normalize: bool,
    /// `Some(blender)` runs the host-native re-UV + rebake quality path.
    pub rebake: Option<PathBuf>,
}

impl Default for LofiConfig {
    fn default() -> Self {
        Self {
            target_triangles: 1500,
            decimate: true,
            texture_size: 128,
            palette_colors: 256,
            pixelate: true,
            cleanup: true,
            normalize: true,
            rebake: None,
        }
    }
}

/// Convert the textured mesh at `mesh_path` into a lo-fi glTF asset at `out`.
pub fn lofi(mesh_path: &Path, out: &Path, cfg: &LofiConfig) -> Result<()> {
    validate::require_file(mesh_path, "input mesh")?;

    let mut m = mesh::load_textured_ply(mesh_path)?;
    tracing::info!(
        triangles = m.triangle_count(),
        vertices = m.vertex_count(),
        "loaded mesh"
    );

    if cfg.cleanup {
        m = mesh::keep_largest_component(&m);
        tracing::info!(triangles = m.triangle_count(), "kept largest component");
    }
    if cfg.decimate {
        m = mesh::decimate(&m, cfg.target_triangles as usize);
        tracing::info!(
            triangles = m.triangle_count(),
            vertices = m.vertex_count(),
            "decimated"
        );
    }
    if let Some(blender) = &cfg.rebake {
        let bake_size = (cfg.texture_size * 4).clamp(256, 2048);
        m = rebake::rebake(&m, &out.with_extension("rebake"), blender, bake_size)?;
        tracing::info!(bake_size, "rebaked via Blender (clean UVs)");
    }
    if cfg.normalize {
        mesh::normalize(&mut m);
        tracing::info!("normalized (centered + unit-scaled)");
    }
    if cfg.pixelate
        && let Some(src) = m.texture.clone()
    {
        let pix = out.with_extension("tex.png");
        texture::pixelate(&src, &pix, cfg.texture_size, cfg.palette_colors)?;
        m.texture = Some(pix);
        tracing::info!(
            size = cfg.texture_size,
            colors = cfg.palette_colors,
            "pixelated texture"
        );
    }

    export::write_glb(&m, out)?;
    tracing::info!(out = %out.display(), "wrote glTF asset");
    Ok(())
}
