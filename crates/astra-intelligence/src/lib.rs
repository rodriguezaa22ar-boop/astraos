mod analyzer;
mod authority;
mod entity;
mod error;
mod evidence;
mod id;
mod input;
mod insight;
mod model;
mod relationship;
mod report;
mod resolution;
mod rule;
mod status;

pub use analyzer::{DeterministicProjectIntelligenceAnalyzer, ProjectIntelligenceAnalyzer};
pub use authority::{authority_targets, entity_target_binding, insight_target_binding};
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
pub use resolution::{
    render_resolved_text, InterpretationStatus, OperatorAuthorityResolver, ResolutionConflict,
    ResolutionConflictKind, ResolutionExplanation, ResolutionStatus, ResolvedAnnotation,
    ResolvedInterpretation, ResolvedProjectIntelligence,
    RESOLVED_PROJECT_INTELLIGENCE_SCHEMA_VERSION,
};
pub use status::{Availability, IntelligenceConfidence, VerificationValidity};
