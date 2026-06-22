//! `modelgen` — command-line interface for the lo-fi 3D asset generator.

use anyhow::Result;
use clap::{Parser, Subcommand};
use modelgen_core::external;
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
        target_tris: usize,
        /// Pixelated texture size (longest edge, px).
        #[arg(long, default_value_t = 128)]
        texture_size: u32,
        /// Palette colour count for the texture.
        #[arg(long, default_value_t = 256)]
        palette_colors: u16,
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
        /// Keep disconnected fragments (skip largest-component cleanup).
        #[arg(long)]
        keep_floaters: bool,
        /// Skip normalization (centering + unit scaling).
        #[arg(long)]
        no_normalize: bool,
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
        target_tris: usize,
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
        Commands::Doctor => doctor(),
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
            let recon = ReconOpts {
                no_downscale: false,
                mask,
                max_edge,
            };
            let lofi_opts = LofiOpts {
                target_tris,
                no_decimate: false,
                texture_size,
                palette_colors,
                no_pixelate: false,
                keep_floaters: false,
                no_normalize: false,
            };
            let mesh = reconstruct(&photos, &work, &recon)?;
            lofi(&mesh, &out, &lofi_opts)
        }
        Commands::Reconstruct {
            images,
            work,
            no_downscale,
            mask,
            max_edge,
        } => {
            let opts = ReconOpts {
                no_downscale,
                mask,
                max_edge,
            };
            let mesh = reconstruct(&images, &work, &opts)?;
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
        } => {
            let opts = LofiOpts {
                target_tris,
                no_decimate,
                texture_size,
                palette_colors,
                no_pixelate,
                keep_floaters,
                no_normalize,
            };
            lofi(&mesh, &out, &opts)
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
            let opts = BatchOpts {
                recon: ReconOpts {
                    no_downscale: false,
                    mask,
                    max_edge,
                },
                lofi: LofiOpts {
                    target_tris,
                    no_decimate: false,
                    texture_size,
                    palette_colors,
                    no_pixelate: false,
                    keep_floaters: false,
                    no_normalize: false,
                },
                force,
            };
            batch(&input_dir, &output_dir, &opts)
        }
    }
}

struct LofiOpts {
    target_tris: usize,
    no_decimate: bool,
    texture_size: u32,
    palette_colors: u16,
    no_pixelate: bool,
    keep_floaters: bool,
    no_normalize: bool,
}

/// [Phase 2] Convert a reconstructed textured mesh to a lo-fi glTF asset: import
/// → heal (largest component) → decimate → normalize → pixelate → glTF export
/// (unlit + nearest).
fn lofi(mesh_path: &Path, out: &Path, opts: &LofiOpts) -> Result<()> {
    use modelgen_core::{export, mesh, texture};

    let mut m = mesh::load_textured_ply(mesh_path)?;
    println!(
        "loaded: {} triangles, {} vertices",
        m.triangle_count(),
        m.vertex_count()
    );

    if !opts.keep_floaters {
        m = mesh::keep_largest_component(&m);
        println!("largest component: {} triangles", m.triangle_count());
    }

    if !opts.no_decimate {
        m = mesh::decimate(&m, opts.target_tris);
        println!(
            "decimated: {} triangles, {} vertices",
            m.triangle_count(),
            m.vertex_count()
        );
    }

    if !opts.no_normalize {
        mesh::normalize(&mut m);
        println!("normalized: centered + unit-scaled");
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

struct ReconOpts {
    no_downscale: bool,
    mask: bool,
    max_edge: u32,
}

/// Default scratch dir for `process`: "<out-stem>.work" beside the output.
fn default_work_dir(out: &Path) -> PathBuf {
    let stem = out
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "modelgen".to_string());
    out.with_file_name(format!("{stem}.work"))
}

/// [Phase 1] Reconstruct a textured mesh from photos: preprocess (downscale,
/// optional background masking), then COLMAP SfM + OpenMVS. Returns the mesh path.
fn reconstruct(images: &Path, work: &Path, opts: &ReconOpts) -> Result<PathBuf> {
    use modelgen_core::preprocess;

    // Downscale (unless skipped) to keep CPU reconstruction tractable.
    let downscaled = if opts.no_downscale {
        images.to_path_buf()
    } else {
        let out = work.join("images");
        let n = preprocess::downscale_images(images, &out, opts.max_edge)?;
        println!("downscaled {n} image(s)");
        out
    };

    // Optionally remove the background so the reconstructed mesh is object-only.
    let input = if opts.mask {
        let out = work.join("masked");
        let n = preprocess::mask_images(&downscaled, &out, &work.join("masks"))?;
        println!("masked {n} image(s) (background removed)");
        out
    } else {
        downscaled
    };

    Ok(modelgen_core::reconstruct::run(&input, work)?.textured_mesh)
}

struct BatchOpts {
    recon: ReconOpts,
    lofi: LofiOpts,
    force: bool,
}

/// [Phase 4] Batch end-to-end over a directory of object subfolders. Resumable
/// (skips objects whose output already exists) and fault-tolerant (a failed
/// object is recorded and the batch continues). Writes a `manifest.txt`.
fn batch(input_dir: &Path, output_dir: &Path, opts: &BatchOpts) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;

    let mut objects: Vec<PathBuf> = std::fs::read_dir(input_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    objects.sort();
    if objects.is_empty() {
        println!("no object subdirectories found in {}", input_dir.display());
        return Ok(());
    }
    println!("batch: {} object(s)", objects.len());

    let mut manifest = String::new();
    let (mut ok, mut failed, mut skipped) = (0u32, 0u32, 0u32);
    for obj in &objects {
        let name = obj
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let out = output_dir.join(format!("{name}.glb"));

        if out.exists() && !opts.force {
            println!("[skip] {name}");
            manifest.push_str(&format!("skipped\t{name}\n"));
            skipped += 1;
        } else {
            println!("[run ] {name}");
            let work = output_dir.join(format!("{name}.work"));
            // Catch per-object failures so one bad object doesn't abort the batch.
            let result = (|| -> Result<()> {
                let mesh = reconstruct(obj, &work, &opts.recon)?;
                lofi(&mesh, &out, &opts.lofi)
            })();
            match result {
                Ok(()) => {
                    println!("[ ok ] {name}");
                    manifest.push_str(&format!("ok\t{name}\n"));
                    ok += 1;
                }
                Err(e) => {
                    eprintln!("[fail] {name}: {e}");
                    manifest.push_str(&format!("failed\t{name}\t{e}\n"));
                    failed += 1;
                }
            }
        }
        // Persist the manifest after each object (progress survives interruption).
        std::fs::write(output_dir.join("manifest.txt"), &manifest)?;
    }

    println!("\nbatch complete: {ok} ok, {failed} failed, {skipped} skipped");
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
