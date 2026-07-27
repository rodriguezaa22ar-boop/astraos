use crate::{
    facts::{Fact, FactKind},
    scanner::{detected_from_fact, metadata, ScannerInput, ScannerOutput},
    Detected, DocumentSummary, ProjectPath,
};

pub(crate) fn scan(input: &ScannerInput<'_>) -> ScannerOutput<Vec<Detected<DocumentSummary>>> {
    let mut values = input
        .facts()
        .primary_facts_of_kind(FactKind::Documentation)
        .into_iter()
        .filter_map(|stored| {
            let Fact::Documentation(document) = &stored.fact else {
                return None;
            };
            Some(detected_from_fact(
                DocumentSummary {
                    path: ProjectPath(document.path.clone()),
                    kind: document.kind,
                    title: document.title.clone(),
                    headings: document.headings.clone(),
                    bytes: document.bytes,
                },
                stored,
            ))
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| left.value.path.cmp(&right.value.path));
    let findings = values.len();
    ScannerOutput::complete(
        metadata(
            "documentation",
            1,
            "Projects bounded documentation metadata and headings",
        ),
        values,
        findings,
    )
}
