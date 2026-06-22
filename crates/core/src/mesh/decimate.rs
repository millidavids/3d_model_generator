//! Mesh decimation to a low-poly budget via meshopt (quadric edge collapse).
//!
//! The imported mesh is "exploded" (no shared vertices), so we first weld
//! identical position+UV vertices into a shared-vertex index buffer (UV seams
//! stay split, since the UV is part of a vertex's identity), then simplify to
//! the target triangle count. UVs ride along on the surviving vertices. Finally
//! we compact so only referenced vertices remain — keeping the asset small.

use crate::mesh::Mesh;
use meshopt::{SimplifyOptions, VertexDataAdapter};

#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    uv: [f32; 2],
}

/// Decimate `mesh` to at most `target_triangles`. A mesh already within budget
/// is returned re-welded (shared vertices) but otherwise unchanged.
pub fn decimate(mesh: &Mesh, target_triangles: usize) -> Mesh {
    let verts: Vec<Vertex> = mesh
        .positions
        .iter()
        .zip(&mesh.uvs)
        .map(|(&position, &uv)| Vertex { position, uv })
        .collect();

    // Weld identical vertices into a shared index buffer.
    let (unique_count, remap) = meshopt::generate_vertex_remap(&verts, Some(&mesh.indices));
    let welded_indices =
        meshopt::remap_index_buffer(Some(&mesh.indices), mesh.indices.len(), &remap);
    let verts = meshopt::remap_vertex_buffer(&verts, unique_count, &remap);

    // Simplify to the target triangle (index) count.
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
