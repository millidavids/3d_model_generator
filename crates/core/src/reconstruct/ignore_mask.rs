//! Derive OpenMVS "ignore" masks from the black-background undistorted images.
//!
//! When `--mask` composited the subject onto black (see [`crate::preprocess`]),
//! the dense stereo step still tries to match that uniform-black background — and
//! in a dark, narrow concavity *between* parts of the subject (the gap between a
//! standing person's legs, or under the feet) it invents a thin membrane of
//! surface, webbing the gap shut. Handing `DensifyPointCloud` a per-image mask
//! that marks the black background as "ignore" stops it estimating any depth
//! there, so the gap stays open.
//!
//! The mask is derived from the undistorted image itself (background = near
//! black), so it aligns with that image exactly — no separate undistortion of a
//! mask, no convention mismatch. The foreground is eroded a few pixels: this
//! widens the ignored band slightly around the silhouette, which both drops the
//! dark JPEG-ringing edge pixels and helps adjacent limbs separate cleanly.
//!
//! Caveat: a genuinely black *object* region would also be ignored, but
//! multi-view depth fusion recovers it from the views where it is lit.

use crate::error::Result;
use image::{GrayImage, Luma};
use std::path::Path;

/// A pixel whose brightest channel is below this is treated as background. The
/// composited background is pure black; this tolerance absorbs JPEG ringing.
const BG_MAX_CHANNEL: u8 = 16;
/// Erode the foreground by this many pixels (widens the ignored background band).
const ERODE_ITERS: usize = 3;
/// OpenMVS reads `<image-stem>.mask.png` for an image from the `--mask-path` folder.
const MASK_SUFFIX: &str = ".mask.png";
/// The mask label value `DensifyPointCloud --ignore-mask-label` must ignore
/// (matches the background written by [`write_ignore_masks`]).
pub(super) const IGNORE_LABEL: &str = "0";

/// Write an OpenMVS ignore-mask (`<stem>.mask.png`, background = 0, foreground =
/// 255) for every image in `images_dir` into `mask_dir`. Returns the count written.
pub(super) fn write_ignore_masks(images_dir: &Path, mask_dir: &Path) -> Result<usize> {
    std::fs::create_dir_all(mask_dir)?;
    let mut count = 0usize;
    for entry in std::fs::read_dir(images_dir)? {
        let path = entry?.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(img) = image::open(&path) else {
            continue; // not a decodable image (e.g. a stray sidecar file)
        };
        let rgb = img.to_rgb8();
        let (w, h) = rgb.dimensions();
        let mut fg: Vec<bool> = rgb
            .pixels()
            .map(|p| p.0.iter().copied().max().unwrap_or(0) >= BG_MAX_CHANNEL)
            .collect();
        erode(&mut fg, w as usize, h as usize, ERODE_ITERS);

        let mut mask = GrayImage::new(w, h);
        for (px, &keep) in mask.pixels_mut().zip(fg.iter()) {
            *px = Luma([if keep { 255 } else { 0 }]);
        }
        mask.save(mask_dir.join(format!("{stem}{MASK_SUFFIX}")))
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        count += 1;
    }
    Ok(count)
}

/// In-place binary erosion: a foreground pixel survives an iteration only if all
/// eight of its neighbours are foreground (out-of-bounds counts as background).
fn erode(fg: &mut [bool], w: usize, h: usize, iters: usize) {
    for _ in 0..iters {
        let prev = fg.to_vec();
        let survives = |x: usize, y: usize| -> bool {
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        return false;
                    }
                    if !prev[ny as usize * w + nx as usize] {
                        return false;
                    }
                }
            }
            true
        };
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                fg[i] = prev[i] && survives(x, y);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erode_peels_one_ring_per_iteration() {
        // 5x5 solid block of foreground; one erosion pass should strip the border.
        let (w, h) = (5usize, 5usize);
        let mut fg = vec![true; w * h];
        erode(&mut fg, w, h, 1);
        // Only the inner 3x3 survives.
        let survivors: usize = fg.iter().filter(|&&b| b).count();
        assert_eq!(survivors, 9);
        assert!(fg[2 * w + 2], "centre survives");
        assert!(!fg[0], "corner eroded");
    }

    #[test]
    fn writes_a_mask_per_image_with_black_as_background() {
        let dir = tempfile::tempdir().unwrap();
        let imgs = dir.path().join("images");
        let masks = dir.path().join("masks");
        std::fs::create_dir_all(&imgs).unwrap();
        // 11x11 image: black background with a bright 9x9 block (a 1px background
        // border). After a 3px erosion the centre 3x3 of the block still survives.
        let mut im = image::RgbImage::new(11, 11);
        for y in 1..10 {
            for x in 1..10 {
                im.put_pixel(x, y, image::Rgb([200, 200, 200]));
            }
        }
        // Save lossless so the black border stays exactly black for the threshold.
        im.save(imgs.join("frame.001.png")).unwrap();

        let n = write_ignore_masks(&imgs, &masks).unwrap();
        assert_eq!(n, 1);
        // Mask is named after the full stem (dots preserved) + .mask.png.
        let mask = image::open(masks.join("frame.001.mask.png"))
            .unwrap()
            .to_luma8();
        assert_eq!(mask.get_pixel(0, 0)[0], 0, "background is ignore (0)");
        assert_eq!(mask.get_pixel(5, 5)[0], 255, "object centre is kept (255)");
    }
}
