//! Write a [`Mesh`] to a self-contained binary glTF (`.glb`): geometry plus the
//! base-colour texture embedded in the BIN chunk, with a **nearest-neighbour
//! sampler** and an **unlit material** — the lo-fi look (hard pixels, no
//! bilinear smoothing, no shading; vertex-jitter / affine-warp stay the engine's
//! job).
//!
//! Built directly with `serde_json` + a hand-assembled GLB container: simpler
//! and gives full control over the unlit extension and sampler, versus the
//! typed `gltf-json` builder.

use crate::error::{Error, Result};
use crate::mesh::Mesh;
use serde_json::json;
use std::path::Path;

// glTF enum constants.
const F32: u32 = 5126; // FLOAT
const U32: u32 = 5125; // UNSIGNED_INT
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const NEAREST: u32 = 9728;
const REPEAT: u32 = 10497;
const TRIANGLES: u32 = 4;

/// Write `mesh` to `out` as a `.glb`. Requires `mesh.texture` to be set.
pub fn write_glb(mesh: &Mesh, out: &Path) -> Result<()> {
    let texture = mesh.texture.as_ref().ok_or_else(|| Error::GateFailed {
        stage: "export".into(),
        reason: "mesh has no texture to embed".into(),
    })?;

    // --- assemble the single binary buffer (positions, uvs, indices, image) ---
    let mut bin: Vec<u8> = Vec::new();
    let (pos_off, pos_len) = push(&mut bin, mesh.positions.iter().flatten());
    let (uv_off, uv_len) = push(&mut bin, mesh.uvs.iter().flatten());
    let (idx_off, idx_len) = push_u32(&mut bin, &mesh.indices);

    let img = std::fs::read(texture)?;
    let img_off = bin.len();
    let img_len = img.len();
    bin.extend_from_slice(&img);
    pad4(&mut bin);

    let (min, max) = bbox(&mesh.positions);

    let gltf = json!({
        "asset": { "version": "2.0", "generator": "modelgen" },
        "extensionsUsed": ["KHR_materials_unlit"],
        "buffers": [ { "byteLength": bin.len() } ],
        "bufferViews": [
            { "buffer": 0, "byteOffset": pos_off, "byteLength": pos_len, "target": ARRAY_BUFFER },
            { "buffer": 0, "byteOffset": uv_off,  "byteLength": uv_len,  "target": ARRAY_BUFFER },
            { "buffer": 0, "byteOffset": idx_off, "byteLength": idx_len, "target": ELEMENT_ARRAY_BUFFER },
            { "buffer": 0, "byteOffset": img_off, "byteLength": img_len }
        ],
        "accessors": [
            { "bufferView": 0, "componentType": F32, "count": mesh.positions.len(), "type": "VEC3", "min": min, "max": max },
            { "bufferView": 1, "componentType": F32, "count": mesh.uvs.len(),       "type": "VEC2" },
            { "bufferView": 2, "componentType": U32, "count": mesh.indices.len(),   "type": "SCALAR" }
        ],
        "images":   [ { "bufferView": 3, "mimeType": "image/png" } ],
        "samplers": [ { "magFilter": NEAREST, "minFilter": NEAREST, "wrapS": REPEAT, "wrapT": REPEAT } ],
        "textures": [ { "sampler": 0, "source": 0 } ],
        "materials": [ {
            "pbrMetallicRoughness": { "baseColorTexture": { "index": 0 }, "metallicFactor": 0.0, "roughnessFactor": 1.0 },
            "extensions": { "KHR_materials_unlit": {} }
        } ],
        "meshes": [ { "primitives": [ {
            "attributes": { "POSITION": 0, "TEXCOORD_0": 1 },
            "indices": 2, "material": 0, "mode": TRIANGLES
        } ] } ],
        "nodes":  [ { "mesh": 0 } ],
        "scenes": [ { "nodes": [0] } ],
        "scene": 0
    });

    write_container(&gltf, &bin, out)
}

/// Assemble the 12-byte header + JSON chunk + BIN chunk and write the `.glb`.
fn write_container(gltf: &serde_json::Value, bin: &[u8], out: &Path) -> Result<()> {
    let mut json_bytes =
        serde_json::to_vec(gltf).map_err(|e| std::io::Error::other(e.to_string()))?;
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' '); // JSON chunk padded with spaces
    }
    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();

    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    // JSON chunk
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json_bytes);
    // BIN chunk
    glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(bin);

    std::fs::write(out, glb)?;
    Ok(())
}

fn pad4(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
}

/// Append little-endian f32s, returning (byte offset, byte length); pads to 4.
fn push<'a>(bin: &mut Vec<u8>, data: impl Iterator<Item = &'a f32>) -> (usize, usize) {
    let off = bin.len();
    for c in data {
        bin.extend_from_slice(&c.to_le_bytes());
    }
    let len = bin.len() - off;
    pad4(bin);
    (off, len)
}

fn push_u32(bin: &mut Vec<u8>, data: &[u32]) -> (usize, usize) {
    let off = bin.len();
    for i in data {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    let len = bin.len() - off;
    pad4(bin);
    (off, len)
}

fn bbox(positions: &[[f32; 3]]) -> (serde_json::Value, serde_json::Value) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in positions {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    (json!(min), json!(max))
}
