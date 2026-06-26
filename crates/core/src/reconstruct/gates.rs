//! Inter-stage gate checks: fail fast (and clearly) on degenerate output, so a
//! bad object doesn't silently waste downstream reconstruction time.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

/// Pick the COLMAP sparse sub-model (`sparse/0`, `sparse/1`, ...) with the most
/// registered images, using `images.bin` size as a proxy. Errors if none exist
/// (COLMAP registered nothing — too few images or poor overlap).
pub fn pick_largest_submodel(sparse_dir: &Path) -> Result<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in std::fs::read_dir(sparse_dir)? {
        let dir = entry?.path();
        if let Ok(meta) = std::fs::metadata(dir.join("images.bin"))
            && best.as_ref().is_none_or(|(b, _)| meta.len() > *b)
        {
            best = Some((meta.len(), dir));
        }
    }
    best.map(|(_, dir)| dir).ok_or_else(|| Error::GateFailed {
        stage: "sfm".to_string(),
        reason: "COLMAP produced no sparse reconstruction (too few images or poor overlap?)"
            .to_string(),
    })
}

/// Number of registered images in a COLMAP sparse model. COLMAP writes the
/// registered-image count as the leading little-endian `u64` of `images.bin`, so
/// we read just those 8 bytes (the file itself can be large).
pub fn registered_image_count(model_dir: &Path) -> Result<u64> {
    use std::io::Read;
    let mut buf = [0u8; 8];
    std::fs::File::open(model_dir.join("images.bin"))?.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

/// Error unless `path` exists and is non-empty.
pub fn require_nonempty(stage: &str, path: &Path) -> Result<()> {
    if std::fs::metadata(path)
        .map(|m| m.len() > 0)
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(Error::GateFailed {
            stage: stage.to_string(),
            reason: format!("expected non-empty output {}", path.display()),
        })
    }
}

/// Error unless `path` is a PLY whose header declares at least `min_faces` faces.
/// Stronger than [`require_nonempty`]: a collapsed/degenerate mesh is still
/// non-zero bytes (header + a few elements), so size alone can't catch it.
pub fn require_ply_faces(stage: &str, path: &Path, min_faces: usize) -> Result<()> {
    let faces = ply_face_count(path).unwrap_or(0);
    if faces >= min_faces {
        Ok(())
    } else {
        Err(Error::GateFailed {
            stage: stage.to_string(),
            reason: format!(
                "{} declares only {faces} faces (< {min_faces}) — degenerate mesh",
                path.display()
            ),
        })
    }
}

/// Parse the `element face <n>` count from a PLY header. The header is ASCII even
/// for binary PLY and is tiny, so we only read the first chunk of the file.
fn ply_face_count(path: &Path) -> Option<usize> {
    use std::io::Read;
    let mut buf = [0u8; 8192];
    let n = std::fs::File::open(path).ok()?.read(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf[..n]);
    for line in text.lines() {
        if line.starts_with("end_header") {
            break;
        }
        if let Some(rest) = line.strip_prefix("element face ") {
            return rest.trim().parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{registered_image_count, require_ply_faces};

    #[test]
    fn reads_registered_count_from_images_bin_header() {
        let dir = tempfile::tempdir().unwrap();
        // COLMAP's images.bin leads with a little-endian u64 count, then per-image data.
        let mut bytes = 34u64.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[0xAB; 64]); // trailing payload is ignored
        std::fs::write(dir.path().join("images.bin"), &bytes).unwrap();
        assert_eq!(registered_image_count(dir.path()).unwrap(), 34);
    }

    fn write_ply(dir: &std::path::Path, faces: usize) -> std::path::PathBuf {
        let p = dir.join("m.ply");
        let header = format!(
            "ply\nformat binary_little_endian 1.0\nelement vertex 3\nproperty float x\nelement face {faces}\nproperty list uchar int vertex_indices\nend_header\n"
        );
        std::fs::write(&p, header.as_bytes()).unwrap();
        p
    }

    #[test]
    fn passes_when_faces_meet_minimum_and_fails_when_collapsed() {
        let dir = tempfile::tempdir().unwrap();
        assert!(require_ply_faces("refine", &write_ply(dir.path(), 5000), 100).is_ok());
        // A non-empty but collapsed mesh (header present, few faces) is rejected.
        assert!(require_ply_faces("refine", &write_ply(dir.path(), 4), 100).is_err());
    }
}
