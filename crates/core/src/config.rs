//! Pipeline configuration — the lo-fi "budgets" and tool settings.
//!
//! Serializable so it can be loaded from a TOML profile (Phase 4) and reused by
//! a future web backend.

use serde::{Deserialize, Serialize};

/// Top-level configuration controlling a single pipeline run.
///
/// This is the serializable "profile" form (a future TOML config); the
/// operational settings the pipeline functions take are
/// [`crate::pipeline::ReconstructConfig`] and [`crate::pipeline::LofiConfig`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    /// Target triangle budget for the decimated mesh (PS1-era props: ~300–1500).
    pub target_triangles: u32,
    /// Edge length in pixels of the square output texture; nearest-neighbour
    /// downscaled from the baked atlas.
    pub texture_size: u32,
    /// Maximum number of colours in the quantized palette.
    pub palette_colors: u16,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            target_triangles: 1_000,
            texture_size: 128,
            palette_colors: 256,
        }
    }
}
