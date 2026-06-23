//! Pipeline orchestration — photos → textured mesh, reusable by the `modelgen`
//! CLI and a future web backend.
//!
//! preprocess (downscale + optional background mask) → COLMAP SfM → OpenMVS
//! dense / mesh / texture.

mod reconstruct;

pub use reconstruct::{ReconstructConfig, reconstruct};
