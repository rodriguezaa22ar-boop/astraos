use crate::{authority_targets, IntelligenceError, ProjectIntelligence};
use astra_knowledge::{
    AnnotationScope, OperatorConfidence, OperatorId, OperatorIntent, OperatorResponse,
    OperatorResponseId, OperatorResponsePayload, OperatorTargetBinding, ResponseLifecycle,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub const RESOLVED_PROJECT_INTELLIGENCE_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    Resolved,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InterpretationStatus {
    Base,
    Accepted,
    Rejected,
    Corrected,
    Disputed,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionConflictKind {
    MultipleGoverningResponses,
    Disputed,
    ReviewRequired,
    Orphaned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolutionConflict {
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    pub kind: ResolutionConflictKind,
    pub response_ids: Vec<OperatorResponseId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedInterpretation {
    pub target_id: String,
    pub rule_id: String,
    pub base_statement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_statement: Option<String>,
    pub status: InterpretationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub governing_response: Option<OperatorResponseId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedAnnotation {
    pub response_id: OperatorResponseId,
    pub target_id: String,
    pub statement: String,
    pub intent: OperatorIntent,
    pub scope: AnnotationScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<OperatorConfidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolutionExplanation {
    pub response_id: OperatorResponseId,
    pub response_type: String,
    pub operator_id: OperatorId,
    pub operator_display_name: String,
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    pub base_interpretation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_interpretation: Option<String>,
    pub lifecycle: ResponseLifecycle,
    pub evidence_fingerprint: String,
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedProjectIntelligence {
    pub schema_version: u32,
    pub resolution_status: ResolutionStatus,
    pub active_response_count: usize,
    pub conflicts: Vec<ResolutionConflict>,
    pub base_intelligence: ProjectIntelligence,
    pub resolved_interpretations: Vec<ResolvedInterpretation>,
    pub annotations: Vec<ResolvedAnnotation>,
    pub explanations: Vec<ResolutionExplanation>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct OperatorAuthorityResolver;

impl OperatorAuthorityResolver {
    pub fn resolve(
        &self,
        base: &ProjectIntelligence,
        responses: &[OperatorResponse],
        include_explanations: bool,
    ) -> Result<ResolvedProjectIntelligence, IntelligenceError> {
        for response in responses {
            if response.project != base.project.name {
                return Err(IntelligenceError::InvalidInput(format!(
                    "operator response {} belongs to another project",
                    response.id
                )));
            }
        }
        let targets = authority_targets(base)?;
        let targets_by_id = targets
            .iter()
            .map(|target| (target.target_id.clone(), target))
            .collect::<BTreeMap<_, _>>();
        let targets_by_rule = targets
            .iter()
            .filter_map(|target| target.rule_id.as_ref().map(|rule| (rule.clone(), target)))
            .collect::<BTreeMap<_, _>>();

        let superseded = responses
            .iter()
            .filter_map(|response| response.supersedes.clone())
            .collect::<BTreeSet<_>>();
        let mut projected = responses
            .iter()
            .map(|response| {
                let lifecycle = if superseded.contains(&response.id)
                    && response.lifecycle == ResponseLifecycle::Active
                {
                    ResponseLifecycle::Superseded
                } else {
                    projected_lifecycle(response, &targets_by_id, &targets_by_rule)
                };
                ProjectedResponse {
                    response,
                    lifecycle,
                }
            })
            .collect::<Vec<_>>();
        projected.sort_by(|left, right| left.response.id.cmp(&right.response.id));

        let active = projected
            .iter()
            .filter(|response| response.lifecycle == ResponseLifecycle::Active)
            .collect::<Vec<_>>();
        let active_response_count = active.len();
        let mut governing = BTreeMap::<String, Vec<&ProjectedResponse>>::new();
        for response in &active {
            if response.response.payload.is_governing() {
                governing
                    .entry(response.response.target.governing_key().to_string())
                    .or_default()
                    .push(response);
            }
        }

        let mut conflicts = lifecycle_conflicts(&projected);
        let mut resolved_interpretations = Vec::new();
        for insight in &base.insights {
            let target = crate::insight_target_binding(insight)?;
            let governing_responses = governing
                .get(target.governing_key())
                .cloned()
                .unwrap_or_default();
            let mut interpretation = ResolvedInterpretation {
                target_id: target.target_id.clone(),
                rule_id: insight.rule_id.to_string(),
                base_statement: insight.statement.clone(),
                resolved_statement: Some(insight.statement.clone()),
                status: InterpretationStatus::Base,
                governing_response: None,
            };
            match governing_responses.as_slice() {
                [] => {}
                [governing] => {
                    interpretation.governing_response = Some(governing.response.id.clone());
                    apply_governing_response(&mut interpretation, governing.response);
                    if interpretation.status == InterpretationStatus::Disputed {
                        conflicts.push(ResolutionConflict {
                            target_id: target.target_id,
                            rule_id: target.rule_id,
                            kind: ResolutionConflictKind::Disputed,
                            response_ids: vec![governing.response.id.clone()],
                        });
                    }
                }
                responses => {
                    interpretation.status = InterpretationStatus::Conflict;
                    interpretation.resolved_statement = None;
                    conflicts.push(ResolutionConflict {
                        target_id: target.target_id,
                        rule_id: target.rule_id,
                        kind: ResolutionConflictKind::MultipleGoverningResponses,
                        response_ids: responses
                            .iter()
                            .map(|response| response.response.id.clone())
                            .collect(),
                    });
                }
            }
            resolved_interpretations.push(interpretation);
        }

        let mut annotations = active
            .iter()
            .filter_map(|projected| match &projected.response.payload {
                OperatorResponsePayload::Annotation(annotation) => Some(ResolvedAnnotation {
                    response_id: projected.response.id.clone(),
                    target_id: projected.response.target.target_id.clone(),
                    statement: annotation.statement.clone(),
                    intent: annotation.intent,
                    scope: annotation.scope,
                    confidence: annotation.confidence,
                }),
                _ => None,
            })
            .collect::<Vec<_>>();
        annotations.sort_by(|left, right| {
            left.target_id
                .cmp(&right.target_id)
                .then_with(|| left.response_id.cmp(&right.response_id))
        });

        conflicts.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then_with(|| left.rule_id.cmp(&right.rule_id))
                .then_with(|| left.target_id.cmp(&right.target_id))
                .then_with(|| left.response_ids.cmp(&right.response_ids))
        });
        conflicts.dedup();
        resolved_interpretations.sort_by(|left, right| {
            left.rule_id
                .cmp(&right.rule_id)
                .then_with(|| left.target_id.cmp(&right.target_id))
        });

        let explanations = if include_explanations {
            projected
                .iter()
                .map(explanation)
                .collect::<Vec<ResolutionExplanation>>()
        } else {
            Vec::new()
        };
        let resolution_status = if conflicts.is_empty() {
            ResolutionStatus::Resolved
        } else {
            ResolutionStatus::Unresolved
        };
        Ok(ResolvedProjectIntelligence {
            schema_version: RESOLVED_PROJECT_INTELLIGENCE_SCHEMA_VERSION,
            resolution_status,
            active_response_count,
            conflicts,
            base_intelligence: base.clone(),
            resolved_interpretations,
            annotations,
            explanations,
        })
    }
}

struct ProjectedResponse<'a> {
    response: &'a OperatorResponse,
    lifecycle: ResponseLifecycle,
}

fn projected_lifecycle(
    response: &OperatorResponse,
    targets_by_id: &BTreeMap<String, &OperatorTargetBinding>,
    targets_by_rule: &BTreeMap<String, &OperatorTargetBinding>,
) -> ResponseLifecycle {
    if response.lifecycle != ResponseLifecycle::Active {
        return response.lifecycle;
    }
    if let Some(current) = targets_by_id.get(&response.target.target_id) {
        if response.target.exact_match(current) {
            return ResponseLifecycle::Active;
        }
        return drift_lifecycle(response, true);
    }
    if response
        .target
        .rule_id
        .as_ref()
        .is_some_and(|rule| targets_by_rule.contains_key(rule))
    {
        return drift_lifecycle(response, false);
    }
    ResponseLifecycle::Orphaned
}

fn drift_lifecycle(response: &OperatorResponse, exact_target_exists: bool) -> ResponseLifecycle {
    match &response.payload {
        OperatorResponsePayload::Acceptance(_) => ResponseLifecycle::Expired,
        OperatorResponsePayload::Annotation(annotation) => match annotation.scope {
            AnnotationScope::StateBound => ResponseLifecycle::Expired,
            AnnotationScope::Persistent if exact_target_exists => ResponseLifecycle::Active,
            AnnotationScope::Persistent => ResponseLifecycle::Orphaned,
        },
        OperatorResponsePayload::Rejection(_)
        | OperatorResponsePayload::Correction(_)
        | OperatorResponsePayload::Dispute(_) => ResponseLifecycle::ReviewRequired,
    }
}

fn lifecycle_conflicts(projected: &[ProjectedResponse<'_>]) -> Vec<ResolutionConflict> {
    projected
        .iter()
        .filter(|projected| projected.response.payload.is_governing())
        .filter_map(|projected| {
            let kind = match projected.lifecycle {
                ResponseLifecycle::ReviewRequired => ResolutionConflictKind::ReviewRequired,
                ResponseLifecycle::Orphaned => ResolutionConflictKind::Orphaned,
                _ => return None,
            };
            Some(ResolutionConflict {
                target_id: projected.response.target.target_id.clone(),
                rule_id: projected.response.target.rule_id.clone(),
                kind,
                response_ids: vec![projected.response.id.clone()],
            })
        })
        .collect()
}

fn apply_governing_response(
    interpretation: &mut ResolvedInterpretation,
    response: &OperatorResponse,
) {
    match &response.payload {
        OperatorResponsePayload::Acceptance(_) => {
            interpretation.status = InterpretationStatus::Accepted;
        }
        OperatorResponsePayload::Rejection(_) => {
            interpretation.status = InterpretationStatus::Rejected;
            interpretation.resolved_statement = None;
        }
        OperatorResponsePayload::Correction(correction) => {
            interpretation.status = InterpretationStatus::Corrected;
            interpretation.resolved_statement = Some(correction.replacement_statement.clone());
        }
        OperatorResponsePayload::Dispute(_) => {
            interpretation.status = InterpretationStatus::Disputed;
            interpretation.resolved_statement = None;
        }
        OperatorResponsePayload::Annotation(_) => {}
    }
}

fn explanation(projected: &ProjectedResponse<'_>) -> ResolutionExplanation {
    let resolved_interpretation = match &projected.response.payload {
        OperatorResponsePayload::Correction(correction) => {
            Some(correction.replacement_statement.clone())
        }
        OperatorResponsePayload::Acceptance(_) => Some(projected.response.target.statement.clone()),
        OperatorResponsePayload::Annotation(annotation) => Some(annotation.statement.clone()),
        OperatorResponsePayload::Rejection(_) | OperatorResponsePayload::Dispute(_) => None,
    };
    ResolutionExplanation {
        response_id: projected.response.id.clone(),
        response_type: projected.response.payload.kind().to_string(),
        operator_id: projected.response.operator.id.clone(),
        operator_display_name: projected.response.operator.display_name.clone(),
        target_id: projected.response.target.target_id.clone(),
        rule_id: projected.response.target.rule_id.clone(),
        base_interpretation: projected.response.target.statement.clone(),
        resolved_interpretation,
        lifecycle: projected.lifecycle,
        evidence_fingerprint: projected.response.target.evidence_fingerprint.clone(),
        evidence_references: projected.response.target.evidence_references.clone(),
    }
}

pub fn render_resolved_text(report: &ResolvedProjectIntelligence, explain: bool) -> String {
    let mut output = crate::render_text(&report.base_intelligence);
    output.push_str("\nOperator Resolution\n");
    output.push_str(&format!(
        "  Status: {}\n",
        match report.resolution_status {
            ResolutionStatus::Resolved => "resolved",
            ResolutionStatus::Unresolved => "unresolved",
        }
    ));
    output.push_str(&format!(
        "  Active responses: {}\n",
        report.active_response_count
    ));
    if report.annotations.is_empty() {
        output.push_str("  Annotations: none\n");
    } else {
        output.push_str("  Annotations:\n");
        for annotation in &report.annotations {
            output.push_str(&format!(
                "    - [{}] {}\n",
                annotation.response_id, annotation.statement
            ));
        }
    }
    output.push_str("\nResolved Interpretations\n");
    for interpretation in &report.resolved_interpretations {
        output.push_str(&format!(
            "  - [{}] {}",
            interpretation.rule_id,
            interpretation
                .resolved_statement
                .as_deref()
                .unwrap_or("<no active interpretation>")
        ));
        output.push_str(&format!(" ({:?})\n", interpretation.status).to_ascii_lowercase());
    }
    if !report.conflicts.is_empty() {
        output.push_str("\nAuthority Conflicts\n");
        for conflict in &report.conflicts {
            output.push_str(&format!(
                "  - {}: {:?}\n",
                conflict.rule_id.as_deref().unwrap_or(&conflict.target_id),
                conflict.kind
            ));
        }
    }
    if explain && !report.explanations.is_empty() {
        output.push_str("\nAuthority Explanations\n");
        for explanation in &report.explanations {
            output.push_str(&format!(
                "  - {} {} -> {} ({})\n",
                explanation.response_id,
                explanation.base_interpretation,
                explanation
                    .resolved_interpretation
                    .as_deref()
                    .unwrap_or("<none>"),
                explanation.lifecycle.as_str()
            ));
            output.push_str(&format!(
                "    evidence: {}\n",
                explanation.evidence_fingerprint
            ));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ArchitectureModel, Availability, CapabilityModel, IdentityModel, InformationClassification,
        IntelligenceConfidence, IntelligenceEvidenceRef, KnowledgeSummary, ProjectIdentityModel,
        ProjectInsight, ProjectIntelligence, RepositoryStatus, RuleId, VerificationModel,
    };
    use astra_knowledge::{
        AcceptancePayload, AnnotationPayload, CorrectionPayload, DisputePayload, OperatorIdentity,
        OperatorTransactionId, ResponseAuditMetadata, OPERATOR_RESPONSE_SCHEMA_VERSION,
    };

    fn insight(evidence: &str, related_entities: Vec<crate::EntityId>) -> ProjectInsight {
        ProjectInsight::new(
            RuleId::new(RuleId::PI_006),
            "The project is divided into multiple workspace packages.",
            IntelligenceConfidence::Certain,
            vec![IntelligenceEvidenceRef::context(evidence)],
            related_entities,
        )
        .expect("insight")
    }

    fn base_with_insight(insight: ProjectInsight) -> ProjectIntelligence {
        ProjectIntelligence {
            schema_version: crate::PROJECT_INTELLIGENCE_SCHEMA_VERSION,
            project: ProjectIdentityModel {
                name: "demo".to_string(),
            },
            identity: IdentityModel {
                project_type: Availability::available("Rust".to_string()),
                languages: vec!["rust".to_string()],
                build_systems: vec!["cargo".to_string()],
                workspace: Availability::available("cargo".to_string()),
                package_count: 2,
            },
            architecture: ArchitectureModel {
                workspace_detected: Availability::available(true),
                package_structure: Availability::available(
                    "workspace packages detected".to_string(),
                ),
            },
            capabilities: CapabilityModel {
                discovered_actions: Vec::new(),
                controlled_execution_actions: Vec::new(),
                dry_run_only_actions: Vec::new(),
                unsupported_actions: Vec::new(),
            },
            verification: VerificationModel {
                availability: Availability::Unavailable,
                latest_action: Availability::Unavailable,
                verdict: Availability::Unavailable,
                validity: Availability::Unavailable,
            },
            knowledge: KnowledgeSummary::default(),
            repository: RepositoryStatus {
                state: Availability::Unknown,
                clean: Availability::Unknown,
                continuous_integration: Availability::Unavailable,
            },
            entities: Vec::new(),
            relationships: Vec::new(),
            insights: vec![insight],
            risks: Vec::new(),
            limitations: Vec::new(),
        }
    }

    fn response(
        sequence: u64,
        target: OperatorTargetBinding,
        payload: OperatorResponsePayload,
    ) -> OperatorResponse {
        let suffix = format!("{sequence:06}");
        OperatorResponse {
            schema_version: OPERATOR_RESPONSE_SCHEMA_VERSION,
            id: OperatorResponseId::parse(format!("or-response-v1-{suffix}")).expect("response ID"),
            project: "demo".to_string(),
            target,
            operator: OperatorIdentity::local("Local operator").expect("operator"),
            lifecycle: ResponseLifecycle::Active,
            payload,
            supersedes: None,
            audit: ResponseAuditMetadata {
                transaction_id: OperatorTransactionId::parse(format!("or-transaction-v1-{suffix}"))
                    .expect("transaction ID"),
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
        }
    }

    #[test]
    fn correction_changes_only_resolved_interpretation_and_explains_provenance() {
        let base = base_with_insight(insight("workspace.packages", Vec::new()));
        let target = crate::insight_target_binding(&base.insights[0]).expect("target");
        let correction = response(
            1,
            target,
            OperatorResponsePayload::Correction(CorrectionPayload {
                replacement_statement:
                    "Packages are implementation boundaries within one integrated system."
                        .to_string(),
                reason: Some("Architecture intent differs.".to_string()),
                intent: OperatorIntent::Architecture,
                confidence: Some(OperatorConfidence::High),
            }),
        );
        let resolved = OperatorAuthorityResolver
            .resolve(&base, &[correction], true)
            .expect("resolved");

        assert_eq!(resolved.base_intelligence, base);
        assert_eq!(resolved.resolution_status, ResolutionStatus::Resolved);
        assert_eq!(
            resolved.resolved_interpretations[0].status,
            InterpretationStatus::Corrected
        );
        assert_eq!(
            resolved.resolved_interpretations[0]
                .resolved_statement
                .as_deref(),
            Some("Packages are implementation boundaries within one integrated system.")
        );
        assert_eq!(resolved.explanations.len(), 1);
        assert_eq!(
            resolved.explanations[0].lifecycle,
            ResponseLifecycle::Active
        );
    }

    #[test]
    fn acceptance_expires_on_evidence_change_without_changing_base() {
        let original = base_with_insight(insight("workspace.packages", Vec::new()));
        let acceptance = response(
            1,
            crate::insight_target_binding(&original.insights[0]).expect("target"),
            OperatorResponsePayload::Acceptance(AcceptancePayload {
                reason: None,
                confidence: Some(OperatorConfidence::Certain),
            }),
        );
        let changed = base_with_insight(insight("workspace.packages.changed", Vec::new()));
        let resolved = OperatorAuthorityResolver
            .resolve(&changed, &[acceptance], true)
            .expect("resolved");

        assert_eq!(resolved.base_intelligence, changed);
        assert_eq!(resolved.active_response_count, 0);
        assert_eq!(
            resolved.resolved_interpretations[0].status,
            InterpretationStatus::Base
        );
        assert_eq!(
            resolved.explanations[0].lifecycle,
            ResponseLifecycle::Expired
        );
    }

    #[test]
    fn state_bound_annotations_expire_while_persistent_annotations_remain_on_target() {
        let related = crate::EntityId::derive("package", &["one"]);
        let original = base_with_insight(insight("workspace.packages", Vec::new()));
        let target = crate::insight_target_binding(&original.insights[0]).expect("target");
        let persistent = response(
            1,
            target.clone(),
            OperatorResponsePayload::Annotation(AnnotationPayload {
                statement: "Persistent context.".to_string(),
                intent: OperatorIntent::Context,
                scope: AnnotationScope::Persistent,
                confidence: None,
            }),
        );
        let state_bound = response(
            2,
            target,
            OperatorResponsePayload::Annotation(AnnotationPayload {
                statement: "State-specific context.".to_string(),
                intent: OperatorIntent::TemporaryConstraint,
                scope: AnnotationScope::StateBound,
                confidence: None,
            }),
        );
        let changed = base_with_insight(insight("workspace.packages", vec![related]));
        let resolved = OperatorAuthorityResolver
            .resolve(&changed, &[state_bound, persistent], true)
            .expect("resolved");

        assert_eq!(resolved.active_response_count, 1);
        assert_eq!(resolved.annotations.len(), 1);
        assert_eq!(resolved.annotations[0].statement, "Persistent context.");
        assert_eq!(
            resolved
                .explanations
                .iter()
                .find(|explanation| explanation.response_id.as_str().ends_with("000002"))
                .expect("state-bound explanation")
                .lifecycle,
            ResponseLifecycle::Expired
        );
    }

    #[test]
    fn dispute_is_unresolved_and_output_is_deterministic_without_audit_timestamps() {
        let base = base_with_insight(insight("workspace.packages", Vec::new()));
        let target = crate::insight_target_binding(&base.insights[0]).expect("target");
        let dispute = response(
            2,
            target.clone(),
            OperatorResponsePayload::Dispute(DisputePayload {
                reason: "More evidence is required.".to_string(),
                intent: Some(OperatorIntent::Context),
                confidence: Some(OperatorConfidence::Tentative),
            }),
        );
        let annotation = response(
            1,
            target,
            OperatorResponsePayload::Annotation(AnnotationPayload {
                statement: "Operator context.".to_string(),
                intent: OperatorIntent::Context,
                scope: AnnotationScope::Persistent,
                confidence: None,
            }),
        );
        let first = OperatorAuthorityResolver
            .resolve(&base, &[dispute.clone(), annotation.clone()], true)
            .expect("resolved");
        let second = OperatorAuthorityResolver
            .resolve(&base, &[annotation, dispute], true)
            .expect("resolved");
        assert_eq!(first, second);
        assert_eq!(first.resolution_status, ResolutionStatus::Unresolved);
        assert_eq!(
            first.resolved_interpretations[0].status,
            InterpretationStatus::Disputed
        );
        let json = serde_json::to_string(&first).expect("JSON");
        assert!(!json.contains("2026-01-01"));
        assert!(!json.contains("/Users/"));
    }

    #[test]
    fn observed_entities_can_be_annotated_but_are_never_reinterpreted() {
        let mut base = base_with_insight(insight("workspace.packages", Vec::new()));
        let entity = crate::ProjectEntity::new(
            crate::EntityKind::BuildSystem,
            "cargo",
            "cargo",
            InformationClassification::Observed,
            IntelligenceConfidence::Certain,
            vec![IntelligenceEvidenceRef::context("tooling.build_systems")],
        )
        .expect("entity");
        let target = crate::entity_target_binding(&entity).expect("target");
        base.entities.push(entity);
        let annotation = response(
            1,
            target,
            OperatorResponsePayload::Annotation(AnnotationPayload {
                statement: "Cargo is governed by workspace policy.".to_string(),
                intent: OperatorIntent::Context,
                scope: AnnotationScope::Persistent,
                confidence: None,
            }),
        );
        let resolved = OperatorAuthorityResolver
            .resolve(&base, &[annotation], false)
            .expect("resolved");
        assert_eq!(resolved.annotations.len(), 1);
        assert_eq!(resolved.resolved_interpretations.len(), 1);
        assert_eq!(
            resolved.resolved_interpretations[0].status,
            InterpretationStatus::Base
        );
    }
}
