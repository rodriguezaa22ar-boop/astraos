use crate::{
    facts::{Fact, FactKind, MarkerKind, RelationKind, StoredFact},
    scanner::{detected_from_fact, metadata, ScannerInput, ScannerOutput},
    Detected, EntryPoint, EntryPointKind, EvidenceSource, ProjectPath,
};
use std::collections::BTreeSet;

pub(crate) fn scan(input: &ScannerInput<'_>) -> ScannerOutput<Vec<Detected<EntryPoint>>> {
    let facts = input.facts().primary_facts_of_kind(FactKind::Marker);
    let manifest_declared_paths = facts
        .iter()
        .filter_map(|stored| {
            let Fact::Marker(marker) = &stored.fact else {
                return None;
            };
            (marker.kind == MarkerKind::EntryPoint && has_manifest_evidence(stored))
                .then(|| marker.path.clone())
        })
        .collect::<BTreeSet<_>>();
    let mut values = facts
        .into_iter()
        .filter_map(|stored| {
            let Fact::Marker(marker) = &stored.fact else {
                return None;
            };
            if marker.kind != MarkerKind::EntryPoint {
                return None;
            }
            if manifest_declared_paths.contains(&marker.path) && !has_manifest_evidence(stored) {
                return None;
            }
            let kind = match marker.id.as_str() {
                "binary" => EntryPointKind::Binary,
                "library" => EntryPointKind::Library,
                "script" => EntryPointKind::Script,
                _ => EntryPointKind::Application,
            };
            let package = input
                .facts()
                .related(stored, RelationKind::EntrypointOf)
                .into_iter()
                .find_map(|related| match &related.fact {
                    Fact::Package(package) => Some(package.name.clone()),
                    _ => None,
                });
            Some(detected_from_fact(
                EntryPoint {
                    path: ProjectPath(marker.path.clone()),
                    kind,
                    language: marker.detail.clone(),
                    package,
                },
                stored,
            ))
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        left.value
            .path
            .cmp(&right.value.path)
            .then_with(|| left.value.kind.cmp(&right.value.kind))
            .then_with(|| left.value.package.cmp(&right.value.package))
    });
    let findings = values.len();
    ScannerOutput::complete(
        metadata(
            "entry_points",
            1,
            "Projects manifest-declared and conventional entry-point facts",
        ),
        values,
        findings,
    )
}

fn has_manifest_evidence(stored: &StoredFact) -> bool {
    stored.provenance.iter().any(|provenance| {
        provenance
            .evidence
            .iter()
            .any(|evidence| evidence.source == EvidenceSource::Manifest)
    })
}
