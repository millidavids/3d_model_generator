//! OBJ round-trip for the Blender rebake: write the low-poly mesh + its texture
//! for Blender to import, and read the re-UV'd / re-baked result back.

use crate::error::{Error, Result};
use crate::mesh::Mesh;
use std::path::{Path, PathBuf};

/// Write `mesh` as `<dir>/<stem>.obj` (+ `.mtl` + a copy of its texture as
/// `<stem>_src.png`) for Blender. Returns the `.obj` path.
pub fn write_obj(mesh: &Mesh, dir: &Path, stem: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let obj_path = dir.join(format!("{stem}.obj"));
    let mtl_name = format!("{stem}.mtl");
    let tex_name = format!("{stem}_src.png");

    if let Some(src) = &mesh.texture {
        std::fs::copy(src, dir.join(&tex_name))?;
    }
    std::fs::write(
        dir.join(&mtl_name),
        format!("newmtl mat0\nKd 1 1 1\nmap_Kd {tex_name}\n"),
    )?;

    let mut s = format!("mtllib {mtl_name}\no mesh\nusemtl mat0\n");
    for p in &mesh.positions {
        s.push_str(&format!("v {} {} {}\n", p[0], p[1], p[2]));
    }
    for uv in &mesh.uvs {
        // OBJ's V origin is bottom-left; our UVs are top-left -> flip V.
        s.push_str(&format!("vt {} {}\n", uv[0], 1.0 - uv[1]));
    }
    for t in mesh.indices.chunks_exact(3) {
        // OBJ is 1-indexed; position and UV share the index (vertices are aligned).
        let (a, b, c) = (t[0] + 1, t[1] + 1, t[2] + 1);
        s.push_str(&format!("f {a}/{a} {b}/{b} {c}/{c}\n"));
    }
    std::fs::write(&obj_path, s)?;
    Ok(obj_path)
}

/// Read Blender's rebake output OBJ back into a [`Mesh`], taking the material's
/// diffuse texture (the baked PNG) as the mesh texture.
pub fn read_obj(obj_path: &Path) -> Result<Mesh> {
    let (models, materials) = tobj::load_obj(
        obj_path,
        &tobj::LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
    )
    .map_err(|e| obj_err(e.to_string()))?;

    let mesh = &models
        .first()
        .ok_or_else(|| obj_err("OBJ has no meshes"))?
        .mesh;
    let positions = mesh
        .positions
        .chunks_exact(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect::<Vec<_>>();
    let uvs = if mesh.texcoords.is_empty() {
        vec![[0.0, 0.0]; positions.len()]
    } else {
        // Un-flip V back to our top-left convention.
        mesh.texcoords
            .chunks_exact(2)
            .map(|c| [c[0], 1.0 - c[1]])
            .collect()
    };

    let texture = materials.ok().and_then(|mats| {
        mesh.material_id
            .and_then(|id| mats.get(id))
            .and_then(|m| m.diffuse_texture.clone())
            .map(|t| obj_path.with_file_name(t))
    });

    Ok(Mesh {
        positions,
        uvs,
        indices: mesh.indices.clone(),
        texture,
    })
}

fn obj_err(reason: impl Into<String>) -> Error {
    Error::GateFailed {
        stage: "rebake-obj".into(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{read_obj, write_obj};
    use crate::mesh::Mesh;

    #[test]
    fn round_trips_geometry_and_uv() {
        let dir = tempfile::tempdir().unwrap();
        let tex = dir.path().join("src.png");
        image::RgbImage::from_pixel(4, 4, image::Rgb([10, 20, 30]))
            .save(&tex)
            .unwrap();

        let mesh = Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            uvs: vec![[0.0, 0.2], [1.0, 0.2], [0.0, 0.9]],
            indices: vec![0, 1, 2],
            texture: Some(tex),
        };
        let obj = write_obj(&mesh, dir.path(), "rt").unwrap();
        let back = read_obj(&obj).unwrap();

        assert_eq!(back.triangle_count(), 1);
        assert_eq!(back.positions.len(), 3);

        // The V double-flip (write then read) must restore the original UVs.
        let mut got: Vec<(f32, f32)> = back.uvs.iter().map(|v| (v[0], v[1])).collect();
        let mut want = vec![(0.0, 0.2), (1.0, 0.2), (0.0, 0.9)];
        let key = |p: &(f32, f32)| (p.0 * 1000.0) as i32 * 100_000 + (p.1 * 1000.0) as i32;
        got.sort_by_key(key);
        want.sort_by_key(key);
        for (g, w) in got.iter().zip(&want) {
            assert!(
                (g.0 - w.0).abs() < 1e-4 && (g.1 - w.1).abs() < 1e-4,
                "{g:?} vs {w:?}"
            );
        }
    }
}
