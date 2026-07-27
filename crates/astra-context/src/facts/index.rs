use super::{
    model::{FactId, FactRelation},
    FactKind, RelationKind, StoredFact,
};
use crate::scope::SemanticScope;
use std::collections::BTreeMap;

#[derive(Debug)]
pub(crate) struct FactGraph {
    facts: Box<[StoredFact]>,
    by_kind: BTreeMap<FactKind, Box<[FactId]>>,
    outgoing: BTreeMap<FactId, Box<[FactRelation]>>,
}

impl FactGraph {
    pub(super) fn new(facts: Vec<StoredFact>, mut relations: Vec<FactRelation>) -> Self {
        relations.sort();

        let mut by_kind = BTreeMap::<FactKind, Vec<FactId>>::new();
        for stored in &facts {
            by_kind
                .entry(stored.fact.kind())
                .or_default()
                .push(stored.id);
        }

        let mut outgoing = BTreeMap::<FactId, Vec<FactRelation>>::new();
        for relation in &relations {
            outgoing.entry(relation.from).or_default().push(*relation);
        }

        Self {
            facts: facts.into_boxed_slice(),
            by_kind: by_kind
                .into_iter()
                .map(|(kind, ids)| (kind, ids.into_boxed_slice()))
                .collect(),
            outgoing: outgoing
                .into_iter()
                .map(|(id, values)| (id, values.into_boxed_slice()))
                .collect(),
        }
    }

    pub(crate) fn facts_of_kind(&self, kind: FactKind) -> Vec<&StoredFact> {
        self.by_kind
            .get(&kind)
            .into_iter()
            .flatten()
            .filter_map(|id| self.facts.get(id.0))
            .collect()
    }

    pub(crate) fn primary_facts_of_kind(&self, kind: FactKind) -> Vec<&StoredFact> {
        self.facts_of_kind(kind)
            .into_iter()
            .filter(|stored| {
                stored
                    .provenance
                    .iter()
                    .any(|provenance| provenance.scope == SemanticScope::Primary)
            })
            .collect()
    }

    pub(crate) fn related(&self, stored: &StoredFact, kind: RelationKind) -> Vec<&StoredFact> {
        self.outgoing
            .get(&stored.id)
            .into_iter()
            .flatten()
            .filter(|relation| relation.kind == kind)
            .filter_map(|relation| self.facts.get(relation.to.0))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn debug_facts(&self) -> Vec<String> {
        self.facts
            .iter()
            .map(|stored| format!("{:?}", stored.fact))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn relation_count(&self) -> usize {
        self.outgoing
            .values()
            .map(|relations| relations.len())
            .sum()
    }
}
