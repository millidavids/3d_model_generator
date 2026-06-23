//! Keep the largest connected component — drops reconstruction floaters and
//! stray disconnected fragments so the asset is just the object.
//!
//! Connectivity is computed on POSITION only. An OpenMVS textured mesh is split
//! into thousands of UV/atlas patches; welding by position+UV (as decimation
//! does) would treat every patch seam as a disconnection and shatter one solid
//! surface into tiny components. We weld positions for true geometric
//! connectivity, then keep the largest component's original triangles with
//! their UVs intact.

use crate::mesh::Mesh;
use std::collections::HashMap;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PosVertex {
    position: [f32; 3],
}

/// Return a mesh containing only the largest connected component (by triangle
/// count). A mesh that is already one component is returned unchanged.
pub fn keep_largest_component(mesh: &Mesh) -> Mesh {
    if mesh.indices.is_empty() {
        return mesh.clone();
    }

    // Weld by position only -> a canonical id per distinct location.
    let pos: Vec<PosVertex> = mesh
        .positions
        .iter()
        .map(|&position| PosVertex { position })
        .collect();
    let (unique, remap) = meshopt::generate_vertex_remap(&pos, Some(&mesh.indices));

    // Union-find over distinct positions, joined by triangle edges.
    let mut parent: Vec<u32> = (0..unique as u32).collect();
    for tri in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (
            remap[tri[0] as usize],
            remap[tri[1] as usize],
            remap[tri[2] as usize],
        );
        union(&mut parent, a, b);
        union(&mut parent, b, c);
    }

    // Largest component = the root with the most triangles.
    let mut counts: HashMap<u32, u32> = HashMap::new();
    for tri in mesh.indices.chunks_exact(3) {
        *counts
            .entry(find(&mut parent, remap[tri[0] as usize]))
            .or_default() += 1;
    }
    let Some(best) = counts.iter().max_by_key(|&(_, &c)| c).map(|(&r, _)| r) else {
        return mesh.clone();
    };

    // Keep that component's original triangles (UVs intact). The mesh is
    // exploded (3 verts/triangle), so each kept triangle carries its own verts.
    let mut out = Mesh {
        texture: mesh.texture.clone(),
        ..Mesh::default()
    };
    for tri in mesh.indices.chunks_exact(3) {
        if find(&mut parent, remap[tri[0] as usize]) == best {
            for &vi in tri {
                out.indices.push(out.positions.len() as u32);
                out.positions.push(mesh.positions[vi as usize]);
                out.uvs.push(mesh.uvs[vi as usize]);
            }
        }
    }
    out
}

fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        parent[x as usize] = parent[parent[x as usize] as usize]; // path halving
        x = parent[x as usize];
    }
    x
}

fn union(parent: &mut [u32], a: u32, b: u32) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra as usize] = rb;
    }
}

#[cfg(test)]
mod tests {
    use super::keep_largest_component;
    use crate::mesh::Mesh;

    #[test]
    fn drops_the_smaller_disconnected_fragment() {
        // Component A: a quad (2 triangles sharing positions a,c). Component B:
        // a separate triangle far away.
        let (a, b, c, d) = (
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        );
        let (e, f, g) = ([9.0, 0.0, 0.0], [10.0, 0.0, 0.0], [9.0, 1.0, 0.0]);
        let mesh = Mesh {
            positions: vec![a, b, c, a, c, d, e, f, g],
            // Distinct UVs across the shared quad edge (an atlas seam) must NOT
            // split the quad — connectivity is position-only.
            uvs: vec![
                [0.0, 0.0],
                [0.1, 0.0],
                [0.2, 0.0],
                [0.7, 0.7],
                [0.8, 0.8],
                [0.9, 0.9],
                [0.0, 0.0],
                [0.0, 0.0],
                [0.0, 0.0],
            ],
            indices: (0..9).collect(),
            texture: None,
        };
        // The quad (2 triangles) is the largest component; the floater is dropped.
        assert_eq!(keep_largest_component(&mesh).triangle_count(), 2);
    }
}
