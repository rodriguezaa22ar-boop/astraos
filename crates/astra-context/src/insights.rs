use crate::{
    facts::{Fact, FactGraph, FactKind, MarkerKind, StoredFact, ToolCategory},
    scanner::detected_from_facts,
    Detected, DiagnosticSeverity, DocumentKind, Insight, ProjectContext,
};
use std::collections::BTreeMap;

pub(crate) struct InsightsEngine;

impl InsightsEngine {
    pub(crate) fn derive(context: &ProjectContext, facts: &FactGraph) -> Vec<Detected<Insight>> {
        let mut insights = Vec::new();
        let inventory_detection = inventory_detection(facts);
        let inventory_complete = has_marker(facts, MarkerKind::InventoryComplete);
        let manifests_complete = has_marker(facts, MarkerKind::ManifestComplete);

        if inventory_complete
            && !context
                .documentation
                .iter()
                .any(|document| document.value.kind == DocumentKind::Readme)
        {
            insights.push(insight(
                "documentation.readme_not_detected",
                DiagnosticSeverity::Info,
                "No README was detected within the selected project root.",
                inventory_detection.clone(),
            ));
        }

        if inventory_complete
            && manifests_complete
            && context.size.value.test_files == 0
            && context.tooling.testing_frameworks.is_empty()
        {
            let completion_detection = marker_detection(
                facts,
                &[MarkerKind::InventoryComplete, MarkerKind::ManifestComplete],
            );
            insights.push(insight(
                "testing.not_detected",
                DiagnosticSeverity::Info,
                "No test files or testing framework were detected.",
                completion_detection,
            ));
        }

        if inventory_complete && context.ci.is_empty() {
            insights.push(insight(
                "ci.not_detected",
                DiagnosticSeverity::Info,
                "No supported continuous-integration configuration was detected.",
                inventory_detection,
            ));
        }

        lockfile_insights(facts, &mut insights);
        marker_insights(facts, &mut insights);
        insights.sort_by(|left, right| left.value.code.cmp(&right.value.code));
        insights
    }
}

fn has_marker(facts: &FactGraph, expected: MarkerKind) -> bool {
    facts
        .primary_facts_of_kind(FactKind::Marker)
        .into_iter()
        .any(|stored| {
            matches!(
                &stored.fact,
                Fact::Marker(marker) if marker.kind == expected
            )
        })
}

fn inventory_detection(facts: &FactGraph) -> Detected<()> {
    marker_detection(
        facts,
        &[
            MarkerKind::InventoryComplete,
            MarkerKind::InventoryPartial,
            MarkerKind::InventoryTruncated,
        ],
    )
}

fn marker_detection(facts: &FactGraph, expected: &[MarkerKind]) -> Detected<()> {
    let observations = facts
        .primary_facts_of_kind(FactKind::Marker)
        .into_iter()
        .filter(|stored| {
            matches!(
                &stored.fact,
                Fact::Marker(marker) if expected.contains(&marker.kind)
            )
        })
        .collect::<Vec<_>>();
    detected_from_facts((), &observations)
}

fn lockfile_insights(facts: &FactGraph, insights: &mut Vec<Detected<Insight>>) {
    let mut ecosystems = BTreeMap::<&str, Vec<&StoredFact>>::new();
    for stored in facts.primary_facts_of_kind(FactKind::Tool) {
        let Fact::Tool(tool) = &stored.fact else {
            continue;
        };
        if tool.category != ToolCategory::PackageManager {
            continue;
        }
        if !is_lockfile(&tool.source_path) {
            continue;
        }
        let ecosystem = match tool.id.as_str() {
            "npm" | "pnpm" | "yarn" | "bun" => "node",
            "uv" | "poetry" => "python",
            _ => continue,
        };
        ecosystems.entry(ecosystem).or_default().push(stored);
    }

    for (ecosystem, observations) in ecosystems {
        let mut managers = observations
            .iter()
            .filter_map(|stored| match &stored.fact {
                Fact::Tool(tool) => Some(tool.id.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        managers.sort_unstable();
        managers.dedup();
        if managers.len() < 2 {
            continue;
        }
        let detection = detected_from_facts((), &observations);
        insights.push(insight(
            &format!("package_manager.{ecosystem}_lockfiles_conflict"),
            DiagnosticSeverity::Warning,
            &format!(
                "Multiple {ecosystem} lockfile families were detected: {}.",
                managers.join(", ")
            ),
            detection,
        ));
    }
}

fn is_lockfile(path: &str) -> bool {
    matches!(
        path.rsplit('/').next(),
        Some(
            "package-lock.json"
                | "pnpm-lock.yaml"
                | "yarn.lock"
                | "bun.lock"
                | "bun.lockb"
                | "uv.lock"
                | "poetry.lock"
        )
    )
}

fn marker_insights(facts: &FactGraph, insights: &mut Vec<Detected<Insight>>) {
    for stored in facts.primary_facts_of_kind(FactKind::Marker) {
        let Fact::Marker(marker) = &stored.fact else {
            continue;
        };
        match marker.kind {
            MarkerKind::InventoryPartial => insights.push(insight(
                "inventory.partial",
                DiagnosticSeverity::Warning,
                "The project inventory is partial because one or more paths could not be inspected.",
                detected_from_facts((), &[stored]),
            )),
            MarkerKind::InventoryTruncated => insights.push(insight(
                "inventory.truncated",
                DiagnosticSeverity::Warning,
                "The project inventory was truncated by a configured safety limit.",
                detected_from_facts((), &[stored]),
            )),
            MarkerKind::MissingWorkspaceMember => insights.push(insight(
                "workspace.member_missing",
                DiagnosticSeverity::Warning,
                &format!(
                    "Workspace member '{}' was not present in the selected project inventory.",
                    marker.path
                ),
                detected_from_facts((), &[stored]),
            )),
            MarkerKind::ManifestPartial => insights.push(insight(
                "manifest.partial",
                DiagnosticSeverity::Warning,
                "One or more recognized manifests could not be fully analyzed.",
                detected_from_facts((), &[stored]),
            )),
            _ => {}
        }
    }
}

fn insight(
    code: &str,
    severity: DiagnosticSeverity,
    observation: &str,
    detection: Detected<()>,
) -> Detected<Insight> {
    Detected {
        value: Insight {
            code: code.to_string(),
            severity,
            observation: observation.to_string(),
        },
        confidence: detection.confidence,
        evidence: detection.evidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        facts::{FactGraphBuilder, FactProvenance, FileFact, FileRole, ToolFact},
        scope::SemanticScope,
        Confidence, LicenseSummary, ProjectIdentity, ProjectPath, ProjectSize, RepositoryContext,
        ToolingSummary, WorkspaceSummary,
    };

    fn context() -> ProjectContext {
        ProjectContext {
            identity: ProjectIdentity {
                root: ProjectPath("/tmp/project".to_string()),
                name: "project".to_string(),
                repository_root: None,
            },
            repository: RepositoryContext::default(),
            languages: Vec::new(),
            workspace: WorkspaceSummary::default(),
            tooling: ToolingSummary::default(),
            dependencies: Vec::new(),
            documentation: Vec::new(),
            ci: Vec::new(),
            configuration: Vec::new(),
            entry_points: Vec::new(),
            development_commands: Vec::new(),
            validation_commands: Vec::new(),
            size: crate::Detected {
                value: ProjectSize {
                    files: 0,
                    bytes: 0,
                    source_files: 0,
                    test_files: 0,
                    documentation_files: 0,
                    configuration_files: 0,
                    truncated: false,
                },
                confidence: Confidence::Low,
                evidence: Vec::new(),
            },
            license: LicenseSummary::default(),
        }
    }

    fn provenance(path: &str) -> FactProvenance {
        FactProvenance {
            scanner: "test".to_string(),
            scope: SemanticScope::Primary,
            confidence: Confidence::Certain,
            evidence: vec![crate::Evidence {
                source: crate::EvidenceSource::File,
                path: Some(ProjectPath(path.to_string())),
                locator: None,
                rule: "test.lockfile".to_string(),
            }],
        }
    }

    #[test]
    fn insights_are_factual_and_deterministically_ordered() {
        let mut builder = FactGraphBuilder::new();
        for (id, path) in [("yarn", "yarn.lock"), ("npm", "package-lock.json")] {
            builder.add_fact(
                Fact::Tool(ToolFact {
                    id: id.to_string(),
                    category: ToolCategory::PackageManager,
                    source_path: path.to_string(),
                }),
                provenance(path),
            );
        }
        builder.add_fact(
            Fact::File(FileFact {
                path: "src/main.rs".to_string(),
                bytes: 1,
                role: FileRole::Source,
                extension: Some("rs".to_string()),
                language: Some("rust".to_string()),
            }),
            provenance("src/main.rs"),
        );
        let graph = builder.finish().expect("graph");

        let first = InsightsEngine::derive(&context(), &graph);
        let second = InsightsEngine::derive(&context(), &graph);
        assert_eq!(first, second);
        assert!(first
            .windows(2)
            .all(|pair| pair[0].value.code <= pair[1].value.code));
        assert!(first
            .iter()
            .any(|value| value.value.code == "package_manager.node_lockfiles_conflict"));
        assert!(first
            .iter()
            .all(|value| !value.value.observation.contains("should")));
    }
}
