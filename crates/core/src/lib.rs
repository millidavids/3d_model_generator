//! Core library for the lo-fi 3D asset generator.
//!
//! Turns a set of photographs of a real object into a low-poly, pixelated glTF
//! game asset (the *Abiotic Factor* / PS1 aesthetic). This crate owns
//! orchestration and the pure-Rust back half of the pipeline; the heavy
//! reconstruction (COLMAP, OpenMVS) and texture baking (Blender) run as
//! external subprocesses — see [`external`].
//!
//! The crate is deliberately UI-agnostic: the [`pipeline`] module is the entry
//! point the `modelgen` CLI drives today, and a future web-service backend can
//! reuse the same functions.

pub mod config;
pub mod error;
pub mod export;
pub mod external;
pub mod mesh;
pub mod pipeline;
pub mod preprocess;
pub mod rebake;
pub mod reconstruct;
pub mod texture;
pub mod validate;

pub use config::PipelineConfig;
pub use error::{Error, Result};
pub use pipeline::{LofiConfig, Pipeline, ReconstructConfig};
