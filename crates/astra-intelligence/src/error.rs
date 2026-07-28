#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IntelligenceError {
    #[error("invalid intelligence input: {0}")]
    InvalidInput(String),
    #[error("invalid relationship: {0}")]
    InvalidRelationship(String),
    #[error("relationship endpoint is missing: {0}")]
    MissingRelationshipEndpoint(String),
    #[error("duplicate semantic identity conflict: {0}")]
    DuplicateSemanticIdentity(String),
    #[error("derived insight requires evidence: {0}")]
    InsightMissingEvidence(String),
    #[error("inconsistent execution capability information: {0}")]
    InconsistentCapability(String),
}
