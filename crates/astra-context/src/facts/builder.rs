use super::{index::FactGraph, Fact, FactKey, FactProvenance, StoredFact};
use crate::{facts::model::FactRelation, ContextError};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
struct PendingFact {
    fact: Fact,
    provenance: BTreeSet<FactProvenance>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PendingRelation {
    from: FactKey,
    to: FactKey,
    kind: super::RelationKind,
}

#[derive(Debug, Default)]
pub(crate) struct FactGraphBuilder {
    facts: BTreeMap<FactKey, PendingFact>,
    relations: BTreeSet<PendingRelation>,
}

impl FactGraphBuilder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add_fact(&mut self, fact: Fact, provenance: FactProvenance) -> FactKey {
        let key = fact.stable_key();
        self.facts
            .entry(key.clone())
            .and_modify(|pending| {
                pending.provenance.insert(provenance.clone());
            })
            .or_insert_with(|| PendingFact {
                fact,
                provenance: BTreeSet::from([provenance]),
            });
        key
    }

    pub(crate) fn add_relation(&mut self, from: &FactKey, to: &FactKey, kind: super::RelationKind) {
        self.relations.insert(PendingRelation {
            from: from.clone(),
            to: to.clone(),
            kind,
        });
    }

    pub(crate) fn finish(self) -> Result<FactGraph, ContextError> {
        let mut ids = BTreeMap::new();
        let mut stored = Vec::with_capacity(self.facts.len());

        for (index, (key, pending)) in self.facts.into_iter().enumerate() {
            let id = super::model::FactId(index);
            ids.insert(key, id);
            let mut provenance = pending.provenance.into_iter().collect::<Vec<_>>();
            provenance.truncate(20);
            stored.push(StoredFact {
                id,
                fact: pending.fact,
                provenance,
            });
        }

        let mut relations = Vec::with_capacity(self.relations.len());
        for relation in self.relations {
            let from = ids.get(&relation.from).copied().ok_or_else(|| {
                ContextError::InvariantViolation(
                    "fact relation references a missing source fact".to_string(),
                )
            })?;
            let to = ids.get(&relation.to).copied().ok_or_else(|| {
                ContextError::InvariantViolation(
                    "fact relation references a missing target fact".to_string(),
                )
            })?;
            relations.push(FactRelation {
                from,
                to,
                kind: relation.kind,
            });
        }

        Ok(FactGraph::new(stored, relations))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        facts::{FileFact, FileRole},
        scope::SemanticScope,
        Confidence, Evidence, EvidenceSource, ProjectPath,
    };

    fn provenance(rule: &str) -> FactProvenance {
        FactProvenance {
            scanner: "test".to_string(),
            scope: SemanticScope::Primary,
            confidence: Confidence::High,
            evidence: vec![Evidence {
                source: EvidenceSource::File,
                path: Some(ProjectPath("src/main.rs".to_string())),
                locator: None,
                rule: rule.to_string(),
            }],
        }
    }

    fn file(path: &str) -> Fact {
        Fact::File(FileFact {
            path: path.to_string(),
            bytes: 10,
            role: FileRole::Source,
            extension: Some("rs".to_string()),
            language: Some("rust".to_string()),
        })
    }

    #[test]
    fn insertion_order_does_not_change_frozen_facts() {
        let mut first = FactGraphBuilder::new();
        first.add_fact(file("b.rs"), provenance("b"));
        first.add_fact(file("a.rs"), provenance("a"));

        let mut second = FactGraphBuilder::new();
        second.add_fact(file("a.rs"), provenance("a"));
        second.add_fact(file("b.rs"), provenance("b"));

        let first = first.finish().expect("first graph");
        let second = second.finish().expect("second graph");
        assert_eq!(first.debug_facts(), second.debug_facts());
    }

    #[test]
    fn duplicate_facts_merge_provenance() {
        let mut builder = FactGraphBuilder::new();
        builder.add_fact(file("src/main.rs"), provenance("extension"));
        builder.add_fact(file("src/main.rs"), provenance("manifest"));

        let graph = builder.finish().expect("graph");
        let facts = graph.facts_of_kind(super::super::FactKind::File);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].provenance.len(), 2);
    }

    #[test]
    fn dangling_relationship_is_rejected() {
        let mut builder = FactGraphBuilder::new();
        let present = builder.add_fact(file("src/main.rs"), provenance("present"));
        let missing = file("missing.rs").stable_key();
        builder.add_relation(&present, &missing, super::super::RelationKind::Supports);

        assert!(matches!(
            builder.finish(),
            Err(ContextError::InvariantViolation(_))
        ));
    }

    #[test]
    fn relationships_resolve_to_frozen_fact_ids() {
        let mut builder = FactGraphBuilder::new();
        let first = builder.add_fact(file("src/main.rs"), provenance("first"));
        let second = builder.add_fact(file("src/lib.rs"), provenance("second"));
        builder.add_relation(&first, &second, super::super::RelationKind::Supports);

        let graph = builder.finish().expect("graph");
        assert_eq!(graph.relation_count(), 1);
    }
}
