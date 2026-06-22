//! End-to-end orchestration: photos → lo-fi glTF asset (reconstruct + lofi).

use super::{LofiConfig, ReconstructConfig, lofi, reconstruct};
use crate::config::PipelineConfig;
use crate::error::Result;
use std::path::Path;

/// Run the full pipeline on `photos`: reconstruct a textured mesh in `work`,
/// then convert it to the lo-fi asset at `out`.
pub fn process(
    photos: &Path,
    work: &Path,
    out: &Path,
    recon: &ReconstructConfig,
    lofi_cfg: &LofiConfig,
) -> Result<()> {
    let mesh = reconstruct(photos, work, recon)?;
    lofi(&mesh, out, lofi_cfg)
}

/// Convenience wrapper that owns a [`PipelineConfig`]. A future web backend can
/// drive the pipeline through this instead of the free functions.
pub struct Pipeline {
    config: PipelineConfig,
}

impl Pipeline {
    /// Create a pipeline with the given configuration.
    pub fn new(config: PipelineConfig) -> Self {
        Self { config }
    }

    /// The configuration this pipeline runs with.
    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }

    /// Reconstruct + lo-fi `photos` into `out`, using `work` for intermediates.
    pub fn run(&self, photos: &Path, work: &Path, out: &Path) -> Result<()> {
        let lofi_cfg = LofiConfig {
            target_triangles: self.config.target_triangles,
            texture_size: self.config.texture_size,
            palette_colors: self.config.palette_colors,
            ..LofiConfig::default()
        };
        process(photos, work, out, &ReconstructConfig::default(), &lofi_cfg)
    }
}
