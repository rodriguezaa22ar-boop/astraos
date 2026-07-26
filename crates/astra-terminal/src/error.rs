use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("unknown workspace: {0}")]
    UnknownWorkspace(String),

    #[error("unknown workspace layout: {0}")]
    UnknownLayout(String),

    #[error(
        "workspace layout '{layout}' was not found for workspace '{workspace}'; pass --layout <layout-name> to select a different layout"
    )]
    MissingDefaultLayout { workspace: String, layout: String },

    #[error("invalid workspace layout '{layout}': {message}")]
    InvalidConfiguration { layout: String, message: String },

    #[error("workspace directory is unavailable: {0}")]
    WorkspaceDirectory(PathBuf),

    #[error("terminal executable is unavailable: {0}")]
    ExecutableUnavailable(String),

    #[error("WezTerm CLI is available, but no GUI or mux server is reachable: {stderr}")]
    MuxUnavailable { stderr: String },

    #[error("WezTerm mux workspace already exists: {0}")]
    ExistingMuxWorkspace(String),

    #[error("operation '{operation}' failed with exit status {status}: {stderr}")]
    CommandFailed {
        operation: String,
        status: String,
        stderr: String,
    },

    #[error("operation '{operation}' timed out")]
    CommandTimedOut { operation: String },

    #[error("could not execute '{operation}': {source}")]
    ProcessExecution {
        operation: String,
        #[source]
        source: std::io::Error,
    },

    #[error("malformed output from '{operation}': {message}")]
    MalformedOutput { operation: String, message: String },

    #[error("timed out discovering the initial pane for WezTerm workspace '{workspace}'")]
    StartupDiscoveryTimeout { workspace: String },

    #[error(
        "initial pane discovery for WezTerm workspace '{workspace}' was ambiguous ({candidates} candidates)"
    )]
    AmbiguousStartupDiscovery {
        workspace: String,
        candidates: usize,
    },

    #[error("launch failed during '{operation}': {source}; a partial WezTerm layout may remain")]
    PartialLaunchFailure {
        operation: String,
        #[source]
        source: Box<TerminalError>,
    },
}

impl TerminalError {
    pub(crate) fn partial(operation: impl Into<String>, source: Self) -> Self {
        Self::PartialLaunchFailure {
            operation: operation.into(),
            source: Box::new(source),
        }
    }
}
