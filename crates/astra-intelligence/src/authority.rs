use crate::{InformationClassification, IntelligenceError, ProjectEntity, ProjectInsight};
use astra_knowledge::{OperatorTargetBinding, OperatorTargetClassification, OperatorTargetKind};

pub fn insight_target_binding(
    insight: &ProjectInsight,
) -> Result<OperatorTargetBinding, IntelligenceError> {
    OperatorTargetBinding::new(
        insight.id.to_string(),
        OperatorTargetKind::Insight,
        target_classification(insight.classification),
        Some(insight.rule_id.to_string()),
        insight.statement.clone(),
        insight
            .evidence
            .iter()
            .map(crate::IntelligenceEvidenceRef::canonical_key)
            .collect(),
        insight
            .related_entities
            .iter()
            .map(ToString::to_string)
            .collect(),
    )
    .map_err(|error| IntelligenceError::InvalidInput(error.to_string()))
}

pub fn entity_target_binding(
    entity: &ProjectEntity,
) -> Result<OperatorTargetBinding, IntelligenceError> {
    OperatorTargetBinding::new(
        entity.id.to_string(),
        OperatorTargetKind::Entity,
        target_classification(entity.classification),
        None,
        entity.name.clone(),
        entity
            .evidence
            .iter()
            .map(crate::IntelligenceEvidenceRef::canonical_key)
            .collect(),
        Vec::new(),
    )
    .map_err(|error| IntelligenceError::InvalidInput(error.to_string()))
}

pub fn authority_targets(
    intelligence: &crate::ProjectIntelligence,
) -> Result<Vec<OperatorTargetBinding>, IntelligenceError> {
    let mut targets = intelligence
        .insights
        .iter()
        .map(insight_target_binding)
        .chain(intelligence.entities.iter().map(entity_target_binding))
        .collect::<Result<Vec<_>, _>>()?;
    targets.sort_by(|left, right| {
        left.target_kind
            .cmp(&right.target_kind)
            .then_with(|| left.rule_id.cmp(&right.rule_id))
            .then_with(|| left.target_id.cmp(&right.target_id))
    });
    Ok(targets)
}

fn target_classification(
    classification: InformationClassification,
) -> OperatorTargetClassification {
    match classification {
        InformationClassification::Observed => OperatorTargetClassification::Observed,
        InformationClassification::Derived => OperatorTargetClassification::Derived,
        InformationClassification::OperatorDecided => OperatorTargetClassification::OperatorDecided,
    }
}
