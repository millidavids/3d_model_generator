//! Keep the largest connected component — drops reconstruction floaters and
//! stray disconnected fragments so the asset is just the object.

use crate::mesh::{Mesh, weld};
use std::collections::HashMap;

/// Return a mesh containing only the largest connected component (by triangle
/// count). A mesh with a single component is returned re-welded but unchanged.
pub fn keep_largest_component(mesh: &Mesh) -> Mesh {
    let (verts, indices) = weld(mesh);
    if indices.is_empty() {
        return mesh.clone();
    }

    // Union-find over vertices joined by triangle edges.
    let mut parent: Vec<u32> = (0..verts.len() as u32).collect();
    for tri in indices.chunks_exact(3) {
        union(&mut parent, tri[0], tri[1]);
        union(&mut parent, tri[1], tri[2]);
    }

    // Largest component = the root with the most triangles.
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for tri in indices.chunks_exact(3) {
        *counts.entry(find(&mut parent, tri[0])).or_default() += 1;
    }
    let Some(best) = counts.iter().max_by_key(|&(_, &c)| c).map(|(&r, _)| r) else {
        return mesh.clone();
    };

    let mut kept: Vec<u32> = Vec::new();
    for tri in indices.chunks_exact(3) {
        if find(&mut parent, tri[0]) == best {
            kept.extend_from_slice(tri);
        }
    }

    // Compact to only the referenced vertices.
    let verts = meshopt::optimize_vertex_fetch(&mut kept, &verts);
    Mesh {
        positions: verts.iter().map(|v| v.position).collect(),
        uvs: verts.iter().map(|v| v.uv).collect(),
        indices: kept,
        texture: mesh.texture.clone(),
    }
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
        // a separate triangle far away. UV=0 everywhere so welding is by position.
        let (a, b, c, d) = (
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        );
        let (e, f, g) = ([9.0, 0.0, 0.0], [10.0, 0.0, 0.0], [9.0, 1.0, 0.0]);
        let m = Mesh {
            positions: vec![a, b, c, a, c, d, e, f, g],
            uvs: vec![[0.0, 0.0]; 9],
            indices: (0..9).collect(),
            texture: None,
        };
        // The quad (2 triangles) is the largest component; the floater is dropped.
        assert_eq!(keep_largest_component(&m).triangle_count(), 2);
    }
}
