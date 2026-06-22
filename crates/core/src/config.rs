//! Pipeline configuration — the lo-fi "budgets" and tool settings.
//!
//! Serializable so it can be loaded from a TOML profile (Phase 4) and reused by
//! a future web backend.

use serde::{Deserialize, Serialize};

/// Top-level configuration controlling a single pipeline run.
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
    /// Which Blender bake backend to use.
    pub bake: BakeBackend,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            target_triangles: 1_000,
            texture_size: 128,
            palette_colors: 256,
            bake: BakeBackend::default(),
        }
    }
}

/// Where the Blender bake runs.
///
/// Defaults to running inside the container on CPU (clone-and-go). On the
/// Apple-Silicon Mac the host-native backend is preferred — it uses the
/// official macOS Blender (Metal GPU) and sidesteps the absence of an official
/// arm64-Linux Blender build.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BakeBackend {
    /// Blender inside the container, CPU.
    #[default]
    ContainerCpu,
    /// Native Blender on the host (GPU; also the arm64-macOS path).
    HostNative,
}
