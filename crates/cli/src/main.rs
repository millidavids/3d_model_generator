//! `modelgen` — command-line interface for the lo-fi 3D asset generator.

use anyhow::Result;
use clap::{Parser, Subcommand};
use modelgen_core::{Pipeline, PipelineConfig, external};
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
    /// Process one object: a directory of photos → a glTF asset.
    Process {
        /// Directory containing the input photographs.
        input: PathBuf,
        /// Output `.glb` path.
        output: PathBuf,
    },
    /// Reconstruct a textured mesh from photos (COLMAP + OpenMVS). [Phase 1]
    Reconstruct {
        /// Directory of input photographs.
        images: PathBuf,
        /// Working directory for intermediate + output artifacts.
        work: PathBuf,
        /// Use the input images as-is (skip downscaling).
        #[arg(long)]
        no_downscale: bool,
        /// Remove the background (rembg) before reconstruction, for clean
        /// object-only meshes.
        #[arg(long)]
        mask: bool,
        /// Longest-edge (px) to downscale inputs to before reconstruction.
        #[arg(long, default_value_t = 1600)]
        max_edge: u32,
    },
    /// Convert a reconstructed textured mesh to a lo-fi glTF asset. [Phase 2 — WIP]
    Lofi {
        /// Path to the OpenMVS textured mesh (`.ply`).
        mesh: PathBuf,
        /// Output `.glb` path.
        out: PathBuf,
        /// Target triangle budget for decimation.
        #[arg(long, default_value_t = 1500)]
        target_tris: usize,
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
    },
}

fn main() -> Result<()> {
    init_tracing();

    match Cli::parse().command {
        Commands::Doctor => doctor(),
        Commands::Process { input, output } => {
            let pipeline = Pipeline::new(PipelineConfig::default());
            let out = pipeline.run(&input, &output)?;
            println!("wrote {}", out.display());
            Ok(())
        }
        Commands::Reconstruct {
            images,
            work,
            no_downscale,
            mask,
            max_edge,
        } => reconstruct(&images, &work, no_downscale, mask, max_edge),
        Commands::Lofi {
            mesh,
            out,
            target_tris,
            no_decimate,
            texture_size,
            palette_colors,
            no_pixelate,
        } => {
            let opts = LofiOpts {
                target_tris,
                no_decimate,
                texture_size,
                palette_colors,
                no_pixelate,
            };
            lofi(&mesh, &out, &opts)
        }
    }
}

struct LofiOpts {
    target_tris: usize,
    no_decimate: bool,
    texture_size: u32,
    palette_colors: u16,
    no_pixelate: bool,
}

/// [Phase 2] Convert a reconstructed textured mesh to a lo-fi glTF asset:
/// import → decimate → pixelate texture → glTF export (unlit + nearest).
fn lofi(mesh_path: &Path, out: &Path, opts: &LofiOpts) -> Result<()> {
    use modelgen_core::{export, mesh, texture};

    let mut m = mesh::load_textured_ply(mesh_path)?;
    println!(
        "loaded: {} triangles, {} vertices",
        m.triangle_count(),
        m.vertex_count()
    );

    if !opts.no_decimate {
        m = mesh::decimate(&m, opts.target_tris);
        println!(
            "decimated: {} triangles, {} vertices",
            m.triangle_count(),
            m.vertex_count()
        );
    }

    if !opts.no_pixelate
        && let Some(src) = m.texture.clone()
    {
        let pix = out.with_extension("tex.png");
        texture::pixelate(&src, &pix, opts.texture_size, opts.palette_colors)?;
        m.texture = Some(pix);
        println!(
            "pixelated texture: {}px, {} colors",
            opts.texture_size, opts.palette_colors
        );
    }

    export::write_glb(&m, out)?;
    println!("wrote {}", out.display());
    Ok(())
}

/// [Phase 1] Reconstruct a textured mesh from photos: preprocess (downscale,
/// optional background masking), then COLMAP SfM + OpenMVS dense/mesh/texture.
fn reconstruct(
    images: &Path,
    work: &Path,
    no_downscale: bool,
    mask: bool,
    max_edge: u32,
) -> Result<()> {
    use modelgen_core::preprocess;

    // Downscale (unless skipped) to keep CPU reconstruction tractable.
    let downscaled = if no_downscale {
        images.to_path_buf()
    } else {
        let out = work.join("images");
        let n = preprocess::downscale_images(images, &out, max_edge)?;
        println!("downscaled {n} image(s)");
        out
    };

    // Optionally remove the background so the reconstructed mesh is object-only.
    let input = if mask {
        let out = work.join("masked");
        let n = preprocess::mask_images(&downscaled, &out, &work.join("masks"))?;
        println!("masked {n} image(s) (background removed)");
        out
    } else {
        downscaled
    };

    let result = modelgen_core::reconstruct::run(&input, work)?;
    println!("textured mesh: {}", result.textured_mesh.display());
    Ok(())
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Report which external tools are present. (Phase 0 prints presence; real
/// smoke inferences are added alongside `external::check_tools`.)
fn doctor() -> Result<()> {
    println!("Checking external tools:");
    let mut missing_required = 0u32;
    for status in external::check_tools() {
        let mark = match (status.found, status.required) {
            (true, _) => "✓",
            (false, true) => "✗",
            (false, false) => "—",
        };
        let note = if !status.found && !status.required {
            "  (optional in-container; host-native Blender bake used instead)"
        } else {
            ""
        };
        println!("  {mark} {}{note}", status.name);
        if status.required && !status.found {
            missing_required += 1;
        }
    }
    println!();
    if missing_required == 0 {
        println!("All required tools found.");
    } else {
        println!(
            "{missing_required} required tool(s) missing — run inside the container, or see the setup docs."
        );
    }
    Ok(())
}
