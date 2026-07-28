use crate::{
    EntityId, IntelligenceConfidence, IntelligenceError, IntelligenceEvidenceRef, ProjectInsight,
    ProjectRisk, RuleId, VerificationValidity,
};
use std::collections::BTreeSet;

/// Pure input supplied to deterministic insight rules after graph construction.
#[derive(Debug, Clone)]
pub(crate) struct RuleContext {
    pub workspace_detected: bool,
    pub package_entities: Vec<EntityId>,
    pub workspace_entity: Option<EntityId>,
    pub discovered_actions: BTreeSet<String>,
    pub controlled_actions: BTreeSet<String>,
    pub check_action_entity: Option<EntityId>,
    pub check_controlled_execution_capability_entity: Option<EntityId>,
    pub restricted_actions: Vec<RestrictedActionRuleInput>,
    pub verification: Option<VerificationRuleInput>,
    pub package_structure_evidence: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct VerificationRuleInput {
    pub claim_id: String,
    pub validity: VerificationValidity,
    pub entity: EntityId,
}

#[derive(Debug, Clone)]
pub(crate) struct RestrictedActionRuleInput {
    pub action_id: String,
    pub action_entity: Option<EntityId>,
    pub dry_run_only_capability_entity: Option<EntityId>,
}

#[derive(Debug, Default)]
pub(crate) struct RuleOutput {
    pub insights: Vec<ProjectInsight>,
    pub risks: Vec<ProjectRisk>,
}

pub(crate) trait InsightRule {
    fn id(&self) -> RuleId;
    fn evaluate(&self, context: &RuleContext) -> Result<RuleOutput, IntelligenceError>;
}

pub(crate) fn default_rules() -> [Box<dyn InsightRule>; 6] {
    [
        Box::new(MultiPackageWorkspace),
        Box::new(ControlledVerification),
        Box::new(RestrictedActions),
        Box::new(StaleVerification),
        Box::new(OperatorDecisionAvailability),
        Box::new(ModularPackageStructure),
    ]
}

struct MultiPackageWorkspace;
impl InsightRule for MultiPackageWorkspace {
    fn id(&self) -> RuleId {
        RuleId::new(RuleId::PI_001)
    }

    fn evaluate(&self, context: &RuleContext) -> Result<RuleOutput, IntelligenceError> {
        if !context.workspace_detected || context.package_entities.len() < 2 {
            return Ok(RuleOutput::default());
        }
        let Some(workspace) = context.workspace_entity.clone() else {
            return Ok(RuleOutput::default());
        };
        Ok(RuleOutput {
            insights: vec![ProjectInsight::new(
                self.id(),
                "The project is organized as a multi-package workspace.",
                IntelligenceConfidence::Certain,
                vec![IntelligenceEvidenceRef::context("workspace.kinds")],
                std::iter::once(workspace)
                    .chain(context.package_entities.iter().cloned())
                    .collect(),
            )?],
            risks: Vec::new(),
        })
    }
}

struct ControlledVerification;
impl InsightRule for ControlledVerification {
    fn id(&self) -> RuleId {
        RuleId::new(RuleId::PI_002)
    }

    fn evaluate(&self, context: &RuleContext) -> Result<RuleOutput, IntelligenceError> {
        let Some(verification) = &context.verification else {
            return Ok(RuleOutput::default());
        };
        if !context.discovered_actions.contains("check")
            || !context.controlled_actions.contains("check")
        {
            return Ok(RuleOutput::default());
        }
        Ok(RuleOutput {
            insights: vec![ProjectInsight::new(
                self.id(),
                "The project supports controlled, evidence-producing verification.",
                IntelligenceConfidence::Certain,
                vec![
                    IntelligenceEvidenceRef::Action {
                        action_id: "check".to_string(),
                    },
                    IntelligenceEvidenceRef::ExecutionCapability {
                        action_id: "check".to_string(),
                    },
                    IntelligenceEvidenceRef::Verification {
                        claim_id: verification.claim_id.clone(),
                    },
                ],
                std::iter::once(verification.entity.clone())
                    .chain(context.check_action_entity.iter().cloned())
                    .chain(
                        context
                            .check_controlled_execution_capability_entity
                            .iter()
                            .cloned(),
                    )
                    .collect(),
            )?],
            risks: Vec::new(),
        })
    }
}

struct RestrictedActions;
impl InsightRule for RestrictedActions {
    fn id(&self) -> RuleId {
        RuleId::new(RuleId::PI_003)
    }

    fn evaluate(&self, context: &RuleContext) -> Result<RuleOutput, IntelligenceError> {
        if context.restricted_actions.is_empty() {
            return Ok(RuleOutput::default());
        }
        let mut evidence = Vec::new();
        let mut related_entities = Vec::new();
        for action in &context.restricted_actions {
            evidence.push(IntelligenceEvidenceRef::Action {
                action_id: action.action_id.clone(),
            });
            evidence.push(IntelligenceEvidenceRef::ExecutionCapability {
                action_id: action.action_id.clone(),
            });
            related_entities.extend(action.action_entity.iter().cloned());
            related_entities.extend(action.dry_run_only_capability_entity.iter().cloned());
        }
        Ok(RuleOutput {
            insights: vec![ProjectInsight::new(
                self.id(),
                "Some project actions are discoverable but remain restricted from direct execution.",
                IntelligenceConfidence::Certain,
                evidence,
                related_entities,
            )?],
            risks: Vec::new(),
        })
    }
}

struct StaleVerification;
impl InsightRule for StaleVerification {
    fn id(&self) -> RuleId {
        RuleId::new(RuleId::PI_004)
    }

    fn evaluate(&self, context: &RuleContext) -> Result<RuleOutput, IntelligenceError> {
        let Some(verification) = &context.verification else {
            return Ok(RuleOutput::default());
        };
        if verification.validity != VerificationValidity::Stale {
            return Ok(RuleOutput::default());
        }
        let evidence = vec![IntelligenceEvidenceRef::Verification {
            claim_id: verification.claim_id.clone(),
        }];
        Ok(RuleOutput {
            insights: vec![ProjectInsight::new(
                self.id(),
                "The latest verification does not apply to the current project state.",
                IntelligenceConfidence::Certain,
                evidence.clone(),
                vec![verification.entity.clone()],
            )?],
            risks: vec![ProjectRisk::new(
                "The latest verification is stale for the current project state.",
                evidence,
            )?],
        })
    }
}

struct OperatorDecisionAvailability;
impl InsightRule for OperatorDecisionAvailability {
    fn id(&self) -> RuleId {
        RuleId::new(RuleId::PI_005)
    }

    fn evaluate(&self, _context: &RuleContext) -> Result<RuleOutput, IntelligenceError> {
        Ok(RuleOutput::default())
    }
}

struct ModularPackageStructure;
impl InsightRule for ModularPackageStructure {
    fn id(&self) -> RuleId {
        RuleId::new(RuleId::PI_006)
    }

    fn evaluate(&self, context: &RuleContext) -> Result<RuleOutput, IntelligenceError> {
        if context.package_entities.len() < 2 || !context.package_structure_evidence {
            return Ok(RuleOutput::default());
        }
        Ok(RuleOutput {
            insights: vec![ProjectInsight::new(
                self.id(),
                "The project is divided into multiple workspace packages.",
                IntelligenceConfidence::Certain,
                vec![IntelligenceEvidenceRef::context("workspace.packages")],
                context.package_entities.clone(),
            )?],
            risks: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(value: &str) -> EntityId {
        EntityId::derive("test", &[value])
    }

    #[test]
    fn stale_rule_emits_a_risk_without_a_recommendation() {
        let rule = StaleVerification;
        let output = rule
            .evaluate(&RuleContext {
                workspace_detected: false,
                package_entities: Vec::new(),
                workspace_entity: None,
                discovered_actions: BTreeSet::new(),
                controlled_actions: BTreeSet::new(),
                check_action_entity: None,
                check_controlled_execution_capability_entity: None,
                restricted_actions: Vec::new(),
                verification: Some(VerificationRuleInput {
                    claim_id: "k1-demo".to_string(),
                    validity: VerificationValidity::Stale,
                    entity: entity("verification"),
                }),
                package_structure_evidence: false,
            })
            .expect("rule output");
        assert_eq!(output.insights.len(), 1);
        assert_eq!(output.risks.len(), 1);
        assert!(!output.insights[0].statement.contains("run"));
    }
}
