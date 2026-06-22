//! The in-memory mesh the lo-fi back-half operates on, plus import from the
//! OpenMVS textured PLY and the geometry transforms (heal, decimate, normalize).
//!
//! Stored glTF-ready: per-vertex positions + UVs with an index buffer (glTF has
//! no per-face attributes). OpenMVS emits per-*face* UVs, so on import the mesh
//! is "exploded" — three fresh vertices per triangle — which [`weld`] then
//! re-shares for the topology-aware transforms.

mod decimate;
mod heal;
mod import;
mod normalize;

pub use decimate::decimate;
pub use heal::keep_largest_component;
pub use import::load_textured_ply;
pub use normalize::normalize;

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

/// Interleaved vertex (position + UV) used for welding/topology operations. The
/// UV is part of a vertex's identity, so welding preserves UV seams.
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

/// Weld a mesh's (exploded) vertices into a shared-vertex index buffer.
/// Returns the deduplicated vertices and the index buffer referencing them.
pub(crate) fn weld(mesh: &Mesh) -> (Vec<Vertex>, Vec<u32>) {
    let verts: Vec<Vertex> = mesh
        .positions
        .iter()
        .zip(&mesh.uvs)
        .map(|(&position, &uv)| Vertex { position, uv })
        .collect();
    let (unique, remap) = meshopt::generate_vertex_remap(&verts, Some(&mesh.indices));
    let indices = meshopt::remap_index_buffer(Some(&mesh.indices), mesh.indices.len(), &remap);
    let verts = meshopt::remap_vertex_buffer(&verts, unique, &remap);
    (verts, indices)
}
