use crate::{
    facts::{Fact, FactKind, MarkerKind},
    scanner::{detected_from_fact, metadata, ScannerInput, ScannerOutput},
    CiDefinition, Detected, ProjectPath,
};

pub(crate) fn scan(input: &ScannerInput<'_>) -> ScannerOutput<Vec<Detected<CiDefinition>>> {
    let mut values = input
        .facts()
        .primary_facts_of_kind(FactKind::Marker)
        .into_iter()
        .filter_map(|stored| {
            let Fact::Marker(marker) = &stored.fact else {
                return None;
            };
            (marker.kind == MarkerKind::Ci).then(|| {
                detected_from_fact(
                    CiDefinition {
                        provider: marker.id.clone(),
                        path: ProjectPath(marker.path.clone()),
                    },
                    stored,
                )
            })
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        left.value
            .provider
            .cmp(&right.value.provider)
            .then_with(|| left.value.path.cmp(&right.value.path))
    });
    let findings = values.len();
    ScannerOutput::complete(
        metadata(
            "ci",
            1,
            "Projects continuous-integration configuration facts",
        ),
        values,
        findings,
    )
}
