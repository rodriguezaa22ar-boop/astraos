use crate::rule::{default_rules, RestrictedActionRuleInput, RuleContext, VerificationRuleInput};
use crate::{
    ArchitectureModel, Availability, CapabilityModel, EntityId, EntityKind, IdentityModel,
    InformationClassification, IntelligenceConfidence, IntelligenceError, IntelligenceEvidenceRef,
    KnowledgeCategoryInput, KnowledgeSummary, ProjectContextInput, ProjectEntity,
    ProjectIdentityModel, ProjectInsight, ProjectIntelligence, ProjectIntelligenceInput,
    ProjectLimitation, ProjectRelationship, ProjectRelationshipKind::*, ProjectRisk,
    RepositoryStatus, VerificationModel, PROJECT_INTELLIGENCE_SCHEMA_VERSION,
};
use std::collections::{BTreeMap, BTreeSet};

/// Pure deterministic analyzer for the Project Intelligence model.
pub trait ProjectIntelligenceAnalyzer {
    fn analyze(
        &self,
        input: &ProjectIntelligenceInput,
    ) -> Result<ProjectIntelligence, IntelligenceError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DeterministicProjectIntelligenceAnalyzer;

impl ProjectIntelligenceAnalyzer for DeterministicProjectIntelligenceAnalyzer {
    fn analyze(
        &self,
        input: &ProjectIntelligenceInput,
    ) -> Result<ProjectIntelligence, IntelligenceError> {
        validate_input(input)?;
        let mut graph = GraphBuilder::default();
        let project = graph.entity(ProjectEntity::new(
            EntityKind::Project,
            &input.project.name,
            &input.project.name,
            InformationClassification::Observed,
            IntelligenceConfidence::Certain,
            vec![IntelligenceEvidenceRef::Input {
                field: "project.name".to_string(),
            }],
        )?)?;

        let workspace = build_workspace(&mut graph, input, &project)?;
        let package_entities = build_packages(&mut graph, &input.context, workspace.as_ref())?;
        build_languages(&mut graph, &input.context, &project)?;
        build_build_systems(&mut graph, &input.context, &project)?;
        build_ci(&mut graph, &input.context, &project)?;
        let action_entities = build_actions_and_capabilities(&mut graph, input, &project)?;
        let knowledge = build_knowledge(&mut graph, input, &project)?;

        let workspace_detected = workspace.is_some();
        let package_structure_evidence = workspace_detected && package_entities.len() > 1;
        let rule_context = RuleContext {
            workspace_detected,
            package_entities: package_entities.clone(),
            workspace_entity: workspace,
            discovered_actions: input
                .actions
                .iter()
                .map(|action| action.id.clone())
                .collect(),
            controlled_actions: sorted_set(&input.execution_capabilities.controlled_actions),
            check_action_entity: action_entities.actions.get("check").cloned(),
            check_controlled_execution_capability_entity: action_entities
                .controlled_execution_capabilities
                .get("check")
                .cloned(),
            restricted_actions: sorted_set(&input.execution_capabilities.dry_run_only_actions)
                .into_iter()
                .map(|action_id| RestrictedActionRuleInput {
                    action_entity: action_entities.actions.get(&action_id).cloned(),
                    dry_run_only_capability_entity: action_entities
                        .dry_run_only_capabilities
                        .get(&action_id)
                        .cloned(),
                    action_id,
                })
                .collect(),
            verification: knowledge.latest_verification.clone(),
            package_structure_evidence,
        };

        let mut insights = Vec::new();
        let mut risks = Vec::new();
        for rule in default_rules() {
            let output = rule.evaluate(&rule_context)?;
            insights.extend(output.insights);
            risks.extend(output.risks);
        }
        let limitations = limitations(&knowledge);

        let mut entities = graph.entities.into_values().collect::<Vec<_>>();
        entities.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });
        let entity_ids = entities
            .iter()
            .map(|entity| entity.id.clone())
            .collect::<BTreeSet<_>>();
        let mut relationships = graph.relationships.into_values().collect::<Vec<_>>();
        relationships.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then_with(|| left.source.cmp(&right.source))
                .then_with(|| left.target.cmp(&right.target))
                .then_with(|| left.id.cmp(&right.id))
        });
        for relationship in &relationships {
            if !entity_ids.contains(&relationship.source) {
                return Err(IntelligenceError::MissingRelationshipEndpoint(
                    relationship.source.to_string(),
                ));
            }
            if !entity_ids.contains(&relationship.target) {
                return Err(IntelligenceError::MissingRelationshipEndpoint(
                    relationship.target.to_string(),
                ));
            }
        }
        sort_insights(&mut insights);
        sort_risks(&mut risks);
        let mut limitations = limitations;
        limitations.sort_by(|left, right| {
            left.statement
                .cmp(&right.statement)
                .then_with(|| left.id.cmp(&right.id))
        });

        Ok(ProjectIntelligence {
            schema_version: PROJECT_INTELLIGENCE_SCHEMA_VERSION,
            project: ProjectIdentityModel {
                name: input.project.name.clone(),
            },
            identity: identity_model(input),
            architecture: architecture_model(workspace_detected, package_structure_evidence),
            capabilities: capability_model(input),
            verification: verification_model(&knowledge),
            knowledge: knowledge.summary,
            repository: repository_model(&input.context),
            entities,
            relationships,
            insights,
            risks,
            limitations,
        })
    }
}

#[derive(Default)]
struct GraphBuilder {
    entities: BTreeMap<EntityId, ProjectEntity>,
    relationships: BTreeMap<crate::RelationshipId, ProjectRelationship>,
}

impl GraphBuilder {
    fn entity(&mut self, entity: ProjectEntity) -> Result<EntityId, IntelligenceError> {
        let id = entity.id.clone();
        if let Some(existing) = self.entities.get_mut(&id) {
            existing.merge_evidence(&entity)?;
        } else {
            self.entities.insert(id.clone(), entity);
        }
        Ok(id)
    }

    fn relationship(&mut self, relationship: ProjectRelationship) -> Result<(), IntelligenceError> {
        if let Some(existing) = self.relationships.get_mut(&relationship.id) {
            existing.merge_evidence(&relationship)
        } else {
            self.relationships
                .insert(relationship.id.clone(), relationship);
            Ok(())
        }
    }
}

fn validate_input(input: &ProjectIntelligenceInput) -> Result<(), IntelligenceError> {
    if input.project.name.trim().is_empty() {
        return Err(IntelligenceError::InvalidInput(
            "project name is required".to_string(),
        ));
    }
    let action_ids = input
        .actions
        .iter()
        .map(|action| action.id.as_str())
        .collect::<BTreeSet<_>>();
    if input
        .actions
        .iter()
        .any(|action| action.id.is_empty() || action.evidence.is_empty())
    {
        return Err(IntelligenceError::InvalidInput(
            "actions require an identifier and evidence".to_string(),
        ));
    }
    if input
        .knowledge
        .claims
        .iter()
        .any(|claim| claim.id.is_empty() || claim.predicate.is_empty())
    {
        return Err(IntelligenceError::InvalidInput(
            "knowledge claims require an identifier and predicate".to_string(),
        ));
    }
    for action in &input.execution_capabilities.discovered_actions {
        if !action_ids.contains(action.as_str()) {
            return Err(IntelligenceError::InconsistentCapability(format!(
                "execution capability references undiscovered action {action}"
            )));
        }
    }
    let discovered = sorted_set(&input.execution_capabilities.discovered_actions);
    for action in input
        .execution_capabilities
        .controlled_actions
        .iter()
        .chain(input.execution_capabilities.dry_run_only_actions.iter())
        .chain(input.execution_capabilities.unsupported_actions.iter())
    {
        if !discovered.contains(action) {
            return Err(IntelligenceError::InconsistentCapability(format!(
                "capability action is not discovered: {action}"
            )));
        }
    }
    let controlled = sorted_set(&input.execution_capabilities.controlled_actions);
    let dry_run = sorted_set(&input.execution_capabilities.dry_run_only_actions);
    if !controlled.is_disjoint(&dry_run) {
        return Err(IntelligenceError::InconsistentCapability(
            "an action cannot be controlled and dry-run-only".to_string(),
        ));
    }
    Ok(())
}

fn build_workspace(
    graph: &mut GraphBuilder,
    input: &ProjectIntelligenceInput,
    project: &EntityId,
) -> Result<Option<EntityId>, IntelligenceError> {
    let kinds = sorted_set(&input.context.workspace_kinds);
    let Some(kind) = kinds.first() else {
        return Ok(None);
    };
    let workspace = graph.entity(ProjectEntity::new(
        EntityKind::Workspace,
        format!("{}:{kind}", input.project.name),
        kind.clone(),
        InformationClassification::Observed,
        IntelligenceConfidence::Certain,
        vec![IntelligenceEvidenceRef::context("workspace.kinds")],
    )?)?;
    graph.relationship(ProjectRelationship::new(
        Contains,
        project.clone(),
        workspace.clone(),
        InformationClassification::Observed,
        IntelligenceConfidence::Certain,
        vec![IntelligenceEvidenceRef::context("workspace.kinds")],
    )?)?;
    Ok(Some(workspace))
}

fn build_packages(
    graph: &mut GraphBuilder,
    context: &ProjectContextInput,
    workspace: Option<&EntityId>,
) -> Result<Vec<EntityId>, IntelligenceError> {
    let mut packages = context.packages.clone();
    packages.sort_by(|left, right| {
        (&left.ecosystem, &left.name, &left.relative_path).cmp(&(
            &right.ecosystem,
            &right.name,
            &right.relative_path,
        ))
    });
    let mut ids = Vec::new();
    for package in packages {
        let evidence = if package.evidence.is_empty() {
            vec![IntelligenceEvidenceRef::ContextPackage {
                package: package.name.clone(),
            }]
        } else {
            package.evidence
        };
        let id = graph.entity(ProjectEntity::new(
            EntityKind::Package,
            format!(
                "{}:{}:{}",
                package.ecosystem, package.name, package.relative_path
            ),
            package.name,
            InformationClassification::Observed,
            package.confidence,
            evidence.clone(),
        )?)?;
        if let Some(workspace) = workspace {
            graph.relationship(ProjectRelationship::new(
                Contains,
                workspace.clone(),
                id.clone(),
                InformationClassification::Observed,
                package.confidence,
                evidence,
            )?)?;
        }
        ids.push(id);
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn build_languages(
    graph: &mut GraphBuilder,
    context: &ProjectContextInput,
    project: &EntityId,
) -> Result<(), IntelligenceError> {
    for language in sorted_set(&context.languages) {
        let evidence = vec![IntelligenceEvidenceRef::context(format!(
            "languages.{language}"
        ))];
        let entity = graph.entity(ProjectEntity::new(
            EntityKind::Language,
            &language,
            language.clone(),
            InformationClassification::Observed,
            IntelligenceConfidence::High,
            evidence.clone(),
        )?)?;
        graph.relationship(ProjectRelationship::new(
            UsesLanguage,
            project.clone(),
            entity,
            InformationClassification::Observed,
            IntelligenceConfidence::High,
            evidence,
        )?)?;
    }
    Ok(())
}

fn build_build_systems(
    graph: &mut GraphBuilder,
    context: &ProjectContextInput,
    project: &EntityId,
) -> Result<(), IntelligenceError> {
    for build_system in sorted_set(&context.build_systems) {
        let evidence = vec![IntelligenceEvidenceRef::context(format!(
            "tooling.build_systems.{build_system}"
        ))];
        let entity = graph.entity(ProjectEntity::new(
            EntityKind::BuildSystem,
            &build_system,
            build_system.clone(),
            InformationClassification::Observed,
            IntelligenceConfidence::High,
            evidence.clone(),
        )?)?;
        graph.relationship(ProjectRelationship::new(
            UsesBuildSystem,
            project.clone(),
            entity,
            InformationClassification::Observed,
            IntelligenceConfidence::High,
            evidence,
        )?)?;
    }
    Ok(())
}

fn build_ci(
    graph: &mut GraphBuilder,
    context: &ProjectContextInput,
    project: &EntityId,
) -> Result<(), IntelligenceError> {
    for provider in sorted_set(&context.continuous_integration) {
        let evidence = vec![IntelligenceEvidenceRef::context(format!("ci.{provider}"))];
        let entity = graph.entity(ProjectEntity::new(
            EntityKind::ContinuousIntegration,
            &provider,
            provider.clone(),
            InformationClassification::Observed,
            IntelligenceConfidence::High,
            evidence.clone(),
        )?)?;
        graph.relationship(ProjectRelationship::new(
            Contains,
            project.clone(),
            entity,
            InformationClassification::Observed,
            IntelligenceConfidence::High,
            evidence,
        )?)?;
    }
    Ok(())
}

#[derive(Default)]
struct ActionEntityReferences {
    actions: BTreeMap<String, EntityId>,
    controlled_execution_capabilities: BTreeMap<String, EntityId>,
    dry_run_only_capabilities: BTreeMap<String, EntityId>,
}

fn build_actions_and_capabilities(
    graph: &mut GraphBuilder,
    input: &ProjectIntelligenceInput,
    project: &EntityId,
) -> Result<ActionEntityReferences, IntelligenceError> {
    let mut references = ActionEntityReferences::default();
    let mut actions = input.actions.clone();
    actions.sort_by(|left, right| left.id.cmp(&right.id));
    for action in actions {
        let action_entity = graph.entity(ProjectEntity::new(
            EntityKind::Action,
            &action.id,
            action.id.clone(),
            InformationClassification::Observed,
            action.confidence,
            action.evidence.clone(),
        )?)?;
        graph.relationship(ProjectRelationship::new(
            ProvidesAction,
            project.clone(),
            action_entity.clone(),
            InformationClassification::Observed,
            action.confidence,
            action.evidence.clone(),
        )?)?;
        graph.relationship(ProjectRelationship::new(
            ActionValidatesProject,
            action_entity.clone(),
            project.clone(),
            InformationClassification::Observed,
            action.confidence,
            action.evidence,
        )?)?;
        references.actions.insert(action.id, action_entity);
    }
    let capabilities = &input.execution_capabilities;
    for (kind, actions) in [
        (
            CapabilityAllowedForControlledExecution,
            &capabilities.controlled_actions,
        ),
        (
            CapabilityRestrictedToDryRun,
            &capabilities.dry_run_only_actions,
        ),
    ] {
        for action in sorted_set(actions) {
            let evidence = vec![IntelligenceEvidenceRef::ExecutionCapability {
                action_id: action.clone(),
            }];
            let capability = graph.entity(ProjectEntity::new(
                EntityKind::ExecutionCapability,
                format!("{}:{action}", kind.as_str()),
                match kind {
                    CapabilityAllowedForControlledExecution => {
                        format!("controlled execution: {action}")
                    }
                    _ => format!("dry run only: {action}"),
                },
                InformationClassification::Observed,
                IntelligenceConfidence::Certain,
                evidence.clone(),
            )?)?;
            let action_entity = references.actions.get(&action).cloned().ok_or_else(|| {
                IntelligenceError::InconsistentCapability(format!(
                    "capability action is not discovered: {action}"
                ))
            })?;
            graph.relationship(ProjectRelationship::new(
                kind,
                capability.clone(),
                action_entity,
                InformationClassification::Observed,
                IntelligenceConfidence::Certain,
                evidence,
            )?)?;
            match kind {
                CapabilityAllowedForControlledExecution => {
                    references
                        .controlled_execution_capabilities
                        .insert(action, capability);
                }
                CapabilityRestrictedToDryRun => {
                    references
                        .dry_run_only_capabilities
                        .insert(action, capability);
                }
                _ => {
                    return Err(IntelligenceError::InvalidInput(
                        "unsupported execution capability kind".to_string(),
                    ));
                }
            }
        }
    }
    Ok(references)
}

#[derive(Default)]
struct KnowledgeBuild {
    summary: KnowledgeSummary,
    latest_verification: Option<VerificationRuleInput>,
    latest_action: Option<String>,
    latest_verdict: Option<String>,
    latest_validity: Option<crate::VerificationValidity>,
    decision_count: usize,
}

fn build_knowledge(
    graph: &mut GraphBuilder,
    input: &ProjectIntelligenceInput,
    project: &EntityId,
) -> Result<KnowledgeBuild, IntelligenceError> {
    let mut claims = input.knowledge.claims.clone();
    claims.sort_by(|left, right| left.id.cmp(&right.id));
    let mut build = KnowledgeBuild::default();
    let mut verification_candidates = Vec::new();
    for claim in claims {
        increment_summary(&mut build.summary, claim.category);
        let evidence = vec![IntelligenceEvidenceRef::KnowledgeClaim {
            claim_id: claim.id.clone(),
        }];
        let kind = if claim.category == KnowledgeCategoryInput::Decision {
            EntityKind::Decision
        } else if claim.category == KnowledgeCategoryInput::Verification {
            EntityKind::Verification
        } else {
            EntityKind::KnowledgeClaim
        };
        let classification = if claim.category == KnowledgeCategoryInput::Decision {
            build.decision_count += 1;
            InformationClassification::OperatorDecided
        } else {
            InformationClassification::Observed
        };
        let entity = graph.entity(ProjectEntity::new(
            kind,
            &claim.id,
            claim.predicate.clone(),
            classification,
            claim.confidence,
            evidence.clone(),
        )?)?;
        let relationship_kind = if kind == EntityKind::Decision {
            DecisionGovernsEntity
        } else if kind == EntityKind::Verification {
            VerificationAppliesToProject
        } else {
            ClaimSupportsEntity
        };
        graph.relationship(ProjectRelationship::new(
            relationship_kind,
            entity.clone(),
            project.clone(),
            classification,
            claim.confidence,
            evidence.clone(),
        )?)?;
        if claim.category == KnowledgeCategoryInput::Verification {
            verification_candidates.push((claim, entity));
        }
    }
    verification_candidates.sort_by(|(left, _), (right, _)| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    if let Some((claim, entity)) = verification_candidates.pop() {
        build.latest_action = claim.verification_action.clone();
        build.latest_verdict = claim.verification_verdict.clone();
        build.latest_validity = Some(claim.validity);
        build.latest_verification = Some(VerificationRuleInput {
            claim_id: claim.id,
            validity: claim.validity,
            entity,
        });
    }
    Ok(build)
}

fn increment_summary(summary: &mut KnowledgeSummary, category: KnowledgeCategoryInput) {
    match category {
        KnowledgeCategoryInput::Fact => summary.facts += 1,
        KnowledgeCategoryInput::Decision => summary.decisions += 1,
        KnowledgeCategoryInput::Verification => summary.verifications += 1,
        KnowledgeCategoryInput::Goal => summary.goals += 1,
    }
}

fn identity_model(input: &ProjectIntelligenceInput) -> IdentityModel {
    let workspace = sorted_set(&input.context.workspace_kinds)
        .into_iter()
        .next()
        .map(Availability::available)
        .unwrap_or(Availability::Unavailable);
    IdentityModel {
        project_type: input
            .project
            .project_type
            .clone()
            .map(Availability::available)
            .unwrap_or(Availability::Unknown),
        languages: sorted_set(&input.context.languages).into_iter().collect(),
        build_systems: sorted_set(&input.context.build_systems)
            .into_iter()
            .collect(),
        workspace,
        package_count: input.context.packages.len(),
    }
}

fn architecture_model(workspace_detected: bool, package_structure: bool) -> ArchitectureModel {
    ArchitectureModel {
        workspace_detected: if workspace_detected {
            Availability::available(true)
        } else {
            Availability::Unavailable
        },
        package_structure: if package_structure {
            Availability::available("workspace packages detected".to_string())
        } else {
            Availability::Unavailable
        },
    }
}

fn capability_model(input: &ProjectIntelligenceInput) -> CapabilityModel {
    CapabilityModel {
        discovered_actions: sorted_set(&input.execution_capabilities.discovered_actions)
            .into_iter()
            .collect(),
        controlled_execution_actions: sorted_set(&input.execution_capabilities.controlled_actions)
            .into_iter()
            .collect(),
        dry_run_only_actions: sorted_set(&input.execution_capabilities.dry_run_only_actions)
            .into_iter()
            .collect(),
        unsupported_actions: sorted_set(&input.execution_capabilities.unsupported_actions)
            .into_iter()
            .collect(),
    }
}

fn verification_model(knowledge: &KnowledgeBuild) -> VerificationModel {
    let availability = if knowledge.latest_verification.is_some() {
        Availability::available("verification_knowledge".to_string())
    } else {
        Availability::Unavailable
    };
    VerificationModel {
        availability,
        latest_action: knowledge
            .latest_action
            .clone()
            .map(Availability::available)
            .unwrap_or(Availability::Unavailable),
        verdict: knowledge
            .latest_verdict
            .clone()
            .map(Availability::available)
            .unwrap_or(Availability::Unavailable),
        validity: knowledge
            .latest_validity
            .map(Availability::available)
            .unwrap_or(Availability::Unavailable),
    }
}

fn repository_model(context: &ProjectContextInput) -> RepositoryStatus {
    RepositoryStatus {
        state: context
            .repository
            .state
            .clone()
            .map(Availability::available)
            .unwrap_or(Availability::Unknown),
        clean: context
            .repository
            .clean
            .map(Availability::available)
            .unwrap_or(Availability::Unknown),
        continuous_integration: if context.continuous_integration.is_empty() {
            Availability::Unavailable
        } else {
            Availability::available(
                sorted_set(&context.continuous_integration)
                    .into_iter()
                    .collect(),
            )
        },
    }
}

fn limitations(knowledge: &KnowledgeBuild) -> Vec<ProjectLimitation> {
    if knowledge.decision_count != 0 {
        return Vec::new();
    }
    ProjectLimitation::new(
        "No operator-owned decision claims are currently available to the intelligence model.",
        vec![IntelligenceEvidenceRef::Input {
            field: "knowledge.decisions".to_string(),
        }],
    )
    .map_or_else(|_| Vec::new(), |limitation| vec![limitation])
}

fn sorted_set(values: &[String]) -> BTreeSet<String> {
    values.iter().cloned().collect()
}

fn sort_insights(insights: &mut Vec<ProjectInsight>) {
    insights.sort_by(|left, right| {
        left.rule_id
            .cmp(&right.rule_id)
            .then_with(|| left.id.cmp(&right.id))
    });
    insights.dedup_by(|left, right| left.id == right.id);
}

fn sort_risks(risks: &mut Vec<ProjectRisk>) {
    risks.sort_by(|left, right| {
        left.statement
            .cmp(&right.statement)
            .then_with(|| left.id.cmp(&right.id))
    });
    risks.dedup_by(|left, right| left.id == right.id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActionInput, ExecutionCapabilityInput, KnowledgeCategoryInput, KnowledgeClaimInput,
        ProjectContextInput, ProjectIdentityInput, ProjectIntelligenceInput, ProjectKnowledgeInput,
        RepositoryInput, WorkspacePackageInput,
    };

    fn input(root_variant: &str) -> ProjectIntelligenceInput {
        let package = |name: &str, path: &str| WorkspacePackageInput {
            name: name.to_string(),
            ecosystem: "cargo".to_string(),
            relative_path: path.to_string(),
            confidence: IntelligenceConfidence::Certain,
            evidence: vec![IntelligenceEvidenceRef::ContextPackage {
                package: name.to_string(),
            }],
        };
        ProjectIntelligenceInput {
            project: ProjectIdentityInput {
                name: "demo".to_string(),
                project_type: Some("Rust Cargo workspace".to_string()),
            },
            context: ProjectContextInput {
                workspace_kinds: vec!["cargo_workspace".to_string()],
                packages: vec![package("core", "crates/core"), package("app", "crates/app")],
                languages: vec!["rust".to_string()],
                build_systems: vec!["cargo".to_string()],
                continuous_integration: vec!["github_actions".to_string()],
                repository: RepositoryInput {
                    state: Some("git".to_string()),
                    clean: Some(true),
                },
            },
            actions: vec![
                ActionInput {
                    id: "build".to_string(),
                    confidence: IntelligenceConfidence::High,
                    evidence: vec![IntelligenceEvidenceRef::Action {
                        action_id: "build".to_string(),
                    }],
                },
                ActionInput {
                    id: "check".to_string(),
                    confidence: IntelligenceConfidence::High,
                    evidence: vec![IntelligenceEvidenceRef::Action {
                        action_id: "check".to_string(),
                    }],
                },
                ActionInput {
                    id: "test".to_string(),
                    confidence: IntelligenceConfidence::High,
                    evidence: vec![IntelligenceEvidenceRef::Action {
                        action_id: "test".to_string(),
                    }],
                },
            ],
            execution_capabilities: ExecutionCapabilityInput {
                discovered_actions: vec![
                    "build".to_string(),
                    "check".to_string(),
                    "test".to_string(),
                ],
                controlled_actions: vec!["check".to_string()],
                dry_run_only_actions: vec!["build".to_string(), "test".to_string()],
                unsupported_actions: Vec::new(),
            },
            knowledge: ProjectKnowledgeInput {
                claims: vec![KnowledgeClaimInput {
                    id: format!("k1-{root_variant}"),
                    category: KnowledgeCategoryInput::Verification,
                    predicate: "check".to_string(),
                    confidence: IntelligenceConfidence::Certain,
                    validity: crate::VerificationValidity::Current,
                    created_at: "2026-01-01T00:00:00Z".to_string(),
                    verification_action: Some("check".to_string()),
                    verification_verdict: Some("verified_check".to_string()),
                }],
            },
        }
    }

    #[test]
    fn analyzer_is_deterministic_and_never_exposes_input_paths() {
        let analyzer = DeterministicProjectIntelligenceAnalyzer;
        let first = analyzer.analyze(&input("one")).expect("analysis");
        let mut shuffled = input("one");
        shuffled.actions.reverse();
        shuffled.context.packages.reverse();
        let second = analyzer.analyze(&shuffled).expect("analysis");
        assert_eq!(first, second);
        let json = serde_json::to_string(&first).expect("JSON");
        assert!(!json.contains("crates/core"));
        assert!(first
            .insights
            .iter()
            .any(|insight| insight.rule_id.as_str() == "PI-001"));
        let pi_002 = first
            .insights
            .iter()
            .find(|insight| insight.rule_id.as_str() == "PI-002")
            .expect("PI-002 insight");
        assert!(pi_002.related_entities.iter().any(|id| {
            first.entities.iter().any(|entity| {
                entity.id == *id && entity.kind == EntityKind::Action && entity.name == "check"
            })
        }));
        assert!(pi_002.related_entities.iter().any(|id| {
            first.entities.iter().any(|entity| {
                entity.id == *id
                    && entity.kind == EntityKind::ExecutionCapability
                    && entity.name == "controlled execution: check"
            })
        }));
        assert!(pi_002.related_entities.iter().any(|id| {
            first
                .entities
                .iter()
                .any(|entity| entity.id == *id && entity.kind == EntityKind::Verification)
        }));

        let pi_003 = first
            .insights
            .iter()
            .find(|insight| insight.rule_id.as_str() == "PI-003")
            .expect("PI-003 insight");
        assert!(!pi_003.related_entities.is_empty());
        let pi_003_entities = pi_003
            .related_entities
            .iter()
            .filter_map(|id| first.entities.iter().find(|entity| entity.id == *id))
            .collect::<Vec<_>>();
        assert_eq!(
            pi_003_entities
                .iter()
                .filter(|entity| entity.kind == EntityKind::Action)
                .count(),
            2
        );
        let pi_003_names = pi_003_entities
            .iter()
            .map(|entity| entity.name.as_str())
            .collect::<Vec<_>>();
        assert!(pi_003_names.contains(&"build"));
        assert!(pi_003_names.contains(&"test"));
        assert!(pi_003_names.contains(&"dry run only: build"));
        assert!(pi_003_names.contains(&"dry run only: test"));
        assert_eq!(
            pi_003_entities
                .iter()
                .filter(|entity| entity.kind == EntityKind::ExecutionCapability)
                .count(),
            2
        );

        let pi_006 = first
            .insights
            .iter()
            .find(|insight| insight.rule_id.as_str() == "PI-006")
            .expect("PI-006 insight");
        assert_eq!(
            pi_006.statement,
            "The project is divided into multiple workspace packages."
        );
    }

    #[test]
    fn analyzer_emits_only_derived_observations_not_recommendations() {
        let report = DeterministicProjectIntelligenceAnalyzer
            .analyze(&input("observations"))
            .expect("analysis");
        assert!(report
            .insights
            .iter()
            .all(|insight| insight.classification == InformationClassification::Derived));
        assert!(report.insights.iter().all(|insight| {
            !insight.statement.contains(" should ")
                && !insight.statement.contains("recommend")
                && !insight.statement.contains("execute")
        }));
    }

    #[test]
    fn stale_verification_creates_an_insight_and_risk() {
        let analyzer = DeterministicProjectIntelligenceAnalyzer;
        let mut input = input("stale");
        input.knowledge.claims[0].validity = crate::VerificationValidity::Stale;
        let report = analyzer.analyze(&input).expect("analysis");
        assert!(report
            .insights
            .iter()
            .any(|insight| insight.rule_id.as_str() == "PI-004"));
        assert_eq!(report.risks.len(), 1);
        assert_eq!(report.limitations.len(), 1);
    }

    #[test]
    fn inconsistent_capabilities_are_typed_errors() {
        let analyzer = DeterministicProjectIntelligenceAnalyzer;
        let mut input = input("bad");
        input.execution_capabilities.controlled_actions = vec!["missing".to_string()];
        assert!(matches!(
            analyzer.analyze(&input),
            Err(IntelligenceError::InconsistentCapability(_))
        ));
    }

    #[test]
    fn operator_decisions_remain_distinct_and_remove_only_the_limitation() {
        let analyzer = DeterministicProjectIntelligenceAnalyzer;
        let mut input = input("decision");
        input.knowledge.claims.push(KnowledgeClaimInput {
            id: "k1-decision".to_string(),
            category: KnowledgeCategoryInput::Decision,
            predicate: "execution_boundary".to_string(),
            confidence: IntelligenceConfidence::High,
            validity: crate::VerificationValidity::Current,
            created_at: "2026-01-02T00:00:00Z".to_string(),
            verification_action: None,
            verification_verdict: None,
        });
        let report = analyzer.analyze(&input).expect("analysis");
        assert!(report.limitations.is_empty());
        assert!(report.entities.iter().any(|entity| {
            entity.kind == EntityKind::Decision
                && entity.classification == InformationClassification::OperatorDecided
        }));
    }

    #[test]
    fn non_current_verifications_do_not_claim_current_state() {
        let analyzer = DeterministicProjectIntelligenceAnalyzer;
        for validity in [
            crate::VerificationValidity::Invalidated,
            crate::VerificationValidity::Unknown,
        ] {
            let mut input = input("validity");
            input.knowledge.claims[0].validity = validity;
            let report = analyzer.analyze(&input).expect("analysis");
            assert_eq!(
                report.verification.validity,
                Availability::Available(validity)
            );
            assert!(!report
                .insights
                .iter()
                .any(|insight| insight.rule_id.as_str() == "PI-004"));
        }
    }
}
