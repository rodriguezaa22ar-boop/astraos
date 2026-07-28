use crate::{
    claim::{KnowledgeClaim, KnowledgeId},
    error::KnowledgeError,
    model::{KnowledgeCategory, KnowledgeNamespace},
    relationship::KnowledgeRelationship,
    version::KNOWLEDGE_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct KnowledgeStore {
    root: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredClaim {
    schema_version: u32,
    claim: KnowledgeClaim,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredRelationships {
    schema_version: u32,
    relationships: Vec<KnowledgeRelationship>,
}

impl KnowledgeStore {
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default_location() -> Result<PathBuf, KnowledgeError> {
        if let Some(path) = std::env::var_os("ASTRA_KNOWLEDGE_DIR") {
            return Ok(PathBuf::from(path));
        }
        dirs::home_dir()
            .map(|home| home.join(".astra").join("knowledge"))
            .ok_or(KnowledgeError::DefaultLocationUnavailable)
    }

    pub fn open_default() -> Result<Self, KnowledgeError> {
        Ok(Self::open(Self::default_location()?))
    }

    pub fn add_claim(
        &self,
        namespace: &KnowledgeNamespace,
        claim: &KnowledgeClaim,
    ) -> Result<KnowledgeId, KnowledgeError> {
        claim.validate_identity()?;
        let path = self.claim_path(namespace, claim.category, &claim.id)?;
        let envelope = StoredClaim {
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
            claim: claim.clone(),
        };
        self.write_json_atomic(&path, &envelope)?;
        Ok(claim.id.clone())
    }

    pub fn get_claim(
        &self,
        namespace: &KnowledgeNamespace,
        id: &KnowledgeId,
    ) -> Result<Option<KnowledgeClaim>, KnowledgeError> {
        for category in KnowledgeCategory::ALL {
            let path = self.claim_path(namespace, category, id)?;
            if path.exists() {
                return self.read_claim(&path).map(Some);
            }
        }
        Ok(None)
    }

    pub fn query_claims(
        &self,
        namespace: &KnowledgeNamespace,
        category: Option<KnowledgeCategory>,
    ) -> Result<Vec<KnowledgeClaim>, KnowledgeError> {
        let categories =
            category.map_or_else(|| KnowledgeCategory::ALL.to_vec(), |value| vec![value]);
        let mut claims = Vec::new();
        for category in categories {
            let directory = self.category_path(namespace, category)?;
            if !directory.exists() {
                continue;
            }
            let mut entries = fs::read_dir(&directory)
                .map_err(|source| KnowledgeError::Io {
                    path: directory.clone(),
                    source,
                })?
                .map(|entry| {
                    entry
                        .map_err(|source| KnowledgeError::Io {
                            path: directory.clone(),
                            source,
                        })
                        .map(|entry| entry.path())
                })
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort();
            for path in entries {
                if path.extension().and_then(|value| value.to_str()) == Some("json") {
                    claims.push(self.read_claim(&path)?);
                }
            }
        }
        claims.sort_by(|left, right| {
            left.category
                .cmp(&right.category)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(claims)
    }

    pub fn list_project_knowledge(
        &self,
        project: &str,
    ) -> Result<Vec<KnowledgeClaim>, KnowledgeError> {
        self.query_claims(&KnowledgeNamespace::project(project), None)
    }

    pub fn list_projects(&self) -> Result<Vec<String>, KnowledgeError> {
        let directory = self.root.join("projects");
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut projects = fs::read_dir(&directory)
            .map_err(|source| KnowledgeError::Io {
                path: directory.clone(),
                source,
            })?
            .map(|entry| {
                entry
                    .map_err(|source| KnowledgeError::Io {
                        path: directory.clone(),
                        source,
                    })
                    .and_then(|entry| {
                        let file_type = entry.file_type().map_err(|source| KnowledgeError::Io {
                            path: entry.path(),
                            source,
                        })?;
                        if !file_type.is_dir() {
                            return Ok(None);
                        }
                        entry.file_name().into_string().map(Some).map_err(|_| {
                            KnowledgeError::InvalidProjectName("non-UTF-8".to_string())
                        })
                    })
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        projects.sort();
        Ok(projects)
    }

    pub fn invalidate_claim(
        &self,
        namespace: &KnowledgeNamespace,
        id: &KnowledgeId,
    ) -> Result<KnowledgeClaim, KnowledgeError> {
        let claim = self
            .get_claim(namespace, id)?
            .ok_or_else(|| KnowledgeError::ClaimNotFound(id.to_string()))?;
        let invalidated = claim.invalidated();
        self.add_claim(namespace, &invalidated)?;
        Ok(invalidated)
    }

    pub fn add_relationship(
        &self,
        namespace: &KnowledgeNamespace,
        relationship: KnowledgeRelationship,
    ) -> Result<(), KnowledgeError> {
        if relationship.from == relationship.to {
            return Err(KnowledgeError::SelfRelationship);
        }
        if self.get_claim(namespace, &relationship.from)?.is_none() {
            return Err(KnowledgeError::RelationshipEndpointMissing(
                relationship.from.to_string(),
            ));
        }
        if self.get_claim(namespace, &relationship.to)?.is_none() {
            return Err(KnowledgeError::RelationshipEndpointMissing(
                relationship.to.to_string(),
            ));
        }
        let path = self.relationship_path(namespace)?;
        let mut relationships = self.read_relationships(&path)?;
        if !relationships.contains(&relationship) {
            relationships.push(relationship);
            relationships.sort();
            self.write_json_atomic(
                &path,
                &StoredRelationships {
                    schema_version: KNOWLEDGE_SCHEMA_VERSION,
                    relationships,
                },
            )?;
        }
        Ok(())
    }

    pub fn list_relationships(
        &self,
        namespace: &KnowledgeNamespace,
    ) -> Result<Vec<KnowledgeRelationship>, KnowledgeError> {
        let path = self.relationship_path(namespace)?;
        self.read_relationships(&path)
    }

    fn read_claim(&self, path: &Path) -> Result<KnowledgeClaim, KnowledgeError> {
        let bytes = fs::read(path).map_err(|source| KnowledgeError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let raw: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|source| KnowledgeError::Serialization {
                path: path.to_path_buf(),
                source,
            })?;
        let schema_version = raw
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
            .ok_or_else(|| KnowledgeError::Corrupt {
                path: path.to_path_buf(),
                message: "missing or invalid schema_version".to_string(),
            })?;
        if schema_version != KNOWLEDGE_SCHEMA_VERSION {
            return Err(KnowledgeError::UnsupportedSchema {
                found: schema_version,
                supported: KNOWLEDGE_SCHEMA_VERSION,
            });
        }
        let envelope: StoredClaim =
            serde_json::from_value(raw).map_err(|source| KnowledgeError::Serialization {
                path: path.to_path_buf(),
                source,
            })?;
        envelope
            .claim
            .validate_identity()
            .map_err(|error| KnowledgeError::Corrupt {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
        Ok(envelope.claim)
    }

    fn read_relationships(
        &self,
        path: &Path,
    ) -> Result<Vec<KnowledgeRelationship>, KnowledgeError> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = fs::read(path).map_err(|source| KnowledgeError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let raw: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|source| KnowledgeError::Serialization {
                path: path.to_path_buf(),
                source,
            })?;
        let schema_version = raw
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
            .ok_or_else(|| KnowledgeError::Corrupt {
                path: path.to_path_buf(),
                message: "missing or invalid schema_version".to_string(),
            })?;
        if schema_version != KNOWLEDGE_SCHEMA_VERSION {
            return Err(KnowledgeError::UnsupportedSchema {
                found: schema_version,
                supported: KNOWLEDGE_SCHEMA_VERSION,
            });
        }
        let envelope: StoredRelationships =
            serde_json::from_value(raw).map_err(|source| KnowledgeError::Serialization {
                path: path.to_path_buf(),
                source,
            })?;
        let mut relationships = envelope.relationships;
        relationships.sort();
        Ok(relationships)
    }

    fn write_json_atomic<T: Serialize>(
        &self,
        path: &Path,
        value: &T,
    ) -> Result<(), KnowledgeError> {
        let parent = path.parent().ok_or_else(|| KnowledgeError::Io {
            path: path.to_path_buf(),
            source: io::Error::other("knowledge path has no parent"),
        })?;
        fs::create_dir_all(parent).map_err(|source| KnowledgeError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
        let bytes =
            serde_json::to_vec_pretty(value).map_err(|source| KnowledgeError::Serialization {
                path: path.to_path_buf(),
                source,
            })?;
        let temporary = path.with_extension("json.tmp");
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temporary)
            .map_err(|source| KnowledgeError::Io {
                path: temporary.clone(),
                source,
            })?;
        file.write_all(&bytes)
            .map_err(|source| KnowledgeError::Io {
                path: temporary.clone(),
                source,
            })?;
        file.write_all(b"\n").map_err(|source| KnowledgeError::Io {
            path: temporary.clone(),
            source,
        })?;
        file.sync_all().map_err(|source| KnowledgeError::Io {
            path: temporary.clone(),
            source,
        })?;
        drop(file);
        fs::rename(&temporary, path).map_err(|source| KnowledgeError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    fn claim_path(
        &self,
        namespace: &KnowledgeNamespace,
        category: KnowledgeCategory,
        id: &KnowledgeId,
    ) -> Result<PathBuf, KnowledgeError> {
        Ok(self
            .category_path(namespace, category)?
            .join(format!("{}.json", id.as_str())))
    }

    fn category_path(
        &self,
        namespace: &KnowledgeNamespace,
        category: KnowledgeCategory,
    ) -> Result<PathBuf, KnowledgeError> {
        let base = match namespace {
            KnowledgeNamespace::Global => self.root.join("global"),
            KnowledgeNamespace::Project(project) => {
                validate_project_name(project)?;
                self.root.join("projects").join(project)
            }
        };
        Ok(base.join(category.as_str()))
    }

    fn relationship_path(&self, namespace: &KnowledgeNamespace) -> Result<PathBuf, KnowledgeError> {
        let base = match namespace {
            KnowledgeNamespace::Global => self.root.join("global"),
            KnowledgeNamespace::Project(project) => {
                validate_project_name(project)?;
                self.root.join("projects").join(project)
            }
        };
        Ok(base.join("relationships.json"))
    }
}

fn validate_project_name(name: &str) -> Result<(), KnowledgeError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.chars().any(char::is_control)
    {
        return Err(KnowledgeError::InvalidProjectName(name.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        confidence::Confidence,
        evidence::{Evidence, EvidenceKind},
        relationship::RelationshipType,
        validity::Validity,
        KnowledgeCategory,
    };
    use tempfile::TempDir;

    fn store() -> (TempDir, KnowledgeStore) {
        let directory = tempfile::tempdir().expect("knowledge root");
        let store = KnowledgeStore::open(directory.path());
        (directory, store)
    }

    fn claim() -> KnowledgeClaim {
        KnowledgeClaim::new(
            KnowledgeCategory::Fact,
            "project:demo",
            "uses_language",
            serde_json::json!("rust"),
            vec![Evidence::new(EvidenceKind::ContextFact, "language-rust")],
            Confidence::High,
            Validity::Current,
        )
        .and_then(|claim| claim.with_created_at("2026-01-01T00:00:00Z"))
        .expect("claim")
    }

    #[test]
    fn missing_store_lists_empty_and_writes_versioned_claims_atomically() {
        let (_directory, store) = store();
        let namespace = KnowledgeNamespace::project("demo");
        assert!(store
            .list_project_knowledge("demo")
            .expect("empty list")
            .is_empty());
        let claim = claim();
        let id = store.add_claim(&namespace, &claim).expect("save claim");
        assert_eq!(
            store.get_claim(&namespace, &id).expect("get claim"),
            Some(claim)
        );
        let path = store
            .root
            .join("projects/demo/facts")
            .join(format!("{}.json", id));
        let text = fs::read_to_string(path).expect("stored claim");
        assert!(text.contains("\"schema_version\": 1"));
        assert!(!text.contains("temporary"));
    }

    #[test]
    fn corrupted_and_future_versioned_claims_are_recoverable_errors() {
        let (_directory, store) = store();
        let namespace = KnowledgeNamespace::project("demo");
        let claim = claim();
        let id = claim.id.clone();
        store.add_claim(&namespace, &claim).expect("save claim");
        let path = store
            .root
            .join("projects/demo/facts")
            .join(format!("{}.json", id));
        fs::write(&path, "not json").expect("corrupt claim");
        assert!(matches!(
            store.get_claim(&namespace, &id),
            Err(KnowledgeError::Serialization { .. })
        ));
        fs::write(&path, r#"{"schema_version":99,"claim":{}}"#).expect("future claim");
        assert!(matches!(
            store.get_claim(&namespace, &id),
            Err(KnowledgeError::UnsupportedSchema { .. })
        ));
    }

    #[test]
    fn invalidation_preserves_the_claim_and_relationships_validate_endpoints() {
        let (_directory, store) = store();
        let namespace = KnowledgeNamespace::project("demo");
        let first = claim();
        let second = KnowledgeClaim::new(
            KnowledgeCategory::Decision,
            "project:demo",
            "uses_language",
            serde_json::json!("rust"),
            vec![Evidence::new(EvidenceKind::ContextFact, "language-rust")],
            Confidence::High,
            Validity::Current,
        )
        .and_then(|claim| claim.with_created_at("2026-01-01T00:00:00Z"))
        .expect("second claim");
        store.add_claim(&namespace, &first).expect("first claim");
        store.add_claim(&namespace, &second).expect("second claim");
        store
            .add_relationship(
                &namespace,
                KnowledgeRelationship {
                    from: second.id.clone(),
                    relationship: RelationshipType::DerivedFrom,
                    to: first.id.clone(),
                },
            )
            .expect("relationship");
        let invalidated = store
            .invalidate_claim(&namespace, &first.id)
            .expect("invalidate");
        assert_eq!(invalidated.validity, Validity::Invalidated);
        assert_eq!(
            store.list_project_knowledge("demo").expect("claims").len(),
            2
        );
        assert_eq!(
            store
                .list_relationships(&namespace)
                .expect("relationships")
                .len(),
            1
        );
        assert!(matches!(
            store.add_relationship(
                &namespace,
                KnowledgeRelationship {
                    from: first.id,
                    relationship: RelationshipType::RelatedTo,
                    to: KnowledgeId::parse(
                        "k1-0000000000000000000000000000000000000000000000000000000000000000"
                    )
                    .expect("id"),
                }
            ),
            Err(KnowledgeError::RelationshipEndpointMissing(_))
        ));
    }

    #[test]
    fn project_names_cannot_escape_storage_root() {
        let (_directory, store) = store();
        assert!(matches!(
            store.list_project_knowledge("../outside"),
            Err(KnowledgeError::InvalidProjectName(_))
        ));
    }
}
