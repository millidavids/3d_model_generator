//! Background removal: produce object-only images (the object composited onto
//! black) using rembg, so COLMAP and OpenMVS focus on the object rather than the
//! surrounding scene.
//!
//! We mask the *images* instead of plumbing per-tool mask files: COLMAP and
//! OpenMVS use different mask-file conventions, and OpenMVS works on COLMAP's
//! *undistorted* images (so original-image masks wouldn't align). A solid black
//! background carries no SIFT features and no stereo texture, so both tools
//! ignore it naturally. Uses an open-licensed rembg model (default `u2net`; we
//! deliberately avoid the non-commercial `bria-rmbg` model). `model` selects the
//! segmentation network — `u2net_human_seg` gives cleaner silhouettes for people.
//!
//! (We pass `--only-mask` and composite onto black ourselves; rembg's alpha
//! matting only refines the *cutout's* alpha channel, so it has no effect on the
//! `--only-mask` output — hence no alpha-matting option here.)

use crate::error::Result;
use crate::external;
use std::path::Path;

/// Remove the background from every image in `src_dir` (compositing the object
/// onto black) and write the results into `dst_dir`. `model` is the rembg model
/// name. Foreground masks are generated in `mask_dir` (scratch). Returns the
/// number of images written.
pub fn mask_images(src_dir: &Path, dst_dir: &Path, mask_dir: &Path, model: &str) -> Result<usize> {
    std::fs::create_dir_all(dst_dir)?;
    std::fs::create_dir_all(mask_dir)?;

    // rembg -> one grayscale foreground mask per image, named by stem
    // (e.g. kermit000.jpg -> <mask_dir>/kermit000.png).
    external::run(
        "rembg",
        &[
            "p",
            "--only-mask",
            "-m",
            model,
            external::path_str(src_dir)?,
            external::path_str(mask_dir)?,
        ],
    )?;

    let mut count = 0usize;
    for entry in std::fs::read_dir(src_dir)? {
        let path = entry?.path();
        let (Some(stem), Some(name)) =
            (path.file_stem().and_then(|s| s.to_str()), path.file_name())
        else {
            continue;
        };
        let mask_path = mask_dir.join(format!("{stem}.png"));
        if !mask_path.exists() {
            continue; // not an image rembg processed
        }
        let Ok(img) = image::open(&path) else {
            continue;
        };
        let mask = image::open(&mask_path)
            .map_err(|e| std::io::Error::other(e.to_string()))?
            .to_luma8();
        composite_on_black(img.to_rgb8(), &mask)
            .save(dst_dir.join(name))
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        count += 1;
    }
    Ok(count)
}

/// Keep object pixels (mask >= 128); set background pixels to black.
fn composite_on_black(mut img: image::RgbImage, mask: &image::GrayImage) -> image::RgbImage {
    let (mw, mh) = (mask.width(), mask.height());
    for (x, y, px) in img.enumerate_pixels_mut() {
        // rembg masks match the input size, but clamp defensively.
        let value = mask.get_pixel(x.min(mw - 1), y.min(mh - 1))[0];
        if value < 128 {
            *px = image::Rgb([0, 0, 0]);
        }
    }
    img
}
