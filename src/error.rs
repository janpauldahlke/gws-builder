//! Error types for the builder pipeline.

use std::path::PathBuf;

use thiserror::Error;

/// Errors produced by `gws-builder`.
#[derive(Debug, Error)]
pub enum BuilderError {
    /// Network or fetch failure for a Discovery document.
    #[error("network fetch failed for {service}/{version}: {source}")]
    Fetch {
        service: String,
        version: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// JSON parse failure for a Discovery document.
    #[error("failed to parse Discovery document for {service}: {source}")]
    Parse {
        service: String,
        source: serde_json::Error,
    },

    /// Schema resolution, filtering, or reference graph error.
    #[error("schema resolution error: {0}")]
    Resolution(String),

    /// Code generation failure.
    #[error("code generation error: {0}")]
    Codegen(String),

    /// File I/O error while writing generated output.
    #[error("file I/O error writing to {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}
