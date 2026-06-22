//! Load the textured PLY OpenMVS writes (vertices + per-face vertex indices and
//! UVs) into a glTF-ready [`Mesh`].

use crate::error::{Error, Result};
use crate::mesh::Mesh;
use ply_rs::parser::Parser;
use ply_rs::ply::{DefaultElement, Property};
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// Load an OpenMVS textured PLY. The texture is assumed to sit beside the PLY as
/// `<stem>0.png` (OpenMVS's single-texture naming).
pub fn load_textured_ply(ply_path: &Path) -> Result<Mesh> {
    let file = std::fs::File::open(ply_path)?;
    let mut reader = BufReader::new(file);
    let ply = Parser::<DefaultElement>::new().read_ply(&mut reader)?;

    let vertices = ply
        .payload
        .get("vertex")
        .ok_or_else(|| gate("PLY has no 'vertex' element"))?;
    let faces = ply
        .payload
        .get("face")
        .ok_or_else(|| gate("PLY has no 'face' element"))?;

    // Source positions, indexed by PLY vertex order.
    let mut src_pos: Vec<[f32; 3]> = Vec::with_capacity(vertices.len());
    for v in vertices {
        src_pos.push([prop_f32(v, "x")?, prop_f32(v, "y")?, prop_f32(v, "z")?]);
    }

    // Explode each triangle into three vertices carrying that face's UVs.
    let mut mesh = Mesh::default();
    for face in faces {
        let idx = prop_indices(face, "vertex_indices")?;
        let tc = prop_texcoord(face, "texcoord")?;
        if idx.len() != 3 || tc.len() != 6 {
            continue; // skip non-triangles / untextured faces
        }
        for k in 0..3 {
            let p = *src_pos
                .get(idx[k])
                .ok_or_else(|| gate("face references a missing vertex"))?;
            mesh.positions.push(p);
            // OpenMVS UVs are bottom-left origin; flip V to our top-left (glTF)
            // convention so the texture maps correctly on export and rebake.
            mesh.uvs.push([tc[k * 2], 1.0 - tc[k * 2 + 1]]);
            mesh.indices.push(mesh.indices.len() as u32);
        }
    }

    if mesh.indices.is_empty() {
        return Err(gate("PLY contained no textured triangles"));
    }
    mesh.texture = derive_texture_path(ply_path);
    Ok(mesh)
}

/// OpenMVS names the single texture `<ply-stem>0.png` next to the mesh.
fn derive_texture_path(ply_path: &Path) -> Option<PathBuf> {
    let stem = ply_path.file_stem()?.to_str()?;
    let tex = ply_path.with_file_name(format!("{stem}0.png"));
    tex.exists().then_some(tex)
}

fn gate(reason: &str) -> Error {
    Error::GateFailed {
        stage: "mesh-import".to_string(),
        reason: reason.to_string(),
    }
}

fn prop_f32(el: &DefaultElement, key: &str) -> Result<f32> {
    match el.get(key) {
        Some(Property::Float(v)) => Ok(*v),
        Some(Property::Double(v)) => Ok(*v as f32),
        _ => Err(gate(&format!(
            "vertex property '{key}' missing / not a float"
        ))),
    }
}

/// Vertex-index list (OpenMVS uses signed int; accept unsigned too).
fn prop_indices(el: &DefaultElement, key: &str) -> Result<Vec<usize>> {
    match el.get(key) {
        Some(Property::ListInt(v)) => Ok(v.iter().map(|&i| i as usize).collect()),
        Some(Property::ListUInt(v)) => Ok(v.iter().map(|&i| i as usize).collect()),
        _ => Err(gate(&format!(
            "face property '{key}' missing / not an int list"
        ))),
    }
}

/// Per-face texture coordinates (6 floats = 3 UV pairs).
fn prop_texcoord(el: &DefaultElement, key: &str) -> Result<Vec<f32>> {
    match el.get(key) {
        Some(Property::ListFloat(v)) => Ok(v.clone()),
        Some(Property::ListDouble(v)) => Ok(v.iter().map(|&x| x as f32).collect()),
        _ => Err(gate(&format!(
            "face property '{key}' missing / not a float list"
        ))),
    }
}
