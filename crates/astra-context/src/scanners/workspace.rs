use crate::{
    facts::{Fact, FactKind, StoredFact, ToolCategory},
    scanner::{detected_from_fact, detected_from_facts, metadata, ScannerInput, ScannerOutput},
    Detected, PackageSummary, ToolSummary, WorkspaceSummary,
};
use std::collections::BTreeMap;

pub(crate) struct WorkspaceProjection {
    pub(crate) summary: WorkspaceSummary,
    pub(crate) package_managers: Vec<Detected<ToolSummary>>,
}

pub(crate) fn scan(input: &ScannerInput<'_>) -> ScannerOutput<WorkspaceProjection> {
    let mut kind_facts = BTreeMap::<String, Vec<&StoredFact>>::new();
    for stored in input.facts().primary_facts_of_kind(FactKind::Workspace) {
        let Fact::Workspace(workspace) = &stored.fact else {
            continue;
        };
        kind_facts
            .entry(workspace.kind.clone())
            .or_default()
            .push(stored);
    }
    let kinds = kind_facts
        .into_iter()
        .map(|(kind, facts)| detected_from_facts(kind, &facts))
        .collect::<Vec<_>>();

    let mut packages = input
        .facts()
        .primary_facts_of_kind(FactKind::Package)
        .into_iter()
        .filter_map(package)
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| {
        left.value
            .path
            .cmp(&right.value.path)
            .then_with(|| left.value.name.cmp(&right.value.name))
    });

    let mut manager_facts = BTreeMap::<String, Vec<&StoredFact>>::new();
    for stored in input.facts().primary_facts_of_kind(FactKind::Tool) {
        let Fact::Tool(tool) = &stored.fact else {
            continue;
        };
        if tool.category == ToolCategory::PackageManager {
            manager_facts
                .entry(tool.id.clone())
                .or_default()
                .push(stored);
        }
    }
    let package_managers = manager_facts
        .into_iter()
        .map(|(id, facts)| detected_from_facts(ToolSummary { id }, &facts))
        .collect::<Vec<_>>();

    let findings = kinds.len() + packages.len() + package_managers.len();
    ScannerOutput::complete(
        metadata(
            "workspace",
            1,
            "Projects workspace and package relationships from immutable facts",
        ),
        WorkspaceProjection {
            summary: WorkspaceSummary { kinds, packages },
            package_managers,
        },
        findings,
    )
}

fn package(stored: &StoredFact) -> Option<Detected<PackageSummary>> {
    let Fact::Package(package) = &stored.fact else {
        return None;
    };
    Some(detected_from_fact(
        PackageSummary {
            name: package.name.clone(),
            path: crate::ProjectPath(package.root.clone()),
            ecosystem: package.ecosystem.clone(),
            manifest: crate::ProjectPath(package.manifest.clone()),
        },
        stored,
    ))
}
