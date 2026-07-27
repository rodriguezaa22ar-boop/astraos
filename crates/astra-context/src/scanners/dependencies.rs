use crate::{
    facts::{Fact, FactKind},
    scanner::{detected_from_fact, metadata, ScannerInput, ScannerOutput},
    DependencySummary, Detected,
};

pub(crate) fn scan(input: &ScannerInput<'_>) -> ScannerOutput<Vec<Detected<DependencySummary>>> {
    let mut values = input
        .facts()
        .primary_facts_of_kind(FactKind::Dependency)
        .into_iter()
        .filter_map(|stored| {
            let Fact::Dependency(dependency) = &stored.fact else {
                return None;
            };
            Some(detected_from_fact(
                DependencySummary {
                    ecosystem: dependency.ecosystem.clone(),
                    package: dependency.package.clone(),
                    name: dependency.name.clone(),
                    requirement: dependency.requirement.clone(),
                    scope: dependency.scope,
                },
                stored,
            ))
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        left.value
            .ecosystem
            .cmp(&right.value.ecosystem)
            .then_with(|| left.value.package.cmp(&right.value.package))
            .then_with(|| left.value.scope.cmp(&right.value.scope))
            .then_with(|| left.value.name.cmp(&right.value.name))
    });
    let findings = values.len();
    ScannerOutput::complete(
        metadata(
            "dependencies",
            1,
            "Projects direct declared dependencies from normalized manifest facts",
        ),
        values,
        findings,
    )
}
