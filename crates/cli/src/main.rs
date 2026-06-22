//! `modelgen` — command-line interface for the lo-fi 3D asset generator.

use anyhow::Result;
use clap::{Parser, Subcommand};
use modelgen_core::{Pipeline, PipelineConfig, external};
use std::path::PathBuf;

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
    }
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
