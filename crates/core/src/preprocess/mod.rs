//! Input preprocessing: prepare photos for reconstruction.
//!
//! For now this is image downscaling, which both speeds up CPU reconstruction
//! and suits the lo-fi target. TODO(phase 1): EXIF-orientation normalization,
//! HEIC->PNG decode (the `image` crate can't read HEIC), and rembg background
//! masking.

mod downscale;

pub use downscale::downscale_images;
