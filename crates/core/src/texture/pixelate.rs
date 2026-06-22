//! Texture pixelation: downscale to a small resolution and reduce to a limited
//! colour palette — the lo-fi pixel-texture look. (The nearest sampler set by
//! the glTF exporter keeps the texels crisp at render time.)

use crate::error::{Error, Result};
use image::imageops::FilterType;
use std::path::Path;

/// Downscale the texture at `src` to fit within `size`x`size` px, quantize to
/// `colors` palette entries, and write the result as a PNG to `dst`.
pub fn pixelate(src: &Path, dst: &Path, size: u32, colors: u16) -> Result<()> {
    let small = image::open(src)
        .map_err(|e| pix_err(e.to_string()))?
        .resize(size, size, FilterType::Triangle)
        .to_rgb8();
    quantize(small, colors)?
        .save(dst)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(())
}

/// Reduce `img` to a `colors`-entry palette with quantette (no dither).
fn quantize(img: image::RgbImage, colors: u16) -> Result<image::RgbImage> {
    use quantette::{ImageBuf, Pipeline};

    let buf = ImageBuf::try_from(img).map_err(|e| pix_err(format!("{e:?}")))?;
    let palette_size = colors.try_into().map_err(|e| pix_err(format!("{e:?}")))?;
    let quantized = Pipeline::new()
        .palette_size(palette_size)
        .ditherer(None)
        .parallel(true)
        .input_image(buf.as_ref())
        .output_srgb8_image();
    Ok(quantized.into())
}

fn pix_err(reason: String) -> Error {
    Error::GateFailed {
        stage: "pixelate".into(),
        reason,
    }
}
