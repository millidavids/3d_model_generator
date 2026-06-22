//! `modelgen` — command-line interface for the lo-fi 3D asset generator.
//!
//! A thin wrapper over [`modelgen_core::pipeline`]: each subcommand parses
//! arguments, builds a core config, and delegates. The actual work lives in the
//! library so a future web backend can reuse it.

mod commands;

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use modelgen_core::pipeline::{self, LofiConfig, ReconstructConfig};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "modelgen",
    version,
    about = "Generate lo-fi 3D game assets from photos"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check that all external tools are available and working.
    Doctor,
    /// End-to-end: photos → lo-fi glTF asset (reconstruct + lofi in one step).
    Process {
        /// Directory of input photographs.
        photos: PathBuf,
        /// Output `.glb` path.
        out: PathBuf,
        /// Working directory (default: "<out-stem>.work" beside the output).
        #[arg(long)]
        work: Option<PathBuf>,
        /// Remove the background (rembg) before reconstruction.
        #[arg(long)]
        mask: bool,
        /// Longest-edge (px) to downscale inputs to.
        #[arg(long, default_value_t = 1600)]
        max_edge: u32,
        /// Target triangle budget.
        #[arg(long, default_value_t = 1500)]
        target_tris: u32,
        /// Pixelated texture size (longest edge, px).
        #[arg(long, default_value_t = 128)]
        texture_size: u32,
        /// Palette colour count for the texture.
        #[arg(long, default_value_t = 256)]
        palette_colors: u16,
    },
    /// Reconstruct a textured mesh from photos (COLMAP + OpenMVS).
    Reconstruct {
        /// Directory of input photographs.
        images: PathBuf,
        /// Working directory for intermediate + output artifacts.
        work: PathBuf,
        /// Use the input images as-is (skip downscaling).
        #[arg(long)]
        no_downscale: bool,
        /// Remove the background (rembg) before reconstruction.
        #[arg(long)]
        mask: bool,
        /// Longest-edge (px) to downscale inputs to before reconstruction.
        #[arg(long, default_value_t = 1600)]
        max_edge: u32,
    },
    /// Convert a reconstructed textured mesh to a lo-fi glTF asset.
    Lofi {
        /// Path to the OpenMVS textured mesh (`.ply`).
        mesh: PathBuf,
        /// Output `.glb` path.
        out: PathBuf,
        /// Target triangle budget for decimation.
        #[arg(long, default_value_t = 1500)]
        target_tris: u32,
        /// Skip decimation (keep full resolution).
        #[arg(long)]
        no_decimate: bool,
        /// Pixelated texture size (longest edge, px).
        #[arg(long, default_value_t = 128)]
        texture_size: u32,
        /// Palette colour count for the texture.
        #[arg(long, default_value_t = 256)]
        palette_colors: u16,
        /// Skip texture pixelation (keep the full-res texture).
        #[arg(long)]
        no_pixelate: bool,
        /// Keep disconnected fragments (skip largest-component cleanup).
        #[arg(long)]
        keep_floaters: bool,
        /// Skip normalization (centering + unit scaling).
        #[arg(long)]
        no_normalize: bool,
        /// Re-UV + rebake the texture via native Blender (host-only): cleaner
        /// texel density than the kept atlas.
        #[arg(long)]
        rebake: bool,
        /// Blender binary path (else $BLENDER, then PATH, then the macOS app).
        #[arg(long)]
        blender: Option<PathBuf>,
    },
    /// Batch end-to-end over a directory of objects (one photo subfolder each).
    /// Resumable; a per-object failure is logged and does not stop the batch.
    Batch {
        /// Directory with one subfolder of photos per object.
        input_dir: PathBuf,
        /// Output directory for the `.glb` assets + manifest.
        output_dir: PathBuf,
        /// Remove the background (rembg) before reconstruction.
        #[arg(long)]
        mask: bool,
        /// Longest-edge (px) to downscale inputs to.
        #[arg(long, default_value_t = 1600)]
        max_edge: u32,
        /// Target triangle budget.
        #[arg(long, default_value_t = 1500)]
        target_tris: u32,
        /// Pixelated texture size (longest edge, px).
        #[arg(long, default_value_t = 128)]
        texture_size: u32,
        /// Palette colour count for the texture.
        #[arg(long, default_value_t = 256)]
        palette_colors: u16,
        /// Re-process objects even if their output already exists.
        #[arg(long)]
        force: bool,
    },
}

fn main() -> Result<()> {
    init_tracing();

    match Cli::parse().command {
        Commands::Doctor => commands::doctor(),

        Commands::Process {
            photos,
            out,
            work,
            mask,
            max_edge,
            target_tris,
            texture_size,
            palette_colors,
        } => {
            let work = work.unwrap_or_else(|| default_work_dir(&out));
            let recon = ReconstructConfig {
                downscale: true,
                mask,
                max_edge,
            };
            let lofi = LofiConfig {
                target_triangles: target_tris,
                texture_size,
                palette_colors,
                ..LofiConfig::default()
            };
            Ok(pipeline::process(&photos, &work, &out, &recon, &lofi)?)
        }

        Commands::Reconstruct {
            images,
            work,
            no_downscale,
            mask,
            max_edge,
        } => {
            let cfg = ReconstructConfig {
                downscale: !no_downscale,
                mask,
                max_edge,
            };
            let mesh = pipeline::reconstruct(&images, &work, &cfg)?;
            println!("textured mesh: {}", mesh.display());
            Ok(())
        }

        Commands::Lofi {
            mesh,
            out,
            target_tris,
            no_decimate,
            texture_size,
            palette_colors,
            no_pixelate,
            keep_floaters,
            no_normalize,
            rebake,
            blender,
        } => {
            let cfg = LofiConfig {
                target_triangles: target_tris,
                decimate: !no_decimate,
                texture_size,
                palette_colors,
                pixelate: !no_pixelate,
                cleanup: !keep_floaters,
                normalize: !no_normalize,
                rebake: resolve_rebake(rebake, blender)?,
            };
            Ok(pipeline::lofi(&mesh, &out, &cfg)?)
        }

        Commands::Batch {
            input_dir,
            output_dir,
            mask,
            max_edge,
            target_tris,
            texture_size,
            palette_colors,
            force,
        } => {
            let opts = commands::BatchOpts {
                recon: ReconstructConfig {
                    downscale: true,
                    mask,
                    max_edge,
                },
                lofi: LofiConfig {
                    target_triangles: target_tris,
                    texture_size,
                    palette_colors,
                    ..LofiConfig::default()
                },
                force,
            };
            commands::batch(&input_dir, &output_dir, &opts)
        }
    }
}

/// Resolve the `--rebake` flag to a Blender path (else a clear error).
fn resolve_rebake(rebake: bool, blender: Option<PathBuf>) -> Result<Option<PathBuf>> {
    if !rebake {
        return Ok(None);
    }
    let path = blender
        .or_else(modelgen_core::rebake::find_blender)
        .ok_or_else(|| {
            anyhow!("Blender not found — install it, pass --blender <path>, or set $BLENDER")
        })?;
    Ok(Some(path))
}

/// Default scratch dir for `process`: "<out-stem>.work" beside the output.
fn default_work_dir(out: &Path) -> PathBuf {
    let stem = out
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "modelgen".to_string());
    out.with_file_name(format!("{stem}.work"))
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
