//! Pipeline orchestration: the [`Stage`] trait and the [`Pipeline`] runner.
//!
//! Each stage consumes the previous stage's output and may apply a gate check
//! before the next stage proceeds (so a bad object fails fast and gracefully
//! rather than wasting reconstruction time downstream). Concrete stages are
//! added incrementally across the build phases:
//!
//! preprocess → reconstruct → import → heal → normalize → decimate →
//! re-UV + bake → lo-fi texture → export.

use crate::config::PipelineConfig;
use crate::error::Result;
use std::path::{Path, PathBuf};

/// A single step in the photos→asset pipeline.
pub trait Stage {
    /// Human-readable stage name, used in logs and gate-failure messages.
    fn name(&self) -> &'static str;
}

/// Orchestrates the full photos→glTF pipeline for one object.
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

    /// Run the full pipeline on a directory of input photos, writing the
    /// resulting glTF asset and returning its path.
    ///
    /// TODO(phase 1–3): wire up the concrete stages. Currently a stub so the
    /// workspace compiles and the CLI surface exists.
    pub fn run(&self, _input_dir: &Path, _output: &Path) -> Result<PathBuf> {
        tracing::warn!("pipeline stages are not implemented yet (Phase 0 skeleton)");
        todo!("implemented across phases 1–3")
    }
}
