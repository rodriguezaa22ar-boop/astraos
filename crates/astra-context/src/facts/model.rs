use crate::{
    scope::SemanticScope, CommandPurpose, Confidence, DependencyScope, DocumentKind, Evidence,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FactId(pub(super) usize);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FactKey(pub(super) Fact);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FactKind {
    ProjectRoot,
    File,
    Manifest,
    Package,
    Workspace,
    Dependency,
    Command,
    Tool,
    Repository,
    Documentation,
    Marker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FileRole {
    Source,
    Test,
    Documentation,
    Configuration,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FileFact {
    pub(crate) path: String,
    pub(crate) bytes: u64,
    pub(crate) role: FileRole,
    pub(crate) extension: Option<String>,
    pub(crate) language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ManifestFact {
    pub(crate) path: String,
    pub(crate) ecosystem: String,
    pub(crate) kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PackageFact {
    pub(crate) name: String,
    pub(crate) root: String,
    pub(crate) ecosystem: String,
    pub(crate) manifest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct WorkspaceFact {
    pub(crate) kind: String,
    pub(crate) root: String,
    pub(crate) manifest: String,
    pub(crate) members: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DependencyFact {
    pub(crate) ecosystem: String,
    pub(crate) package: String,
    pub(crate) name: String,
    pub(crate) requirement: Option<String>,
    pub(crate) scope: DependencyScope,
    pub(crate) manifest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CommandFact {
    pub(crate) executable: String,
    pub(crate) arguments: Vec<String>,
    pub(crate) working_directory: String,
    pub(crate) purpose: CommandPurpose,
    pub(crate) source_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ToolCategory {
    PackageManager,
    BuildSystem,
    TestingFramework,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ToolFact {
    pub(crate) id: String,
    pub(crate) category: ToolCategory,
    pub(crate) source_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RepositoryFact {
    State(String),
    Root(String),
    Branch(String),
    Head(String),
    Clean(bool),
    Change {
        path: String,
        status: String,
    },
    Commit {
        ordinal: usize,
        id: String,
        authored_at: String,
        subject: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DocumentationFact {
    pub(crate) path: String,
    pub(crate) kind: DocumentKind,
    pub(crate) title: Option<String>,
    pub(crate) headings: Vec<String>,
    pub(crate) bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MarkerKind {
    Ci,
    Configuration,
    EntryPoint,
    LicenseFile,
    DeclaredLicense,
    InventoryComplete,
    InventoryPartial,
    InventoryTruncated,
    ManifestComplete,
    ManifestPartial,
    MissingWorkspaceMember,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MarkerFact {
    pub(crate) kind: MarkerKind,
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Fact {
    ProjectRoot(String),
    File(FileFact),
    Manifest(ManifestFact),
    Package(PackageFact),
    Workspace(WorkspaceFact),
    Dependency(DependencyFact),
    Command(CommandFact),
    Tool(ToolFact),
    Repository(RepositoryFact),
    Documentation(DocumentationFact),
    Marker(MarkerFact),
}

impl Fact {
    pub(crate) fn kind(&self) -> FactKind {
        match self {
            Self::ProjectRoot(_) => FactKind::ProjectRoot,
            Self::File(_) => FactKind::File,
            Self::Manifest(_) => FactKind::Manifest,
            Self::Package(_) => FactKind::Package,
            Self::Workspace(_) => FactKind::Workspace,
            Self::Dependency(_) => FactKind::Dependency,
            Self::Command(_) => FactKind::Command,
            Self::Tool(_) => FactKind::Tool,
            Self::Repository(_) => FactKind::Repository,
            Self::Documentation(_) => FactKind::Documentation,
            Self::Marker(_) => FactKind::Marker,
        }
    }

    pub(crate) fn stable_key(&self) -> FactKey {
        FactKey(self.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FactProvenance {
    pub(crate) scanner: String,
    pub(crate) scope: SemanticScope,
    pub(crate) confidence: Confidence,
    pub(crate) evidence: Vec<Evidence>,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredFact {
    pub(crate) id: FactId,
    pub(crate) fact: Fact,
    pub(crate) provenance: Vec<FactProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RelationKind {
    DeclaredBy,
    MemberOf,
    DependsOn,
    EntrypointOf,
    Supports,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FactRelation {
    pub(super) from: FactId,
    pub(super) to: FactId,
    pub(super) kind: RelationKind,
}
