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
