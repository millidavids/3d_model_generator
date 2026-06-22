//! `modelgen batch` — process a directory of objects end-to-end.

use anyhow::Result;
use modelgen_core::pipeline::{self, LofiConfig, ReconstructConfig};
use std::path::{Path, PathBuf};

/// Front + back-half settings shared by every object in a batch.
pub struct BatchOpts {
    /// Reconstruction settings (downscale, mask, max edge).
    pub recon: ReconstructConfig,
    /// Lo-fi back-half settings (budgets, pixelation).
    pub lofi: LofiConfig,
    /// Re-process objects even if their output already exists.
    pub force: bool,
}

/// Batch end-to-end over object subfolders (one folder of photos per object).
/// Resumable (skips objects whose output exists) and fault-tolerant (a failed
/// object is logged to `manifest.txt` and the batch continues).
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
                let mesh = pipeline::reconstruct(obj, &work, &opts.recon)?;
                Ok(pipeline::lofi(&mesh, &out, &opts.lofi)?)
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
