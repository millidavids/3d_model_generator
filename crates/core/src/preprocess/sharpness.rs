//! Input sharpness QC: blurry photos poison feature matching, so we score each
//! image by **variance-of-Laplacian** (higher = sharper) and flag the soft ones.
//!
//! The threshold is **relative** — a fraction of the median score — so it is
//! content-independent (a uniformly soft macro set isn't all flagged; a uniformly
//! sharp set yields no false positives). Default behaviour is to *warn*; the
//! pipeline can also *drop* soft frames, but only within guards (never leave too
//! few images, never drop too large a fraction) so QC can't itself open a
//! coverage gap and lower registration.

use crate::error::Result;
use image::GrayImage;
use std::path::{Path, PathBuf};

/// A frame is "soft" if its sharpness is below this fraction of the median.
pub(crate) const SOFT_FRACTION: f64 = 0.4;
/// When dropping, never leave fewer than this many images.
const MIN_KEPT: usize = 20;
/// ...nor keep fewer than this fraction of the input...
const MIN_KEPT_FRACTION: f64 = 0.8;
/// ...nor drop more than this fraction of the input in one run.
const MAX_DROP_FRACTION: f64 = 0.15;

/// One image's sharpness score.
pub(crate) struct Sharpness {
    pub path: PathBuf,
    pub score: f64,
}

/// Variance of the Laplacian of `img` — higher means sharper.
///
/// Hand-rolled 4-neighbour Laplacian accumulated in `f64`. We deliberately do NOT
/// use `image::imageops::filter3x3`: it clamps each response to the pixel type
/// (`0..=255`), which throws away the signed Laplacian's spread — the exact signal
/// this metric measures.
pub(crate) fn variance_of_laplacian(img: &GrayImage) -> f64 {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return 0.0;
    }
    let at = |x: u32, y: u32| img.get_pixel(x, y)[0] as f64;
    let (mut sum, mut sum_sq, mut n) = (0.0f64, 0.0f64, 0.0f64);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let lap = at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1) - 4.0 * at(x, y);
            sum += lap;
            sum_sq += lap * lap;
            n += 1.0;
        }
    }
    let mean = sum / n;
    (sum_sq / n - mean * mean).max(0.0)
}

/// Score every decodable image in `dir`, sorted sharpest-first.
pub(crate) fn score_dir(dir: &Path) -> Result<Vec<Sharpness>> {
    let mut scores = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let Ok(img) = image::open(&path) else {
            continue; // not a decodable image
        };
        scores.push(Sharpness {
            path,
            score: variance_of_laplacian(&img.to_luma8()),
        });
    }
    scores.sort_by(|a, b| b.score.total_cmp(&a.score));
    Ok(scores)
}

/// Median score (0.0 if empty).
fn median(scores: &[Sharpness]) -> f64 {
    if scores.is_empty() {
        return 0.0;
    }
    let mut s: Vec<f64> = scores.iter().map(|x| x.score).collect();
    s.sort_by(f64::total_cmp);
    let mid = s.len() / 2;
    if s.len().is_multiple_of(2) {
        (s[mid - 1] + s[mid]) / 2.0
    } else {
        s[mid]
    }
}

/// Soft frames: those below `SOFT_FRACTION × median`. Empty when the set is
/// uniform (everything near the median) — by design.
pub(crate) fn soft_frames(scores: &[Sharpness]) -> Vec<&Sharpness> {
    let cutoff = SOFT_FRACTION * median(scores);
    scores.iter().filter(|s| s.score < cutoff).collect()
}

/// The soft frames that may actually be dropped, honoring the guards (drop the
/// worst-scoring first). Returns paths to exclude; empty if guards forbid any drop.
pub(crate) fn droppable(scores: &[Sharpness]) -> Vec<&Path> {
    let total = scores.len();
    let min_kept = MIN_KEPT.max((MIN_KEPT_FRACTION * total as f64).ceil() as usize);
    let max_drop = (MAX_DROP_FRACTION * total as f64).floor() as usize;
    let budget = max_drop.min(total.saturating_sub(min_kept));
    if budget == 0 {
        return Vec::new();
    }
    // `scores` is sharpest-first, so the soft ones are at the tail; take the worst.
    let mut soft: Vec<&Sharpness> = soft_frames(scores);
    soft.sort_by(|a, b| a.score.total_cmp(&b.score)); // worst first
    soft.into_iter()
        .take(budget)
        .map(|s| s.path.as_path())
        .collect()
}

/// Score the images in `dir`, **warn** about soft frames, and — when
/// `drop_blurry` — produce a filtered directory under `work` containing only the
/// kept frames (guards permitting). Returns the directory to feed forward: the
/// original `dir` if nothing is dropped, else the filtered copy.
pub(crate) fn run(dir: &Path, drop_blurry: bool, work: &Path) -> Result<PathBuf> {
    let scores = score_dir(dir)?;
    if scores.is_empty() {
        return Ok(dir.to_path_buf());
    }
    let soft = soft_frames(&scores);
    if !soft.is_empty() {
        tracing::warn!(
            soft = soft.len(),
            total = scores.len(),
            "soft/blurry frames detected (below {SOFT_FRACTION}x median sharpness)"
        );
        for s in &soft {
            tracing::warn!(image = %s.path.display(), score = s.score, "soft frame");
        }
    }
    if !drop_blurry {
        return Ok(dir.to_path_buf());
    }
    let drop = droppable(&scores);
    if drop.is_empty() {
        if !soft.is_empty() {
            tracing::info!("--drop-blurry: guards prevented dropping (too few images)");
        }
        return Ok(dir.to_path_buf());
    }
    let filtered = work.join("qc_filtered");
    std::fs::create_dir_all(&filtered)?;
    let mut kept = 0usize;
    for s in &scores {
        if drop.contains(&s.path.as_path()) {
            continue;
        }
        if let Some(name) = s.path.file_name() {
            link_or_copy(&s.path, &filtered.join(name))?;
            kept += 1;
        }
    }
    tracing::info!(
        dropped = drop.len(),
        kept,
        "--drop-blurry: filtered soft frames"
    );
    Ok(filtered)
}

/// Hard-link `src` into `dst` (cheap, no data copy), falling back to a byte copy
/// across filesystems. `dst` is replaced if it already exists.
fn link_or_copy(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        std::fs::remove_file(dst)?;
    }
    if std::fs::hard_link(src, dst).is_err() {
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, Luma};

    fn checkerboard() -> GrayImage {
        GrayImage::from_fn(32, 32, |x, y| {
            Luma([if (x / 2 + y / 2) % 2 == 0 { 255 } else { 0 }])
        })
    }

    #[test]
    fn blur_lowers_the_sharpness_score() {
        let sharp = checkerboard();
        let blurred = image::imageops::blur(&sharp, 2.0); // a real Gaussian blur
        assert!(
            variance_of_laplacian(&sharp) > variance_of_laplacian(&blurred),
            "a blurred copy must score lower than its sharp original"
        );
    }

    #[test]
    fn flat_image_scores_zero() {
        let flat = GrayImage::from_pixel(16, 16, Luma([128]));
        assert_eq!(variance_of_laplacian(&flat), 0.0);
    }

    fn fake(score: f64) -> Sharpness {
        Sharpness {
            path: PathBuf::from(format!("{score}.png")),
            score,
        }
    }

    #[test]
    fn uniform_sets_flag_nothing() {
        // All near a common value ⇒ none below 0.4×median (no false positives, and
        // a uniformly-soft set isn't wholesale flagged).
        let scores: Vec<_> = [100.0, 102.0, 98.0, 101.0, 99.0]
            .map(fake)
            .into_iter()
            .collect();
        assert!(soft_frames(&scores).is_empty());
    }

    #[test]
    fn a_clear_outlier_is_flagged() {
        let scores: Vec<_> = [100.0, 102.0, 98.0, 5.0, 99.0]
            .map(fake)
            .into_iter()
            .collect();
        let soft = soft_frames(&scores);
        assert_eq!(soft.len(), 1);
        assert_eq!(soft[0].score, 5.0);
    }

    #[test]
    fn drop_guards_protect_small_and_cap_large() {
        // 10 images, 4 soft: max-drop is 15% (=1) and min-kept is max(20, 80%)=20 >
        // total, so NOTHING may be dropped on a small set.
        let mut scores: Vec<_> = (0..6).map(|i| fake(100.0 + i as f64)).collect();
        scores.extend([1.0, 2.0, 3.0, 4.0].map(fake));
        assert!(
            droppable(&scores).is_empty(),
            "small set: guards forbid dropping"
        );

        // 100 images, many soft: capped at 15% = 15 dropped, worst-first.
        let mut big: Vec<_> = (0..60).map(|_| fake(100.0)).collect();
        big.extend((0..40).map(|i| fake(i as f64))); // 40 soft (0..40)
        let dropped = droppable(&big);
        assert_eq!(dropped.len(), 15, "capped at 15% of 100");
    }
}
