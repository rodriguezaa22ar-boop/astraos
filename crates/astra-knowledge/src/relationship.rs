use crate::KnowledgeId;
use serde::{Deserialize, Serialize};

/// Typed provenance and semantic relationships between claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipType {
    CreatedBy,
    Supports,
    DependsOn,
    VerifiedBy,
    InvalidatedBy,
    RelatedTo,
    DerivedFrom,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct KnowledgeRelationship {
    pub from: KnowledgeId,
    pub relationship: RelationshipType,
    pub to: KnowledgeId,
}
