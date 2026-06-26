//! `modelgen batch` — reconstruct a directory of objects.

use anyhow::Result;
use modelgen_core::pipeline::{self, ReconstructConfig};
use std::path::{Path, PathBuf};

/// Settings shared by every object in a batch.
pub struct BatchOpts {
    /// Reconstruction settings (downscale, mask, quality, max edge, clean).
    pub recon: ReconstructConfig,
    /// Re-process objects even if their output already exists.
    pub force: bool,
}

/// Reconstruct every object subfolder into `output_dir/<name>/`. Resumable
/// (skips objects already reconstructed) and fault-tolerant (a failed object is
/// logged to `manifest.txt` and the batch continues).
pub fn batch(input_dir: &Path, output_dir: &Path, opts: &BatchOpts) -> Result<()> {
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
        let work = output_dir.join(&name);

        if is_reconstructed(&work) && !opts.force {
            println!("[skip] {name}");
            manifest.push_str(&format!("skipped\t{name}\n"));
            skipped += 1;
        } else {
            println!("[run ] {name}");
            match pipeline::reconstruct(obj, &work, &opts.recon) {
                Ok(mesh) => {
                    println!("[ ok ] {name} -> {}", mesh.display());
                    manifest.push_str(&format!("ok\t{name}\t{}\n", mesh.display()));
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

/// A reconstruction is "done" if its textured mesh already exists in the work dir.
fn is_reconstructed(work: &Path) -> bool {
    work.join("scene_textured.glb").exists() || work.join("scene_textured.ply").exists()
}
