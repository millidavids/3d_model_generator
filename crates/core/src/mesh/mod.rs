//! The in-memory mesh the lo-fi back-half operates on, plus import from the
//! OpenMVS textured PLY.
//!
//! Stored glTF-ready: per-vertex positions + UVs with an index buffer (glTF has
//! no per-face attributes). OpenMVS emits per-*face* UVs, so on import the mesh
//! is "exploded" — three fresh vertices per triangle — which the later
//! decimation step re-indexes and shrinks.

mod decimate;
mod import;

pub use decimate::decimate;
pub use import::load_textured_ply;

use std::path::PathBuf;

/// A triangle mesh with one UV set and an optional base-colour texture.
#[derive(Debug, Default, Clone)]
pub struct Mesh {
    /// Vertex positions.
    pub positions: Vec<[f32; 3]>,
    /// Per-vertex texture coordinates (same length as `positions`).
    pub uvs: Vec<[f32; 2]>,
    /// Triangle list — indices into `positions`/`uvs` (length is a multiple of 3).
    pub indices: Vec<u32>,
    /// Path to the base-colour texture image, if any.
    pub texture: Option<PathBuf>,
}

impl Mesh {
    /// Number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }
}
