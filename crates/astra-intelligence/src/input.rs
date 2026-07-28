use crate::{IntelligenceConfidence, IntelligenceEvidenceRef, VerificationValidity};

/// Stable project identity supplied by the integration boundary. It intentionally
/// excludes a local filesystem root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectIdentityInput {
    pub name: String,
    pub project_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePackageInput {
    pub name: String,
    pub ecosystem: String,
    /// A project-relative semantic location, never an absolute path.
    pub relative_path: String,
    pub confidence: IntelligenceConfidence,
    pub evidence: Vec<IntelligenceEvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectContextInput {
    pub workspace_kinds: Vec<String>,
    pub packages: Vec<WorkspacePackageInput>,
    pub languages: Vec<String>,
    pub build_systems: Vec<String>,
    pub continuous_integration: Vec<String>,
    pub repository: RepositoryInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryInput {
    pub state: Option<String>,
    pub clean: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionInput {
    pub id: String,
    pub confidence: IntelligenceConfidence,
    pub evidence: Vec<IntelligenceEvidenceRef>,
}

/// Neutral execution-policy information. This represents availability only;
/// it contains neither an execution plan nor a command invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionCapabilityInput {
    pub discovered_actions: Vec<String>,
    pub controlled_actions: Vec<String>,
    pub dry_run_only_actions: Vec<String>,
    pub unsupported_actions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum KnowledgeCategoryInput {
    Fact,
    Decision,
    Verification,
    Goal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeClaimInput {
    pub id: String,
    pub category: KnowledgeCategoryInput,
    pub predicate: String,
    pub confidence: IntelligenceConfidence,
    pub validity: VerificationValidity,
    /// Historical creation time is used only to select the latest verification;
    /// it is never emitted by the intelligence report.
    pub created_at: String,
    pub verification_action: Option<String>,
    pub verification_verdict: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectKnowledgeInput {
    pub claims: Vec<KnowledgeClaimInput>,
}

/// Complete, explicit input for the pure intelligence analyzer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectIntelligenceInput {
    pub project: ProjectIdentityInput,
    pub context: ProjectContextInput,
    pub actions: Vec<ActionInput>,
    pub execution_capabilities: ExecutionCapabilityInput,
    pub knowledge: ProjectKnowledgeInput,
}
