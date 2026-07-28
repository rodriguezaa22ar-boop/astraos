use crate::{
    EntityId, InformationClassification, InsightId, IntelligenceConfidence, IntelligenceError,
    IntelligenceEvidenceRef, LimitationId, RiskId, RuleId,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProjectInsight {
    pub id: InsightId,
    pub rule_id: RuleId,
    pub statement: String,
    pub classification: InformationClassification,
    pub confidence: IntelligenceConfidence,
    pub evidence: Vec<IntelligenceEvidenceRef>,
    pub related_entities: Vec<EntityId>,
}

impl ProjectInsight {
    pub(crate) fn new(
        rule_id: RuleId,
        statement: impl Into<String>,
        confidence: IntelligenceConfidence,
        mut evidence: Vec<IntelligenceEvidenceRef>,
        mut related_entities: Vec<EntityId>,
    ) -> Result<Self, IntelligenceError> {
        let statement = statement.into();
        if statement.is_empty() || evidence.is_empty() {
            return Err(IntelligenceError::InsightMissingEvidence(
                rule_id.to_string(),
            ));
        }
        evidence.sort();
        evidence.dedup();
        related_entities.sort();
        related_entities.dedup();
        let id = InsightId::derive(
            "insight",
            &[rule_id.as_str(), &statement, &evidence_key(&evidence)],
        );
        Ok(Self {
            id,
            rule_id,
            statement,
            classification: InformationClassification::Derived,
            confidence,
            evidence,
            related_entities,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProjectRisk {
    pub id: RiskId,
    pub statement: String,
    pub classification: InformationClassification,
    pub evidence: Vec<IntelligenceEvidenceRef>,
}

impl ProjectRisk {
    pub(crate) fn new(
        statement: impl Into<String>,
        mut evidence: Vec<IntelligenceEvidenceRef>,
    ) -> Result<Self, IntelligenceError> {
        let statement = statement.into();
        if statement.is_empty() || evidence.is_empty() {
            return Err(IntelligenceError::InvalidInput(
                "risks require a statement and evidence".to_string(),
            ));
        }
        evidence.sort();
        evidence.dedup();
        Ok(Self {
            id: RiskId::derive("risk", &[&statement, &evidence_key(&evidence)]),
            statement,
            classification: InformationClassification::Derived,
            evidence,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProjectLimitation {
    pub id: LimitationId,
    pub statement: String,
    pub evidence: Vec<IntelligenceEvidenceRef>,
}

impl ProjectLimitation {
    pub(crate) fn new(
        statement: impl Into<String>,
        mut evidence: Vec<IntelligenceEvidenceRef>,
    ) -> Result<Self, IntelligenceError> {
        let statement = statement.into();
        if statement.is_empty() || evidence.is_empty() {
            return Err(IntelligenceError::InvalidInput(
                "limitations require a statement and evidence".to_string(),
            ));
        }
        evidence.sort();
        evidence.dedup();
        Ok(Self {
            id: LimitationId::derive("limitation", &[&statement, &evidence_key(&evidence)]),
            statement,
            evidence,
        })
    }
}

fn evidence_key(evidence: &[IntelligenceEvidenceRef]) -> String {
    evidence
        .iter()
        .map(IntelligenceEvidenceRef::canonical_key)
        .collect::<Vec<_>>()
        .join("\u{1f}")
}
