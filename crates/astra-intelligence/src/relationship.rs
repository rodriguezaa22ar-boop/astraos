use crate::{
    EntityId, InformationClassification, IntelligenceConfidence, IntelligenceError,
    IntelligenceEvidenceRef, RelationshipId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRelationshipKind {
    Contains,
    UsesLanguage,
    UsesBuildSystem,
    ProvidesAction,
    ActionValidatesProject,
    VerificationAppliesToProject,
    DecisionGovernsEntity,
    ClaimSupportsEntity,
    CapabilityRestrictedToDryRun,
    CapabilityAllowedForControlledExecution,
}

impl ProjectRelationshipKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::UsesLanguage => "uses_language",
            Self::UsesBuildSystem => "uses_build_system",
            Self::ProvidesAction => "provides_action",
            Self::ActionValidatesProject => "action_validates_project",
            Self::VerificationAppliesToProject => "verification_applies_to_project",
            Self::DecisionGovernsEntity => "decision_governs_entity",
            Self::ClaimSupportsEntity => "claim_supports_entity",
            Self::CapabilityRestrictedToDryRun => "capability_restricted_to_dry_run",
            Self::CapabilityAllowedForControlledExecution => {
                "capability_allowed_for_controlled_execution"
            }
        }
    }
}

/// A current graph edge, distinct from a persisted knowledge relationship.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProjectRelationship {
    pub id: RelationshipId,
    pub kind: ProjectRelationshipKind,
    pub source: EntityId,
    pub target: EntityId,
    pub classification: InformationClassification,
    pub confidence: IntelligenceConfidence,
    pub evidence: Vec<IntelligenceEvidenceRef>,
}

impl ProjectRelationship {
    pub(crate) fn new(
        kind: ProjectRelationshipKind,
        source: EntityId,
        target: EntityId,
        classification: InformationClassification,
        confidence: IntelligenceConfidence,
        mut evidence: Vec<IntelligenceEvidenceRef>,
    ) -> Result<Self, IntelligenceError> {
        if source == target || evidence.is_empty() {
            return Err(IntelligenceError::InvalidRelationship(
                "relationships require distinct endpoints and evidence".to_string(),
            ));
        }
        evidence.sort();
        evidence.dedup();
        let id = RelationshipId::derive(kind.as_str(), &[source.as_str(), target.as_str()]);
        Ok(Self {
            id,
            kind,
            source,
            target,
            classification,
            confidence,
            evidence,
        })
    }

    pub(crate) fn merge_evidence(&mut self, other: &Self) -> Result<(), IntelligenceError> {
        if self.kind != other.kind
            || self.source != other.source
            || self.target != other.target
            || self.classification != other.classification
        {
            return Err(IntelligenceError::DuplicateSemanticIdentity(
                self.id.to_string(),
            ));
        }
        self.evidence.extend(other.evidence.iter().cloned());
        self.evidence.sort();
        self.evidence.dedup();
        self.confidence = self.confidence.max(other.confidence);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relationships_reject_missing_evidence_and_self_edges() {
        let entity = EntityId::derive("entity", &["demo"]);
        assert!(ProjectRelationship::new(
            ProjectRelationshipKind::Contains,
            entity.clone(),
            entity,
            InformationClassification::Observed,
            IntelligenceConfidence::High,
            Vec::new(),
        )
        .is_err());
    }
}
