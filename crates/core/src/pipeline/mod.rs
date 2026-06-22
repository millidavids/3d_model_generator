//! Pipeline orchestration — the photos→asset flow, reusable by the `modelgen`
//! CLI and a future web backend.
//!
//! preprocess → reconstruct → import → heal → decimate → [rebake] → normalize →
//! pixelate → export. Split into the front half ([`reconstruct`]), the back half
//! ([`lofi`]), and the end-to-end [`process`].

mod lofi;
mod process;
mod reconstruct;

pub use lofi::{LofiConfig, lofi};
pub use process::{Pipeline, process};
pub use reconstruct::{ReconstructConfig, reconstruct};
