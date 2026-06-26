//! Core library for the CUDA-free photogrammetry reconstruction tool.
//!
//! Turns a set of photographs of a real object into a textured 3D mesh, entirely
//! locally and on CPU (no CUDA): preprocess (downscale + optional background
//! mask) → COLMAP Structure-from-Motion → OpenMVS dense / mesh / texture. The
//! heavy lifting runs as external subprocesses — see [`external`]. The output
//! mesh is meant to be consumed downstream (a lo-fi game-asset converter, or any
//! DCC tool such as Blender).
//!
//! UI-agnostic: the [`pipeline`] module is the entry point the `modelgen` CLI
//! drives today, and a future web-service backend can reuse the same functions.

pub mod error;
pub mod external;
pub mod pipeline;
pub mod preprocess;
pub mod quality;
pub mod reconstruct;
pub mod smoke;
pub mod validate;

pub use error::{Error, Result};
pub use pipeline::{ReconstructConfig, reconstruct};
pub use quality::Quality;
