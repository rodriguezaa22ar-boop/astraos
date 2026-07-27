use crate::{
    facts::{Fact, FactKind, FileRole, MarkerKind, RepositoryFact},
    scanner::{detected_from_fact, detected_from_facts, ScannerInput},
    Detected, GitChange, ProjectIdentity, ProjectPath, ProjectSize, RecentCommit,
    RepositoryContext, RepositoryState,
};
use std::path::Path;

pub(crate) fn identity(input: &ScannerInput<'_>) -> ProjectIdentity {
    let root = input
        .facts()
        .primary_facts_of_kind(FactKind::ProjectRoot)
        .into_iter()
        .find_map(|stored| match &stored.fact {
            Fact::ProjectRoot(root) => Some(root.clone()),
            _ => None,
        })
        .unwrap_or_else(|| ".".to_string());
    let name = Path::new(&root)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(&root)
        .to_string();
    let repository_root = input
        .facts()
        .primary_facts_of_kind(FactKind::Repository)
        .into_iter()
        .find_map(|stored| match &stored.fact {
            Fact::Repository(RepositoryFact::Root(path)) => {
                Some(detected_from_fact(ProjectPath(path.clone()), stored))
            }
            _ => None,
        });

    ProjectIdentity {
        root: ProjectPath(root),
        name,
        repository_root,
    }
}

pub(crate) fn repository(input: &ScannerInput<'_>) -> RepositoryContext {
    let mut value = RepositoryContext::default();
    let mut states = Vec::new();
    let mut commits = Vec::new();

    for stored in input.facts().primary_facts_of_kind(FactKind::Repository) {
        let Fact::Repository(fact) = &stored.fact else {
            continue;
        };
        match fact {
            RepositoryFact::State(state) => states.push((parse_state(state), stored)),
            RepositoryFact::Root(_) => {}
            RepositoryFact::Branch(branch) => {
                value.branch = Some(detected_from_fact(branch.clone(), stored));
            }
            RepositoryFact::Head(head) => {
                value.head = Some(detected_from_fact(head.clone(), stored));
            }
            RepositoryFact::Clean(clean) => {
                value.clean = Some(detected_from_fact(*clean, stored));
            }
            RepositoryFact::Change { path, status } => {
                value.changes.push(detected_from_fact(
                    GitChange {
                        path: ProjectPath(path.clone()),
                        status: status.clone(),
                    },
                    stored,
                ));
            }
            RepositoryFact::Commit {
                ordinal,
                id,
                authored_at,
                subject,
            } => commits.push((
                *ordinal,
                detected_from_fact(
                    RecentCommit {
                        id: id.clone(),
                        authored_at: authored_at.clone(),
                        subject: subject.clone(),
                    },
                    stored,
                ),
            )),
        }
    }

    if !states.is_empty() {
        let state = states
            .iter()
            .fold(RepositoryState::NotRepository, |current, (candidate, _)| {
                merge_state(current, *candidate)
            });
        let state_facts = states.iter().map(|(_, stored)| *stored).collect::<Vec<_>>();
        value.state = detected_from_facts(state, &state_facts);
    }
    value.changes.sort_by(|left, right| {
        left.value
            .path
            .cmp(&right.value.path)
            .then_with(|| left.value.status.cmp(&right.value.status))
    });
    commits.sort_by_key(|(ordinal, _)| *ordinal);
    value.recent_commits = commits.into_iter().map(|(_, commit)| commit).collect();
    value
}

pub(crate) fn size(input: &ScannerInput<'_>) -> Detected<ProjectSize> {
    let mut value = ProjectSize {
        files: 0,
        bytes: 0,
        source_files: 0,
        test_files: 0,
        documentation_files: 0,
        configuration_files: 0,
        truncated: false,
    };
    let mut evidence_facts = Vec::new();
    for stored in input.facts().primary_facts_of_kind(FactKind::File) {
        let Fact::File(file) = &stored.fact else {
            continue;
        };
        evidence_facts.push(stored);
        value.files = value.files.saturating_add(1);
        value.bytes = value.bytes.saturating_add(file.bytes);
        match file.role {
            FileRole::Source => value.source_files = value.source_files.saturating_add(1),
            FileRole::Test => value.test_files = value.test_files.saturating_add(1),
            FileRole::Documentation => {
                value.documentation_files = value.documentation_files.saturating_add(1);
            }
            FileRole::Configuration => {
                value.configuration_files = value.configuration_files.saturating_add(1);
            }
            FileRole::Other => {}
        }
    }
    let inventory_facts = input
        .facts()
        .primary_facts_of_kind(FactKind::Marker)
        .into_iter()
        .filter(|stored| {
            matches!(
                &stored.fact,
                Fact::Marker(marker)
                    if matches!(
                        marker.kind,
                        MarkerKind::InventoryComplete
                            | MarkerKind::InventoryPartial
                            | MarkerKind::InventoryTruncated
                    )
            )
        })
        .collect::<Vec<_>>();
    value.truncated = inventory_facts.iter().any(|stored| {
        matches!(
            &stored.fact,
            Fact::Marker(marker) if marker.kind == MarkerKind::InventoryTruncated
        )
    });
    evidence_facts.extend(inventory_facts);
    detected_from_facts(value, &evidence_facts)
}

fn parse_state(value: &str) -> RepositoryState {
    match value {
        "git" => RepositoryState::Git,
        "git_unavailable" => RepositoryState::GitUnavailable,
        "partial" => RepositoryState::Partial,
        _ => RepositoryState::NotRepository,
    }
}

fn merge_state(current: RepositoryState, candidate: RepositoryState) -> RepositoryState {
    use RepositoryState::{Git, GitUnavailable, NotRepository, Partial};
    match (current, candidate) {
        (Partial, _) | (_, Partial) => Partial,
        (Git, _) | (_, Git) => Git,
        (GitUnavailable, _) | (_, GitUnavailable) => GitUnavailable,
        _ => NotRepository,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        facts::{FactGraphBuilder, FactProvenance},
        scope::SemanticScope,
        Confidence,
    };

    fn provenance() -> FactProvenance {
        FactProvenance {
            scanner: "test".to_string(),
            scope: SemanticScope::Primary,
            confidence: Confidence::Certain,
            evidence: Vec::new(),
        }
    }

    #[test]
    fn identity_and_repository_are_derived_only_from_facts() {
        let mut builder = FactGraphBuilder::new();
        builder.add_fact(
            Fact::ProjectRoot("/tmp/example-project".to_string()),
            provenance(),
        );
        builder.add_fact(
            Fact::Repository(RepositoryFact::State("git".to_string())),
            provenance(),
        );
        builder.add_fact(
            Fact::Repository(RepositoryFact::State("partial".to_string())),
            provenance(),
        );
        builder.add_fact(
            Fact::Repository(RepositoryFact::Branch("main".to_string())),
            provenance(),
        );
        let graph = builder.finish().expect("graph");
        let input = ScannerInput::new(&graph);

        assert_eq!(identity(&input).name, "example-project");
        let repository = repository(&input);
        assert_eq!(repository.state.value, RepositoryState::Partial);
        assert_eq!(
            repository
                .branch
                .as_ref()
                .map(|branch| branch.value.as_str()),
            Some("main")
        );
    }
}
