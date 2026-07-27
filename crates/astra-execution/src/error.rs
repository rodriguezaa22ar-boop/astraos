use astra_actions::PolicyRejectionReason;
use std::{io, path::PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("real execution currently requires a Git repository")]
    NonGitExecutionUnsupported,

    #[error("policy rejected the action: {0}")]
    PolicyRejected(PolicyRejectionReason),

    #[error("real execution currently supports only the check action")]
    UnsupportedExecutionAction,

    #[error("unsupported execution plan schema version: {0}")]
    UnsupportedPlanSchema(u32),

    #[error("unsupported action policy version: {0}")]
    UnsupportedPolicyVersion(u32),

    #[error("project root does not exist or is not a directory: {0}")]
    InvalidProjectRoot(PathBuf),

    #[error("project root changed after planning; execution refused")]
    ProjectRootChanged,

    #[error("working directory changed or is no longer valid: {0}")]
    InvalidWorkingDirectory(PathBuf),

    #[error("working directory is outside the project root: {0}")]
    WorkingDirectoryOutsideProject(PathBuf),

    #[error("project state changed after planning; execution refused")]
    StateFingerprintMismatch,

    #[error("action fingerprint no longer matches the authorized plan")]
    ActionFingerprintMismatch,

    #[error("execution plan fingerprint is invalid")]
    PlanFingerprintMismatch,

    #[error("Git state capture failed: {0}")]
    GitStateCapture(String),

    #[error("Git output was malformed: {0}")]
    MalformedGitOutput(String),

    #[error("Git output exceeded the supported bound: {0}")]
    GitOutputLimitExceeded(String),

    #[error("Git state capture timed out during {0}")]
    GitCommandTimedOut(String),

    #[error("untracked source state exceeds execution fingerprint limits")]
    UntrackedStateLimitExceeded,

    #[error("unable to fingerprint untracked file {path}: {source}")]
    UntrackedFileUnreadable { path: PathBuf, source: io::Error },

    #[error("cannot execute {executable}: {source}")]
    SpawnFailed {
        executable: String,
        source: io::Error,
    },

    #[error("could not forward child output: {0}")]
    OutputForwardFailed(String),

    #[error("could not capture source state after execution: {0}")]
    PostExecutionStateCapture(String),

    #[error("execution process was interrupted")]
    Interrupted,
}
