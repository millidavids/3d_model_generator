//! The crate error type.
//!
//! Per the project conventions, the library exposes a `thiserror` enum; the CLI
//! adds human-facing context with `anyhow`.

/// Convenient result alias used throughout the crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors produced while generating an asset.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A required external tool was not resolvable on `PATH`.
    #[error("required tool `{0}` not found on PATH")]
    ToolNotFound(String),

    /// An external tool ran but exited with a non-zero status.
    #[error("`{tool}` exited with status {status}")]
    ToolFailed { tool: String, status: i32 },

    /// A stage's output failed its gate check (e.g. too few images registered,
    /// empty dense cloud). Carries enough context to fail the object gracefully.
    #[error("stage `{stage}` gate check failed: {reason}")]
    GateFailed { stage: String, reason: String },

    /// A path could not be represented as UTF-8 for passing to an external tool.
    #[error("path is not valid UTF-8: {0}")]
    InvalidPath(std::path::PathBuf),

    /// An underlying I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
