use crate::{
    facts::{Fact, FactKind, StoredFact},
    scanner::{detected_from_facts, metadata, ScannerInput, ScannerOutput},
    Detected, LanguageSummary,
};
use std::collections::BTreeMap;

pub(crate) fn scan(input: &ScannerInput<'_>) -> ScannerOutput<Vec<Detected<LanguageSummary>>> {
    let mut grouped = BTreeMap::<String, (u64, u64, Vec<&StoredFact>)>::new();
    for stored in input.facts().primary_facts_of_kind(FactKind::File) {
        let Fact::File(file) = &stored.fact else {
            continue;
        };
        let Some(language) = &file.language else {
            continue;
        };
        let group = grouped
            .entry(language.clone())
            .or_insert_with(|| (0, 0, Vec::new()));
        group.0 += 1;
        group.1 = group.1.saturating_add(file.bytes);
        group.2.push(stored);
    }

    let mut values = grouped
        .into_iter()
        .map(|(id, (file_count, bytes, facts))| {
            detected_from_facts(
                LanguageSummary {
                    id,
                    file_count,
                    bytes,
                },
                &facts,
            )
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        right
            .value
            .bytes
            .cmp(&left.value.bytes)
            .then_with(|| left.value.id.cmp(&right.value.id))
    });
    let findings = values.len();
    ScannerOutput::complete(
        metadata(
            "languages",
            1,
            "Aggregates language observations from immutable file facts",
        ),
        values,
        findings,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        facts::{FactGraphBuilder, FactProvenance, FileFact, FileRole},
        scope::SemanticScope,
        Confidence, Evidence, EvidenceSource, ProjectPath,
    };

    fn add_file(builder: &mut FactGraphBuilder, path: &str, language: &str, bytes: u64) {
        builder.add_fact(
            Fact::File(FileFact {
                path: path.to_string(),
                bytes,
                role: FileRole::Source,
                extension: Some("x".to_string()),
                language: Some(language.to_string()),
            }),
            FactProvenance {
                scanner: "test".to_string(),
                scope: SemanticScope::Primary,
                confidence: Confidence::High,
                evidence: vec![Evidence {
                    source: EvidenceSource::File,
                    path: Some(ProjectPath(path.to_string())),
                    locator: None,
                    rule: "test".to_string(),
                }],
            },
        );
    }

    #[test]
    fn orders_languages_by_bytes_then_identifier() {
        let mut builder = FactGraphBuilder::new();
        add_file(&mut builder, "a.rs", "rust", 10);
        add_file(&mut builder, "b.py", "python", 20);
        let graph = builder.finish().expect("graph");
        let output = scan(&ScannerInput::new(&graph));
        assert_eq!(output.value[0].value.id, "python");
        assert_eq!(output.value[1].value.id, "rust");
    }
}
