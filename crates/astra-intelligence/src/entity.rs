use crate::{EntityId, IntelligenceConfidence, IntelligenceError, IntelligenceEvidenceRef};

/// Whether information is direct observation, a deterministic derivation, or
/// an authoritative operator-owned decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InformationClassification {
    Observed,
    Derived,
    OperatorDecided,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Project,
    Workspace,
    Package,
    Language,
    BuildSystem,
    Action,
    ExecutionCapability,
    Verification,
    KnowledgeClaim,
    Decision,
    ContinuousIntegration,
}

impl EntityKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Workspace => "workspace",
            Self::Package => "package",
            Self::Language => "language",
            Self::BuildSystem => "build_system",
            Self::Action => "action",
            Self::ExecutionCapability => "execution_capability",
            Self::Verification => "verification",
            Self::KnowledgeClaim => "knowledge_claim",
            Self::Decision => "decision",
            Self::ContinuousIntegration => "continuous_integration",
        }
    }
}

/// A safe, runtime entity in the project-understanding graph.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProjectEntity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub name: String,
    pub classification: InformationClassification,
    pub confidence: IntelligenceConfidence,
    pub evidence: Vec<IntelligenceEvidenceRef>,
    #[serde(skip)]
    pub(crate) semantic_key: String,
}

impl ProjectEntity {
    pub(crate) fn new(
        kind: EntityKind,
        semantic_key: impl Into<String>,
        name: impl Into<String>,
        classification: InformationClassification,
        confidence: IntelligenceConfidence,
        mut evidence: Vec<IntelligenceEvidenceRef>,
    ) -> Result<Self, IntelligenceError> {
        let semantic_key = semantic_key.into();
        let name = name.into();
        if semantic_key.is_empty() || name.is_empty() || evidence.is_empty() {
            return Err(IntelligenceError::InvalidInput(
                "entities require a semantic key, name, and evidence".to_string(),
            ));
        }
        evidence.sort();
        evidence.dedup();
        Ok(Self {
            id: EntityId::derive(kind.as_str(), &[&semantic_key, &name]),
            kind,
            name,
            classification,
            confidence,
            evidence,
            semantic_key,
        })
    }

    pub(crate) fn merge_evidence(&mut self, other: &Self) -> Result<(), IntelligenceError> {
        if self.kind != other.kind
            || self.name != other.name
            || self.classification != other.classification
            || self.semantic_key != other.semantic_key
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
    fn semantic_ids_are_stable_and_serialization_hides_internal_keys() {
        let entity = ProjectEntity::new(
            EntityKind::Package,
            "cargo:app:crates/app",
            "app",
            InformationClassification::Observed,
            IntelligenceConfidence::High,
            vec![IntelligenceEvidenceRef::context("workspace.packages")],
        )
        .expect("entity");
        let same = ProjectEntity::new(
            EntityKind::Package,
            "cargo:app:crates/app",
            "app",
            InformationClassification::Observed,
            IntelligenceConfidence::High,
            vec![IntelligenceEvidenceRef::context("workspace.packages")],
        )
        .expect("entity");
        assert_eq!(entity.id, same.id);
        let json = serde_json::to_string(&entity).expect("JSON");
        assert!(!json.contains("semantic_key"));
        assert!(!json.contains("crates/app"));
    }
}
