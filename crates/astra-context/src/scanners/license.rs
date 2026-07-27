use crate::{
    facts::{Fact, FactKind, MarkerKind},
    scanner::{detected_from_fact, metadata, ScannerInput, ScannerOutput},
    LicenseSummary, ProjectPath,
};

pub(crate) fn scan(input: &ScannerInput<'_>) -> ScannerOutput<LicenseSummary> {
    let mut summary = LicenseSummary::default();
    for stored in input.facts().primary_facts_of_kind(FactKind::Marker) {
        let Fact::Marker(marker) = &stored.fact else {
            continue;
        };
        match marker.kind {
            MarkerKind::LicenseFile => summary
                .files
                .push(detected_from_fact(ProjectPath(marker.path.clone()), stored)),
            MarkerKind::DeclaredLicense => summary
                .declared
                .push(detected_from_fact(marker.id.clone(), stored)),
            _ => {}
        }
    }
    summary
        .files
        .sort_by(|left, right| left.value.cmp(&right.value));
    summary
        .declared
        .sort_by(|left, right| left.value.cmp(&right.value));
    let findings = summary.files.len() + summary.declared.len();
    ScannerOutput::complete(
        metadata(
            "license",
            1,
            "Projects declared licenses and conventional license-file facts",
        ),
        summary,
        findings,
    )
}
