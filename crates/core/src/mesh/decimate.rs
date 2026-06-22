//! Mesh decimation to a low-poly budget via meshopt (quadric edge collapse).
//!
//! Welds the exploded mesh into a shared-vertex index buffer (see
//! [`super::weld`]), simplifies to the target triangle count, then compacts so
//! only referenced vertices remain. UVs ride along on the surviving vertices.

use crate::mesh::{Mesh, Vertex, weld};
use meshopt::{SimplifyOptions, VertexDataAdapter};

/// Decimate `mesh` to at most `target_triangles`. A mesh already within budget
/// is returned re-welded (shared vertices) but otherwise unchanged.
pub fn decimate(mesh: &Mesh, target_triangles: usize) -> Mesh {
    let (verts, welded_indices) = weld(mesh);

    let target_index_count = target_triangles * 3;
    let mut indices = if welded_indices.len() > target_index_count {
        let adapter = VertexDataAdapter::new(
            bytemuck::cast_slice(&verts),
            std::mem::size_of::<Vertex>(),
            0,
        )
        .expect("interleaved Vertex is a valid meshopt vertex layout");
        meshopt::simplify(
            &welded_indices,
            &adapter,
            target_index_count,
            0.05, // target error (0..1) — generous; the lo-fi target is forgiving
            SimplifyOptions::empty(),
            None,
        )
    } else {
        welded_indices
    };

    // Keep only the vertices the surviving triangles reference.
    let verts = meshopt::optimize_vertex_fetch(&mut indices, &verts);

    Mesh {
        positions: verts.iter().map(|v| v.position).collect(),
        uvs: verts.iter().map(|v| v.uv).collect(),
        indices,
        texture: mesh.texture.clone(),
    }
}
