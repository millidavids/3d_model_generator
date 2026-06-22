//! Input preprocessing: prepare photos for reconstruction — downscaling
//! ([`downscale_images`], which speeds up CPU reconstruction and suits the
//! lo-fi target) and optional background masking ([`mask_images`], object-only
//! via rembg). TODO(phase 1): EXIF-orientation normalization and HEIC->PNG
//! decode (the `image` crate can't read HEIC).

mod downscale;
mod segment;

pub use downscale::downscale_images;
pub use segment::mask_images;
