use std::{io, path::PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KnowledgeError {
    #[error("invalid knowledge claim: {0}")]
    InvalidClaim(String),

    #[error("knowledge claim contains a sensitive field: {0}")]
    SensitiveField(String),

    #[error("invalid knowledge identifier: {0}")]
    InvalidId(String),

    #[error("invalid knowledge project name: {0}")]
    InvalidProjectName(String),

    #[error("knowledge claim not found: {0}")]
    ClaimNotFound(String),

    #[error("knowledge relationship endpoint does not exist: {0}")]
    RelationshipEndpointMissing(String),

    #[error("knowledge relationship cannot connect a claim to itself")]
    SelfRelationship,

    #[error("knowledge storage I/O failed at {path}: {source}")]
    Io { path: PathBuf, source: io::Error },

    #[error("knowledge serialization failed at {path}: {source}")]
    Serialization {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("knowledge file is corrupt at {path}: {message}")]
    Corrupt { path: PathBuf, message: String },

    #[error("unsupported knowledge schema version {found}; supported version is {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },

    #[error("the default knowledge directory cannot be determined")]
    DefaultLocationUnavailable,
}
