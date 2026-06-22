//! Normalize geometry: center on the origin and scale to a unit bounding box.
//!
//! Orientation is intentionally left untouched — a photogrammetry coordinate
//! frame is gauge-free, so "up" can't be inferred reliably from the mesh alone.
//! Orient in-engine, or via a future fiducial-marker feature.

use crate::mesh::Mesh;

/// Center `mesh` on the origin and scale so its longest bounding-box edge is 1.
pub fn normalize(mesh: &mut Mesh) {
    if mesh.positions.is_empty() {
        return;
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in &mesh.positions {
        for (i, &c) in p.iter().enumerate() {
            min[i] = min[i].min(c);
            max[i] = max[i].max(c);
        }
    }
    let center = [
        (min[0] + max[0]) / 2.0,
        (min[1] + max[1]) / 2.0,
        (min[2] + max[2]) / 2.0,
    ];
    let longest = (max[0] - min[0])
        .max(max[1] - min[1])
        .max(max[2] - min[2])
        .max(f32::EPSILON);
    let scale = 1.0 / longest;
    for p in &mut mesh.positions {
        for (i, c) in p.iter_mut().enumerate() {
            *c = (*c - center[i]) * scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::normalize;
    use crate::mesh::Mesh;

    #[test]
    fn centers_on_origin_and_scales_longest_edge_to_one() {
        // bbox (1,2,3)..(3,6,7): extent (2,4,4), longest 4 (the Y axis).
        let mut m = Mesh {
            positions: vec![[1.0, 2.0, 3.0], [3.0, 6.0, 7.0]],
            uvs: vec![[0.0, 0.0]; 2],
            indices: vec![],
            texture: None,
        };
        normalize(&mut m);
        let (p0, p1) = (m.positions[0], m.positions[1]);
        // centered: each axis midpoint ~ 0.
        for i in 0..3 {
            assert!(((p0[i] + p1[i]) / 2.0).abs() < 1e-6);
        }
        // longest edge (Y) is exactly 1, and no axis exceeds 1.
        assert!((p1[1] - p0[1] - 1.0).abs() < 1e-6);
        assert!((p1[0] - p0[0]) <= 1.0 + 1e-6);
    }
}
