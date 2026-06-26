//! `modelgen doctor` — report which external tools are available.

use anyhow::Result;
use modelgen_core::external;

/// Report which external tools are present. With `full`, also run a smoke test
/// (rembg + COLMAP on a tiny input) to catch tools that resolve but crash at work.
/// Exits non-zero if any *required* tool is missing or the smoke test fails, so
/// `modelgen doctor` can gate scripts/CI.
pub fn doctor(full: bool) -> Result<()> {
    println!("Checking external tools:");
    let mut missing_required = 0u32;
    for status in external::check_tools() {
        let mark = if status.found { "✓" } else { "✗" };
        let opt = if status.required { "" } else { " (optional)" };
        let ver = status
            .version
            .as_deref()
            .map(|v| format!("  [{v}]"))
            .unwrap_or_default();
        println!("  {mark} {}{opt}{ver}", status.name);
        // Only required-and-missing tools count as a failure; an absent optional
        // tool (e.g. RefineMesh, used only at --quality high) is just reported.
        if status.required && !status.found {
            missing_required += 1;
        }
    }
    println!();
    if missing_required > 0 {
        println!(
            "{missing_required} required tool(s) missing — run inside the container, or see the setup docs."
        );
        // Non-zero exit so `modelgen doctor && ...` gates correctly.
        std::process::exit(1);
    }
    println!("All required tools found.");

    if full {
        print!("Smoke test (rembg + COLMAP on a tiny input)... ");
        std::io::Write::flush(&mut std::io::stdout()).ok();
        match modelgen_core::smoke::smoke_test() {
            Ok(()) => println!("OK"),
            Err(e) => {
                println!("FAILED: {e}");
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
