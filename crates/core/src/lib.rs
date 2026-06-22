//! Core library for the lo-fi 3D asset generator.
//!
//! Turns a set of photographs of a real object into a low-poly, pixelated glTF
//! game asset (the *Abiotic Factor* / PS1 aesthetic). This crate owns
//! orchestration and the pure-Rust back half of the pipeline; the heavy
//! reconstruction (COLMAP, OpenMVS) and texture baking (Blender) run as
//! external subprocesses — see [`external`].
//!
//! The crate is deliberately UI-agnostic so a future web-service backend can
//! reuse it as a library, exactly as the [`crate::pipeline::Pipeline`] is used
//! by the `modelgen` CLI today.

pub mod config;
pub mod error;
pub mod export;
pub mod external;
pub mod mesh;
pub mod pipeline;
pub mod preprocess;
pub mod reconstruct;

pub use config::PipelineConfig;
pub use error::{Error, Result};
pub use pipeline::Pipeline;
