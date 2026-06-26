//! `modelgen` — CLI for the CUDA-free photogrammetry reconstruction tool.
//!
//! A thin wrapper over [`modelgen_core::pipeline`]: each subcommand parses
//! arguments, builds a core config, and delegates. The work lives in the library
//! so a future web backend can reuse it.

mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use modelgen_core::Quality;
use modelgen_core::pipeline::{self, ReconstructConfig};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "modelgen",
    version,
    about = "Reconstruct a textured 3D mesh from photos (local, CPU, CUDA-free)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Detail-vs-speed preset (CLI mirror of [`modelgen_core::Quality`], so the core
/// stays free of the `clap` dependency). Keep the doc text in step with `Quality`.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum QualityArg {
    /// Fast and coarse — quick previews and capture iteration.
    Draft,
    /// Default: 1600px inputs, quarter-res dense, no refinement.
    Balanced,
    /// 2400px inputs, half-res dense, plus the RefineMesh pass. Much slower.
    High,
}

impl From<QualityArg> for Quality {
    fn from(q: QualityArg) -> Self {
        match q {
            QualityArg::Draft => Quality::Draft,
            QualityArg::Balanced => Quality::Balanced,
            QualityArg::High => Quality::High,
        }
    }
}

/// Resolve the input downscale cap: an explicit `--max-edge` overrides the preset
/// default. Shared by the `reconstruct` and `batch` subcommands so the precedence
/// rule lives in one place.
fn resolve_max_edge(quality: Quality, max_edge: Option<u32>) -> u32 {
    max_edge.unwrap_or(quality.max_edge())
}

/// rembg background-segmentation model for `--mask` (open-licensed only).
#[derive(Clone, Copy, Debug, ValueEnum)]
enum MaskModelArg {
    /// General-purpose (default).
    U2net,
    /// Tuned for people — cleaner silhouettes around limbs.
    U2netHumanSeg,
}

impl MaskModelArg {
    fn rembg_name(self) -> &'static str {
        match self {
            MaskModelArg::U2net => "u2net",
            MaskModelArg::U2netHumanSeg => "u2net_human_seg",
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Check that all external tools are available and working.
    Doctor {
        /// Also run a smoke test (rembg + COLMAP on a tiny input) to catch tools
        /// that resolve but crash at work.
        #[arg(long)]
        full: bool,
    },
    /// Reconstruct a textured mesh from photos (COLMAP SfM + OpenMVS dense/mesh/texture).
    Reconstruct {
        /// Directory of input photographs.
        images: PathBuf,
        /// Working directory for intermediate + output artifacts.
        work: PathBuf,
        /// Use the input images as-is (skip downscaling).
        #[arg(long)]
        no_downscale: bool,
        /// Remove the background (rembg) before reconstruction, for object-only meshes.
        #[arg(long)]
        mask: bool,
        /// Background-removal model for --mask (people: u2net-human-seg).
        #[arg(long, value_enum, default_value_t = MaskModelArg::U2net)]
        mask_model: MaskModelArg,
        /// Detail-vs-speed preset (sets dense resolution + refinement).
        #[arg(long, value_enum, default_value_t = QualityArg::Balanced)]
        quality: QualityArg,
        /// Longest-edge (px) to downscale inputs to (overrides the --quality default).
        #[arg(long)]
        max_edge: Option<u32>,
        /// Drop soft/blurry input frames (within guards) instead of only warning.
        #[arg(long)]
        drop_blurry: bool,
        /// Delete all intermediates after a successful run, keeping only the .glb.
        #[arg(long)]
        clean: bool,
    },
    /// Reconstruct every object in a directory (one photo subfolder each).
    /// Resumable; a per-object failure is logged and does not stop the batch.
    Batch {
        /// Directory with one subfolder of photos per object.
        input_dir: PathBuf,
        /// Output directory: one reconstruction subfolder per object, + a manifest.
        output_dir: PathBuf,
        /// Remove the background (rembg) before reconstruction.
        #[arg(long)]
        mask: bool,
        /// Background-removal model for --mask (people: u2net-human-seg).
        #[arg(long, value_enum, default_value_t = MaskModelArg::U2net)]
        mask_model: MaskModelArg,
        /// Use the input images as-is (skip downscaling).
        #[arg(long)]
        no_downscale: bool,
        /// Detail-vs-speed preset (sets dense resolution + refinement).
        #[arg(long, value_enum, default_value_t = QualityArg::Balanced)]
        quality: QualityArg,
        /// Longest-edge (px) to downscale inputs to (overrides the --quality default).
        #[arg(long)]
        max_edge: Option<u32>,
        /// Drop soft/blurry input frames (within guards) instead of only warning.
        #[arg(long)]
        drop_blurry: bool,
        /// Delete each object's intermediates, keeping only its .glb.
        #[arg(long)]
        clean: bool,
        /// Re-process objects even if their output already exists.
        #[arg(long)]
        force: bool,
    },
}

fn main() -> Result<()> {
    init_tracing();

    match Cli::parse().command {
        Commands::Doctor { full } => commands::doctor(full),

        Commands::Reconstruct {
            images,
            work,
            no_downscale,
            mask,
            mask_model,
            quality,
            max_edge,
            drop_blurry,
            clean,
        } => {
            let quality: Quality = quality.into();
            let cfg = ReconstructConfig {
                downscale: !no_downscale,
                mask,
                max_edge: resolve_max_edge(quality, max_edge),
                clean,
                quality,
                drop_blurry,
                mask_model: mask_model.rembg_name().to_string(),
            };
            let mesh = pipeline::reconstruct(&images, &work, &cfg)?;
            println!("textured mesh: {}", mesh.display());
            Ok(())
        }

        Commands::Batch {
            input_dir,
            output_dir,
            mask,
            mask_model,
            no_downscale,
            quality,
            max_edge,
            drop_blurry,
            clean,
            force,
        } => {
            let quality: Quality = quality.into();
            let opts = commands::BatchOpts {
                recon: ReconstructConfig {
                    downscale: !no_downscale,
                    mask,
                    max_edge: resolve_max_edge(quality, max_edge),
                    clean,
                    quality,
                    drop_blurry,
                    mask_model: mask_model.rembg_name().to_string(),
                },
                force,
            };
            commands::batch(&input_dir, &output_dir, &opts)
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .without_time()
        .with_target(false)
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_model_maps_to_the_exact_rembg_model_name() {
        // The CLI value is kebab-case (u2net-human-seg) but rembg wants underscores;
        // a wrong name silently fails masking, so pin the mapping.
        assert_eq!(MaskModelArg::U2net.rembg_name(), "u2net");
        assert_eq!(MaskModelArg::U2netHumanSeg.rembg_name(), "u2net_human_seg");
    }

    #[test]
    fn quality_arg_maps_to_core_quality() {
        assert_eq!(Quality::from(QualityArg::Draft), Quality::Draft);
        assert_eq!(Quality::from(QualityArg::Balanced), Quality::Balanced);
        assert_eq!(Quality::from(QualityArg::High), Quality::High);
    }
}
