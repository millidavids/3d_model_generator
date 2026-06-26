//! `doctor --full` smoke test: run the crash-prone external tools on a tiny
//! synthetic input to confirm they actually *execute*, not just resolve on PATH.
//!
//! `--version` (the cheap `doctor` check) catches a binary that won't launch, but
//! not one that launches and then crashes doing real work — e.g. onnxruntime
//! importing fine yet aborting with an illegal instruction on inference (a real
//! arm64 case), or COLMAP's SIFT hitting a bad OpenCV/feature lib. We exercise
//! rembg (runs the model) and COLMAP feature extraction (runs SIFT). We test
//! *execution*, not registration, so no real overlapping-photo fixture is needed —
//! high-frequency synthetic noise gives SIFT plenty of features. OpenMVS needs a
//! real scene to run, so it stays at the presence/`--version` check.

use crate::error::{Error, Result};
use crate::external;

/// Edge length of the synthetic smoke images.
const SMOKE_SIZE: u32 = 256;

/// Run rembg + COLMAP on a tiny synthetic input. Returns an error (naming the
/// failed tool) if either does not produce its expected output.
pub fn smoke_test() -> Result<()> {
    let dir = std::env::temp_dir().join("modelgen-smoke");
    let _ = std::fs::remove_dir_all(&dir);
    let imgs = dir.join("imgs");
    let masks = dir.join("masks");
    let db = dir.join("colmap.db");
    std::fs::create_dir_all(&imgs)?;

    // Two high-frequency synthetic images (deterministic — no rng dep). Pure noise
    // is rich in corners, so SIFT extracts plenty of features.
    write_noise(&imgs.join("smoke0.png"), 0x9E37)?;
    write_noise(&imgs.join("smoke1.png"), 0x85EB)?;

    // rembg: actually run the segmentation model (catches an onnxruntime that
    // imports yet crashes on inference). Mask quality is irrelevant — just output.
    external::run(
        "rembg",
        &[
            "p",
            "--only-mask",
            external::path_str(&imgs)?,
            external::path_str(&masks)?,
        ],
    )?;
    let n_masks = std::fs::read_dir(&masks).map(|rd| rd.count()).unwrap_or(0);
    if n_masks < 2 {
        return Err(smoke_err(
            "rembg produced no mask (segmentation model failed?)",
        ));
    }

    // COLMAP: actually extract SIFT features (catches an OpenCV/feature lib crash).
    external::run(
        "colmap",
        &[
            "feature_extractor",
            "--database_path",
            external::path_str(&db)?,
            "--image_path",
            external::path_str(&imgs)?,
            "--FeatureExtraction.use_gpu",
            "0",
        ],
    )?;
    if std::fs::metadata(&db).map(|m| m.len()).unwrap_or(0) == 0 {
        return Err(smoke_err("COLMAP wrote no feature database (SIFT failed?)"));
    }

    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Write a deterministic high-frequency noise PNG (an integer pixel hash).
fn write_noise(path: &std::path::Path, seed: u32) -> Result<()> {
    let img = image::RgbImage::from_fn(SMOKE_SIZE, SMOKE_SIZE, |x, y| {
        let h = x.wrapping_mul(73856093) ^ y.wrapping_mul(19349663) ^ seed.wrapping_mul(83492791);
        image::Rgb([h as u8, (h >> 8) as u8, (h >> 16) as u8])
    });
    img.save(path)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(())
}

fn smoke_err(reason: &str) -> Error {
    Error::GateFailed {
        stage: "smoke".to_string(),
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_image_is_feature_rich_not_flat() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("n.png");
        write_noise(&p, 0x1234).unwrap();
        let img = image::open(&p).unwrap().to_luma8();
        // A flat image would have one unique value; noise has many.
        let distinct: std::collections::HashSet<u8> = img.pixels().map(|p| p[0]).collect();
        assert!(
            distinct.len() > 50,
            "synthetic smoke image must carry texture"
        );
    }
}
