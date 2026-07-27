use crate::{
    facts::{FactGraph, StoredFact},
    Confidence, Detected, Diagnostic, Evidence, ScannerMetadata, ScannerResult, ScannerStatus,
};
use std::collections::BTreeSet;

pub(crate) struct ScannerInput<'a> {
    facts: &'a FactGraph,
}

impl<'a> ScannerInput<'a> {
    pub(crate) fn new(facts: &'a FactGraph) -> Self {
        Self { facts }
    }

    pub(crate) fn facts(&self) -> &'a FactGraph {
        self.facts
    }
}

pub(crate) struct ScannerOutput<T> {
    pub(crate) value: T,
    pub(crate) result: ScannerResult,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl<T> ScannerOutput<T> {
    pub(crate) fn complete(metadata: ScannerMetadata, value: T, findings: usize) -> Self {
        Self {
            value,
            result: ScannerResult {
                metadata,
                status: ScannerStatus::Complete,
                findings,
                diagnostic_codes: Vec::new(),
            },
            diagnostics: Vec::new(),
        }
    }
}

pub(crate) fn metadata(id: &str, version: u32, description: &str) -> ScannerMetadata {
    ScannerMetadata {
        id: id.to_string(),
        version,
        description: description.to_string(),
    }
}

pub(crate) fn detected_from_fact<T>(value: T, fact: &StoredFact) -> Detected<T> {
    detected_from_facts(value, std::slice::from_ref(&fact))
}

pub(crate) fn detected_from_facts<T>(value: T, facts: &[&StoredFact]) -> Detected<T> {
    let confidence = facts
        .iter()
        .flat_map(|fact| fact.provenance.iter())
        .map(|provenance| provenance.confidence)
        .max()
        .unwrap_or(Confidence::Low);
    let evidence = facts
        .iter()
        .flat_map(|fact| fact.provenance.iter())
        .flat_map(|provenance| provenance.evidence.iter().cloned())
        .collect::<BTreeSet<Evidence>>()
        .into_iter()
        .take(20)
        .collect();

    Detected {
        value,
        confidence,
        evidence,
    }
}
