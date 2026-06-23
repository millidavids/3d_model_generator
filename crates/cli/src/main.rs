//! `modelgen` — CLI for the CUDA-free photogrammetry reconstruction tool.
//!
//! A thin wrapper over [`modelgen_core::pipeline`]: each subcommand parses
//! arguments, builds a core config, and delegates. The work lives in the library
//! so a future web backend can reuse it.

mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
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

#[derive(Subcommand)]
enum Commands {
    /// Check that all external tools are available and working.
    Doctor,
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
        /// Longest-edge (px) to downscale inputs to before reconstruction.
        #[arg(long, default_value_t = 1600)]
        max_edge: u32,
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
        /// Longest-edge (px) to downscale inputs to.
        #[arg(long, default_value_t = 1600)]
        max_edge: u32,
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
        Commands::Doctor => commands::doctor(),

        Commands::Reconstruct {
            images,
            work,
            no_downscale,
            mask,
            max_edge,
            clean,
        } => {
            let cfg = ReconstructConfig {
                downscale: !no_downscale,
                mask,
                max_edge,
                clean,
            };
            let mesh = pipeline::reconstruct(&images, &work, &cfg)?;
            println!("textured mesh: {}", mesh.display());
            Ok(())
        }

        Commands::Batch {
            input_dir,
            output_dir,
            mask,
            max_edge,
            clean,
            force,
        } => {
            let opts = commands::BatchOpts {
                recon: ReconstructConfig {
                    downscale: true,
                    mask,
                    max_edge,
                    clean,
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
