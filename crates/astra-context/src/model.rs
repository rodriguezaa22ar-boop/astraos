use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectPath(pub String);

impl ProjectPath {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProjectPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Certain,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum EvidenceSource {
    File,
    Manifest,
    Git,
    Convention,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Evidence {
    pub source: EvidenceSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<ProjectPath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    pub rule: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detected<T> {
    pub value: T,
    pub confidence: Confidence,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub scanner: String,
    pub code: String,
    pub severity: DiagnosticSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<ProjectPath>,
    pub message: String,
}

impl Diagnostic {
    pub(crate) fn new(
        scanner: impl Into<String>,
        code: impl Into<String>,
        severity: DiagnosticSeverity,
        path: Option<ProjectPath>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            scanner: scanner.into(),
            code: code.into(),
            severity,
            path,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScannerMetadata {
    pub id: String,
    pub version: u32,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScannerStatus {
    Complete,
    Partial,
    Unavailable,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScannerResult {
    pub metadata: ScannerMetadata,
    pub status: ScannerStatus,
    pub findings: usize,
    pub diagnostic_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectIdentity {
    pub root: ProjectPath,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_root: Option<Detected<ProjectPath>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryState {
    Git,
    NotRepository,
    GitUnavailable,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitChange {
    pub path: ProjectPath,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentCommit {
    pub id: String,
    pub authored_at: String,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryContext {
    pub state: Detected<RepositoryState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<Detected<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<Detected<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clean: Option<Detected<bool>>,
    pub changes: Vec<Detected<GitChange>>,
    pub recent_commits: Vec<Detected<RecentCommit>>,
}

impl Default for RepositoryContext {
    fn default() -> Self {
        Self {
            state: Detected {
                value: RepositoryState::NotRepository,
                confidence: Confidence::Low,
                evidence: Vec::new(),
            },
            branch: None,
            head: None,
            clean: None,
            changes: Vec::new(),
            recent_commits: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageSummary {
    pub id: String,
    pub file_count: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageSummary {
    pub name: String,
    pub path: ProjectPath,
    pub ecosystem: String,
    pub manifest: ProjectPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceSummary {
    pub kinds: Vec<Detected<String>>,
    pub packages: Vec<Detected<PackageSummary>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSummary {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ToolingSummary {
    pub package_managers: Vec<Detected<ToolSummary>>,
    pub build_systems: Vec<Detected<ToolSummary>>,
    pub testing_frameworks: Vec<Detected<ToolSummary>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyScope {
    Runtime,
    Development,
    Build,
    Optional,
    Peer,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencySummary {
    pub ecosystem: String,
    pub package: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirement: Option<String>,
    pub scope: DependencyScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Readme,
    Architecture,
    Adr,
    Milestone,
    Contributing,
    Changelog,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSummary {
    pub path: ProjectPath,
    pub kind: DocumentKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub headings: Vec<String>,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiDefinition {
    pub provider: String,
    pub path: ProjectPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigurationFile {
    pub tool: String,
    pub path: ProjectPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandPurpose {
    Develop,
    Build,
    Test,
    Lint,
    Format,
    Validate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandSpec {
    pub executable: String,
    pub arguments: Vec<String>,
    pub working_directory: ProjectPath,
    pub purpose: CommandPurpose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryPointKind {
    Binary,
    Library,
    Application,
    Script,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryPoint {
    pub path: ProjectPath,
    pub kind: EntryPointKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LicenseSummary {
    pub declared: Vec<Detected<String>>,
    pub files: Vec<Detected<ProjectPath>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSize {
    pub files: u64,
    pub bytes: u64,
    pub source_files: u64,
    pub test_files: u64,
    pub documentation_files: u64,
    pub configuration_files: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Insight {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub observation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectContext {
    pub identity: ProjectIdentity,
    pub repository: RepositoryContext,
    pub languages: Vec<Detected<LanguageSummary>>,
    pub workspace: WorkspaceSummary,
    pub tooling: ToolingSummary,
    pub dependencies: Vec<Detected<DependencySummary>>,
    pub documentation: Vec<Detected<DocumentSummary>>,
    pub ci: Vec<Detected<CiDefinition>>,
    pub configuration: Vec<Detected<ConfigurationFile>>,
    pub entry_points: Vec<Detected<EntryPoint>>,
    pub development_commands: Vec<Detected<CommandSpec>>,
    pub validation_commands: Vec<Detected<CommandSpec>>,
    pub size: Detected<ProjectSize>,
    pub license: LicenseSummary,
}
