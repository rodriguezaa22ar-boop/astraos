use crate::{
    facts::{Fact, FactKind, MarkerKind},
    scanner::{detected_from_fact, metadata, ScannerInput, ScannerOutput},
    ConfigurationFile, Detected, ProjectPath,
};

pub(crate) fn scan(input: &ScannerInput<'_>) -> ScannerOutput<Vec<Detected<ConfigurationFile>>> {
    let mut values = input
        .facts()
        .primary_facts_of_kind(FactKind::Marker)
        .into_iter()
        .filter_map(|stored| {
            let Fact::Marker(marker) = &stored.fact else {
                return None;
            };
            (marker.kind == MarkerKind::Configuration).then(|| {
                detected_from_fact(
                    ConfigurationFile {
                        tool: marker.id.clone(),
                        path: ProjectPath(marker.path.clone()),
                    },
                    stored,
                )
            })
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        left.value
            .tool
            .cmp(&right.value.tool)
            .then_with(|| left.value.path.cmp(&right.value.path))
    });
    let findings = values.len();
    ScannerOutput::complete(
        metadata(
            "configuration",
            1,
            "Projects known non-sensitive configuration-file facts",
        ),
        values,
        findings,
    )
}
