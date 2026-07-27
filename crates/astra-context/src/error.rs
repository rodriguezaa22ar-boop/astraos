use std::{io, path::PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("project root does not exist: {0}")]
    RootNotFound(PathBuf),

    #[error("project root is not a directory: {0}")]
    RootNotDirectory(PathBuf),

    #[error("could not resolve project root {path}: {source}")]
    RootCanonicalization {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("could not read project root {path}: {source}")]
    RootPermissionDenied {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("could not inspect project root {path}: {source}")]
    RootRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("invalid scan options: {0}")]
    InvalidOptions(String),

    #[error("project context serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("context invariant violated: {0}")]
    InvariantViolation(String),
}
