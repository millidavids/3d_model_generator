//! Early input validation, so bad paths or empty inputs fail with a clear,
//! actionable message instead of a confusing error deep in an external tool.

use crate::error::{Error, Result};
use std::path::Path;

/// Image extensions the reconstruction front-half accepts.
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "bmp", "tif", "tiff", "webp"];

/// Require that `path` is an existing regular file.
pub fn require_file(path: &Path, what: &str) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(invalid(format!("{what} not found: {}", path.display())))
    }
}

/// Require that `dir` exists and holds at least one image file.
pub fn require_image_dir(dir: &Path) -> Result<()> {
    if !dir.is_dir() {
        return Err(invalid(format!(
            "photo directory not found: {}",
            dir.display()
        )));
    }
    let has_image = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .any(|e| is_image(&e.path()));
    if has_image {
        Ok(())
    } else {
        Err(invalid(format!(
            "no images ({}) in {}",
            IMAGE_EXTS.join("/"),
            dir.display()
        )))
    }
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn invalid(reason: String) -> Error {
    Error::GateFailed {
        stage: "input".to_string(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::{require_file, require_image_dir};

    #[test]
    fn require_file_distinguishes_present_and_missing() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("mesh.ply");
        std::fs::write(&f, b"x").unwrap();
        assert!(require_file(&f, "mesh").is_ok());
        assert!(require_file(&dir.path().join("missing.ply"), "mesh").is_err());
    }

    #[test]
    fn require_image_dir_needs_an_image() {
        let dir = tempfile::tempdir().unwrap();
        assert!(require_image_dir(dir.path()).is_err()); // empty
        std::fs::write(dir.path().join("readme.txt"), b"x").unwrap();
        assert!(require_image_dir(dir.path()).is_err()); // no images
        std::fs::write(dir.path().join("a.JPG"), b"x").unwrap();
        assert!(require_image_dir(dir.path()).is_ok()); // case-insensitive ext
    }
}
