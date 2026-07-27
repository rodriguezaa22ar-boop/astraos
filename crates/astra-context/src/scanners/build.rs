use crate::{
    facts::{Fact, FactKind, StoredFact, ToolCategory},
    scanner::{detected_from_facts, metadata, ScannerInput, ScannerOutput},
    Detected, ToolSummary,
};
use std::collections::BTreeMap;

pub(crate) fn scan(input: &ScannerInput<'_>) -> ScannerOutput<Vec<Detected<ToolSummary>>> {
    let mut grouped = BTreeMap::<String, Vec<&StoredFact>>::new();
    for stored in input.facts().primary_facts_of_kind(FactKind::Tool) {
        let Fact::Tool(tool) = &stored.fact else {
            continue;
        };
        if tool.category == ToolCategory::BuildSystem {
            grouped.entry(tool.id.clone()).or_default().push(stored);
        }
    }
    let values = grouped
        .into_iter()
        .map(|(id, facts)| detected_from_facts(ToolSummary { id }, &facts))
        .collect::<Vec<_>>();
    let findings = values.len();
    ScannerOutput::complete(
        metadata("build", 1, "Projects detected build-system facts"),
        values,
        findings,
    )
}
