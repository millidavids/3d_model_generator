//! Thin wrappers for invoking external tools as subprocesses, plus the
//! environment checks behind the CLI `doctor` command.
//!
//! The heavy lifting lives in C++/Python tools (COLMAP, OpenMVS, Blender,
//! rembg) that we drive out-of-process. Running them as separate processes —
//! never linking them — also keeps this crate's license independent of the
//! tools' (notably OpenMVS's AGPL).

use crate::error::{Error, Result};
use std::process::Command;

/// Single-binary tools that must be present for reconstruction (the core pipeline).
pub const REQUIRED_TOOLS: &[&str] = &["colmap", "rembg"];

/// The bake tool. Optional *in-container*: there is no arm64-Linux Blender, so on
/// Apple Silicon the bake runs host-native instead (see the `Baker` design).
pub const BAKE_TOOL: &str = "blender";

/// OpenMVS ships several binaries; the dense → mesh → texture steps we use.
pub const OPENMVS_TOOLS: &[&str] = &[
    "InterfaceCOLMAP",
    "DensifyPointCloud",
    "ReconstructMesh",
    "TextureMesh",
];

/// Presence (and, later, smoke-test) status of one external tool.
#[derive(Debug)]
pub struct ToolStatus {
    /// The binary name as invoked.
    pub name: String,
    /// Whether it resolves on `PATH`.
    pub found: bool,
    /// Whether its absence is a hard failure (vs. optional, e.g. in-container Blender).
    pub required: bool,
    /// Reported version, once `doctor` runs real smoke checks (TODO Phase 0).
    pub version: Option<String>,
}

/// Check that every required tool resolves on `PATH`.
///
/// This is the cheap presence check. The full `doctor` additionally runs real
/// smoke inferences (a 1-image `rembg`, a tiny COLMAP/OpenMVS run on a fixture,
/// a cube bake), since "binary resolves" does not prove "binary works" — e.g.
/// onnxruntime can import yet crash with an illegal instruction on some arm64
/// setups. TODO(phase 0): add the smoke runs.
pub fn check_tools() -> Vec<ToolStatus> {
    let mut statuses: Vec<ToolStatus> = REQUIRED_TOOLS
        .iter()
        .chain(OPENMVS_TOOLS.iter())
        .copied()
        .map(|name| ToolStatus {
            name: name.to_string(),
            found: is_on_path(name),
            required: true,
            version: None,
        })
        .collect();
    // Blender is optional in-container: absent on arm64, where the bake is host-native.
    statuses.push(ToolStatus {
        name: BAKE_TOOL.to_string(),
        found: is_on_path(BAKE_TOOL),
        required: false,
        version: None,
    });
    statuses
}

/// Returns true if `tool` is resolvable on `PATH`.
///
/// Uses `command -v`, which is portable across the Linux container and the
/// macOS host shell.
pub(crate) fn is_on_path(tool: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool}"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run an external tool to completion, erroring if it is missing or exits
/// non-zero.
pub fn run(tool: &str, args: &[&str]) -> Result<()> {
    run_impl(None, tool, args)
}

/// Like [`run`], but executes the tool with `dir` as its working directory.
/// OpenMVS tools resolve their `-w` working folder and relative outputs there.
pub fn run_in(dir: &std::path::Path, tool: &str, args: &[&str]) -> Result<()> {
    run_impl(Some(dir), tool, args)
}

fn run_impl(dir: Option<&std::path::Path>, tool: &str, args: &[&str]) -> Result<()> {
    if !is_on_path(tool) {
        return Err(Error::ToolNotFound(tool.to_string()));
    }
    let mut cmd = Command::new(tool);
    cmd.args(args);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    tracing::info!(tool, ?args, "running external tool");
    let status = cmd.status()?;
    if !status.success() {
        return Err(Error::ToolFailed {
            tool: tool.to_string(),
            status: status.code().unwrap_or(-1),
        });
    }
    Ok(())
}

/// Convert a path to `&str` for passing to a tool, erroring (not panicking) on
/// non-UTF-8 paths.
pub fn path_str(p: &std::path::Path) -> Result<&str> {
    p.to_str()
        .ok_or_else(|| Error::InvalidPath(p.to_path_buf()))
}
