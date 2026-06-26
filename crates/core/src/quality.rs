//! Reconstruction quality presets.
//!
//! One dial that trades reconstruction time for fidelity by setting three things
//! together: how much the input photos are downscaled, the working resolution of
//! the dense stereo step, and whether the (slow, CPU-only) photoconsistency
//! mesh-refinement pass runs. `Balanced` reproduces the pipeline's historical
//! hard-coded settings, so it is the default and changes nothing for existing runs.
//!
//! The dominant lever is the dense step's working resolution. OpenMVS's
//! `--resolution-level` halves the image *that many times* before densifying, so
//! a lower level keeps far more pixels (and detail); we pair it with a matching
//! input-downscale cap so the extra dense resolution has real image data behind it.

/// How hard the pipeline works for detail. Higher is slower but sharper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// Fast and coarse — quick previews and capture iteration.
    Draft,
    /// The historical default: 1600px inputs, quarter-res dense, no refinement.
    #[default]
    Balanced,
    /// Full-detail inputs, half-res dense, plus a `RefineMesh` photoconsistency
    /// pass. Substantially slower (the refine pass is CPU-bound here).
    High,
}

impl Quality {
    /// Longest-edge (px) the preprocess step downscales inputs to. Public because
    /// it is also the default for the CLI's `--max-edge` when left unset.
    pub fn max_edge(self) -> u32 {
        match self {
            Quality::Draft => 1000,
            Quality::Balanced => 1600,
            Quality::High => 2400,
        }
    }

    /// `DensifyPointCloud --resolution-level`: how many times to halve the images
    /// before densifying (higher = coarser and faster).
    pub(crate) fn dense_resolution_level(self) -> u32 {
        match self {
            Quality::Draft => 3,
            Quality::Balanced => 2,
            Quality::High => 1,
        }
    }

    /// Whether to run the `RefineMesh` photoconsistency pass, which sharpens the
    /// mesh against the source images. CPU-only here, so it is the slow part.
    pub(crate) fn refine(self) -> bool {
        matches!(self, Quality::High)
    }

    /// Whether SfM uses the slower, more robust COLMAP feature settings (richer
    /// camera model, more features, affine-invariant SIFT). Worth it on
    /// low-texture / smooth / thin surfaces; only `High` pays the cost. See
    /// [`crate::reconstruct`]'s `sfm` module.
    pub(crate) fn robust_sfm(self) -> bool {
        matches!(self, Quality::High)
    }
}

// Note: the dense step's `--max-resolution` is NOT derived here. It tracks the
// *resolved* input downscale (`ReconstructConfig::max_edge`, which `--max-edge`
// can override), threaded into `reconstruct::run`, so the cap always matches the
// images actually present rather than the preset's default.

#[cfg(test)]
mod tests {
    use super::Quality;

    #[test]
    fn balanced_is_the_default_and_matches_the_historical_settings() {
        assert_eq!(Quality::default(), Quality::Balanced);
        assert_eq!(Quality::Balanced.max_edge(), 1600);
        assert_eq!(Quality::Balanced.dense_resolution_level(), 2);
        assert!(!Quality::Balanced.refine());
        // Default SfM stays on the fast path — robust features are High-only.
        assert!(!Quality::Balanced.robust_sfm());
        assert!(!Quality::Draft.robust_sfm());
    }

    #[test]
    fn higher_quality_means_more_pixels_and_refinement() {
        // Detail rises (bigger inputs, lower scale-down level) from draft -> high.
        assert!(Quality::High.max_edge() > Quality::Balanced.max_edge());
        assert!(Quality::Balanced.max_edge() > Quality::Draft.max_edge());
        assert!(
            Quality::High.dense_resolution_level() < Quality::Balanced.dense_resolution_level()
        );
        // Only High pays for the refinement pass and robust SfM.
        assert!(Quality::High.refine());
        assert!(!Quality::Draft.refine());
        assert!(Quality::High.robust_sfm());
    }
}
