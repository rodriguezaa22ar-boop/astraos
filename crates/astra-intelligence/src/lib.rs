mod analyzer;
mod entity;
mod error;
mod evidence;
mod id;
mod input;
mod insight;
mod model;
mod relationship;
mod report;
mod rule;
mod status;

pub use analyzer::{DeterministicProjectIntelligenceAnalyzer, ProjectIntelligenceAnalyzer};
pub use entity::{EntityKind, InformationClassification, ProjectEntity};
pub use error::IntelligenceError;
pub use evidence::IntelligenceEvidenceRef;
pub use id::{EntityId, InsightId, LimitationId, RelationshipId, RiskId, RuleId};
pub use input::{
    ActionInput, ExecutionCapabilityInput, KnowledgeCategoryInput, KnowledgeClaimInput,
    ProjectContextInput, ProjectIdentityInput, ProjectIntelligenceInput, ProjectKnowledgeInput,
    RepositoryInput, WorkspacePackageInput,
};
pub use insight::{ProjectInsight, ProjectLimitation, ProjectRisk};
pub use model::{
    ArchitectureModel, CapabilityModel, IdentityModel, KnowledgeSummary, ProjectIdentityModel,
    ProjectIntelligence, RepositoryStatus, VerificationModel, PROJECT_INTELLIGENCE_SCHEMA_VERSION,
};
pub use relationship::{ProjectRelationship, ProjectRelationshipKind};
pub use report::render_text;
pub use status::{Availability, IntelligenceConfidence, VerificationValidity};
