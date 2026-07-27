mod engine;
mod error;
mod facts;
mod insights;
mod inventory;
mod manifests;
mod model;
mod options;
mod policy;
mod process;
mod projection;
mod render;
mod report;
mod scanner;
mod scanners;
mod scope;

pub use engine::{analyze, ProjectAnalyzer};
pub use error::ContextError;
pub use model::{
    CiDefinition, CommandPurpose, CommandSpec, Confidence, ConfigurationFile, DependencyScope,
    DependencySummary, Detected, Diagnostic, DiagnosticSeverity, DocumentKind, DocumentSummary,
    EntryPoint, EntryPointKind, Evidence, EvidenceSource, GitChange, Insight, LanguageSummary,
    LicenseSummary, PackageSummary, ProjectContext, ProjectIdentity, ProjectPath, ProjectSize,
    RecentCommit, RepositoryContext, RepositoryState, ScannerMetadata, ScannerResult,
    ScannerStatus, ToolSummary, ToolingSummary, WorkspaceSummary,
};
pub use options::ScanOptions;
pub use render::{render_json, render_text, render_tree};
pub use report::{ScanReport, PROJECT_CONTEXT_SCHEMA_VERSION};
