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

#[cfg(test)]
mod tests {
    use super::decimate;
    use crate::mesh::Mesh;

    #[test]
    fn respects_the_triangle_budget() {
        // A 16x16 grid of quads = 512 triangles (exploded).
        let n = 16;
        let (mut positions, mut uvs) = (Vec::new(), Vec::new());
        for y in 0..n {
            for x in 0..n {
                let (x0, y0, x1, y1) = (x as f32, y as f32, (x + 1) as f32, (y + 1) as f32);
                for p in [
                    [x0, y0, 0.0],
                    [x1, y0, 0.0],
                    [x1, y1, 0.0],
                    [x0, y0, 0.0],
                    [x1, y1, 0.0],
                    [x0, y1, 0.0],
                ] {
                    positions.push(p);
                    uvs.push([0.0, 0.0]);
                }
            }
        }
        let m = Mesh {
            indices: (0..positions.len() as u32).collect(),
            positions,
            uvs,
            texture: None,
        };
        assert_eq!(m.triangle_count(), 512);

        let out = decimate(&m, 50);
        assert!(out.triangle_count() <= 50, "{} > 50", out.triangle_count());
        assert!(out.triangle_count() > 0);
    }
}
