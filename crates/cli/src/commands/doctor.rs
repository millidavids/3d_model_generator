//! `modelgen doctor` — report which external tools are available.

use anyhow::Result;
use modelgen_core::external;

/// Report which external tools are present.
pub fn doctor() -> Result<()> {
    println!("Checking external tools:");
    let mut missing_required = 0u32;
    for status in external::check_tools() {
        let mark = if status.found { "✓" } else { "✗" };
        let ver = status
            .version
            .as_deref()
            .map(|v| format!("  [{v}]"))
            .unwrap_or_default();
        println!("  {mark} {}{ver}", status.name);
        if !status.found {
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
