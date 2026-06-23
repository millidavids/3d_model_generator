//! Make OpenMVS's glb self-contained by embedding its texture.
//!
//! OpenMVS exports `scene_textured.glb` (geometry only) plus a *separate*
//! `scene_textured_0.png`, with the glb's image referenced by an external `uri`.
//! That means the `.glb` alone is incomplete — move or delete the PNG and it
//! renders untextured (magenta). We rewrite the glb so the PNG lives inside its
//! binary buffer (`uri` → `bufferView`), making the single `.glb` portable and
//! safe for `--clean`.

use crate::error::{Error, Result};
use std::path::Path;

/// Embed any externally-referenced (`uri`) images of a binary glTF into its
/// buffer, in place. A glb with no external images is left unchanged.
pub fn embed_textures(glb_path: &Path) -> Result<()> {
    let bytes = std::fs::read(glb_path)?;
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
        return Err(err("not a binary glTF"));
    }
    let json_len = read_u32(&bytes, 12) as usize;
    let bin_len = read_u32(&bytes, 20 + json_len) as usize;
    let bin_start = 28 + json_len;
    if bin_start + bin_len > bytes.len() {
        return Err(err("truncated glb"));
    }

    let mut json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len])
        .map_err(|e| err(format!("glb JSON: {e}")))?;
    let mut bin = bytes[bin_start..bin_start + bin_len].to_vec();
    let dir = glb_path.parent().unwrap_or(Path::new("."));

    let n_images = json["images"].as_array().map_or(0, |a| a.len());
    let mut buffer_views = json["bufferViews"].as_array().cloned().unwrap_or_default();
    let mut embedded = 0usize;

    for i in 0..n_images {
        let Some(uri) = json["images"][i]
            .get("uri")
            .and_then(|u| u.as_str())
            .map(str::to_owned)
        else {
            continue; // already embedded
        };
        let png = std::fs::read(dir.join(&uri))
            .map_err(|e| err(format!("texture '{uri}' unreadable: {e}")))?;

        align4(&mut bin); // bufferView offsets must be 4-byte aligned
        let offset = bin.len();
        bin.extend_from_slice(&png);
        buffer_views.push(serde_json::json!({
            "buffer": 0, "byteOffset": offset, "byteLength": png.len(),
        }));
        json["images"][i] = serde_json::json!({
            "mimeType": mime_for(&uri), "bufferView": buffer_views.len() - 1,
        });
        embedded += 1;
    }

    if embedded == 0 {
        return Ok(());
    }

    align4(&mut bin);
    json["bufferViews"] = serde_json::Value::Array(buffer_views);
    json["buffers"] = serde_json::json!([{ "byteLength": bin.len() }]);
    std::fs::write(glb_path, assemble_glb(&json, &bin)?)?;
    Ok(())
}

/// Reassemble a binary glTF from its JSON + BIN chunk.
fn assemble_glb(json: &serde_json::Value, bin: &[u8]) -> Result<Vec<u8>> {
    let mut json_bytes = serde_json::to_vec(json).map_err(|e| err(format!("serialize: {e}")))?;
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }
    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json_bytes);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(bin);
    Ok(out)
}

fn align4(buf: &mut Vec<u8>) {
    while !buf.len().is_multiple_of(4) {
        buf.push(0);
    }
}

fn mime_for(uri: &str) -> &'static str {
    if uri.to_ascii_lowercase().ends_with(".jpg") || uri.to_ascii_lowercase().ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "image/png"
    }
}

fn read_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn err(reason: impl Into<String>) -> Error {
    Error::GateFailed {
        stage: "glb-embed".to_string(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_an_external_texture_and_drops_the_uri() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("tex.png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]))
            .save(&png)
            .unwrap();
        let png_len = std::fs::metadata(&png).unwrap().len() as usize;

        // A minimal glb that references the PNG by external uri.
        let json = serde_json::json!({
            "asset": {"version": "2.0"},
            "buffers": [{"byteLength": 4}],
            "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 4}],
            "images": [{"uri": "tex.png"}],
        });
        let glb = dir.path().join("m.glb");
        std::fs::write(&glb, assemble_glb(&json, &[1, 2, 3, 4]).unwrap()).unwrap();

        embed_textures(&glb).unwrap();

        let bytes = std::fs::read(&glb).unwrap();
        let jlen = read_u32(&bytes, 12) as usize;
        let g: serde_json::Value = serde_json::from_slice(&bytes[20..20 + jlen]).unwrap();
        assert!(g["images"][0].get("uri").is_none(), "uri should be gone");
        let bv = g["images"][0]["bufferView"].as_u64().unwrap() as usize;
        assert_eq!(
            g["bufferViews"][bv]["byteLength"].as_u64().unwrap() as usize,
            png_len
        );

        // The sidecar is now irrelevant: deleting it, a re-embed is a clean no-op.
        std::fs::remove_file(&png).unwrap();
        embed_textures(&glb).unwrap();
    }
}
