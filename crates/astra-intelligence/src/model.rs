use crate::{
    Availability, ProjectEntity, ProjectInsight, ProjectLimitation, ProjectRelationship,
    ProjectRisk, VerificationValidity,
};

/// Versioned public contract for Project Intelligence JSON.
pub const PROJECT_INTELLIGENCE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProjectIdentityModel {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct IdentityModel {
    pub project_type: Availability<String>,
    pub languages: Vec<String>,
    pub build_systems: Vec<String>,
    pub workspace: Availability<String>,
    pub package_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ArchitectureModel {
    pub workspace_detected: Availability<bool>,
    pub package_structure: Availability<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CapabilityModel {
    pub discovered_actions: Vec<String>,
    pub controlled_execution_actions: Vec<String>,
    pub dry_run_only_actions: Vec<String>,
    pub unsupported_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VerificationModel {
    pub availability: Availability<String>,
    pub latest_action: Availability<String>,
    pub verdict: Availability<String>,
    pub validity: Availability<VerificationValidity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct KnowledgeSummary {
    pub facts: usize,
    pub decisions: usize,
    pub verifications: usize,
    pub goals: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RepositoryStatus {
    pub state: Availability<String>,
    pub clean: Availability<bool>,
    pub continuous_integration: Availability<Vec<String>>,
}

/// The complete, deterministic project-understanding report model.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProjectIntelligence {
    pub schema_version: u32,
    pub project: ProjectIdentityModel,
    pub identity: IdentityModel,
    pub architecture: ArchitectureModel,
    pub capabilities: CapabilityModel,
    pub verification: VerificationModel,
    pub knowledge: KnowledgeSummary,
    pub repository: RepositoryStatus,
    pub entities: Vec<ProjectEntity>,
    pub relationships: Vec<ProjectRelationship>,
    pub insights: Vec<ProjectInsight>,
    pub risks: Vec<ProjectRisk>,
    pub limitations: Vec<ProjectLimitation>,
}
