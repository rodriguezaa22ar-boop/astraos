use crate::{
    claim::{KnowledgeClaim, KnowledgeId},
    error::KnowledgeError,
    model::{KnowledgeCategory, KnowledgeNamespace},
    relationship::KnowledgeRelationship,
    transaction::{
        OperatorHistoryOperation, OperatorResponseHistoryEntry, OperatorSequenceState,
        StoredResponseRevision, TransactionCommit, TransactionManifest, TransactionMutation,
    },
    version::KNOWLEDGE_SCHEMA_VERSION,
    NewOperatorResponse, OperatorResponse, OperatorResponseId, OperatorTransactionId,
    ResponseAuditMetadata, ResponseLifecycle, OPERATOR_RESPONSE_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
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

#[derive(Debug)]
struct CurrentResponse {
    response: OperatorResponse,
    transaction_id: OperatorTransactionId,
}

#[derive(Debug)]
struct OperatorStoreLock {
    path: PathBuf,
}

impl Drop for OperatorStoreLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
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

    pub fn create_operator_response(
        &self,
        request: NewOperatorResponse,
    ) -> Result<OperatorResponse, KnowledgeError> {
        validate_project_name(&request.project)?;
        request.target.validate()?;
        request.payload.validate()?;
        if request.payload.is_governing()
            && request.target.classification != crate::OperatorTargetClassification::Derived
        {
            return Err(KnowledgeError::ObservedTargetGovernance);
        }
        let _lock = self.operator_lock(&request.project)?;
        let current = self.replay_operator_responses(&request.project)?;
        let superseded = superseded_response_ids(current.values().map(|entry| &entry.response));
        validate_governing_response(
            current
                .values()
                .map(|entry| &entry.response)
                .filter(|response| !superseded.contains(&response.id)),
            &request.target,
            &request.payload,
            request.supersedes.as_ref(),
        )?;
        let (transaction_id, response_id) = self.allocate_operator_ids(&request.project, true)?;
        let response = OperatorResponse {
            schema_version: OPERATOR_RESPONSE_SCHEMA_VERSION,
            id: response_id.ok_or_else(|| {
                KnowledgeError::InvalidOperatorResponse(
                    "response allocation did not produce an ID".to_string(),
                )
            })?,
            project: request.project.clone(),
            target: request.target,
            operator: request.operator,
            lifecycle: request.payload.initial_lifecycle(),
            payload: request.payload,
            supersedes: request.supersedes,
            audit: ResponseAuditMetadata {
                transaction_id: transaction_id.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        };
        response.validate()?;
        self.commit_response_transaction(
            &request.project,
            transaction_id,
            OperatorHistoryOperation::Create,
            response.clone(),
            None,
        )?;
        Ok(response)
    }

    pub fn edit_operator_response_draft(
        &self,
        project: &str,
        id: &OperatorResponseId,
        payload: crate::OperatorResponsePayload,
    ) -> Result<OperatorResponse, KnowledgeError> {
        validate_project_name(project)?;
        payload.validate()?;
        if payload.initial_lifecycle() != ResponseLifecycle::Draft {
            return Err(KnowledgeError::InvalidResponseTransition(
                "a draft cannot be edited into an immediate-active response".to_string(),
            ));
        }
        let _lock = self.operator_lock(project)?;
        let current = self.replay_operator_responses(project)?;
        let previous = current
            .get(id)
            .ok_or_else(|| KnowledgeError::OperatorResponseNotFound(id.to_string()))?;
        if previous.response.lifecycle != ResponseLifecycle::Draft {
            return Err(KnowledgeError::InvalidResponseTransition(
                "only draft responses are editable".to_string(),
            ));
        }
        let (transaction_id, response_id) = self.allocate_operator_ids(project, false)?;
        if response_id.is_some() {
            return Err(KnowledgeError::InvalidOperatorResponse(
                "draft edit unexpectedly allocated a response ID".to_string(),
            ));
        }
        let mut response = previous.response.clone();
        response.payload = payload;
        response.audit = ResponseAuditMetadata {
            transaction_id: transaction_id.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        response.validate()?;
        self.commit_response_transaction(
            project,
            transaction_id,
            OperatorHistoryOperation::EditDraft,
            response.clone(),
            Some(previous.transaction_id.clone()),
        )?;
        Ok(response)
    }

    pub fn delete_operator_response_draft(
        &self,
        project: &str,
        id: &OperatorResponseId,
    ) -> Result<(), KnowledgeError> {
        validate_project_name(project)?;
        let _lock = self.operator_lock(project)?;
        let current = self.replay_operator_responses(project)?;
        let previous = current
            .get(id)
            .ok_or_else(|| KnowledgeError::OperatorResponseNotFound(id.to_string()))?;
        if previous.response.lifecycle != ResponseLifecycle::Draft {
            return Err(KnowledgeError::InvalidResponseTransition(
                "only draft responses are deletable".to_string(),
            ));
        }
        let (transaction_id, response_id) = self.allocate_operator_ids(project, false)?;
        if response_id.is_some() {
            return Err(KnowledgeError::InvalidOperatorResponse(
                "draft deletion unexpectedly allocated a response ID".to_string(),
            ));
        }
        self.commit_delete_transaction(
            project,
            transaction_id,
            id.clone(),
            previous.transaction_id.clone(),
        )
    }

    pub fn activate_operator_response(
        &self,
        project: &str,
        id: &OperatorResponseId,
        current_target: &crate::OperatorTargetBinding,
        supersedes: Option<OperatorResponseId>,
    ) -> Result<OperatorResponse, KnowledgeError> {
        validate_project_name(project)?;
        current_target.validate()?;
        let _lock = self.operator_lock(project)?;
        let current = self.replay_operator_responses(project)?;
        let previous = current
            .get(id)
            .ok_or_else(|| KnowledgeError::OperatorResponseNotFound(id.to_string()))?;
        if previous.response.lifecycle != ResponseLifecycle::Draft {
            return Err(KnowledgeError::InvalidResponseTransition(
                "only draft responses can be activated".to_string(),
            ));
        }
        if !previous.response.target.exact_match(current_target) {
            return Err(KnowledgeError::OperatorTargetChanged);
        }
        let superseded = superseded_response_ids(current.values().map(|entry| &entry.response));
        validate_governing_response(
            current
                .values()
                .map(|entry| &entry.response)
                .filter(|response| response.id != *id && !superseded.contains(&response.id)),
            current_target,
            &previous.response.payload,
            supersedes.as_ref(),
        )?;
        let (transaction_id, response_id) = self.allocate_operator_ids(project, false)?;
        if response_id.is_some() {
            return Err(KnowledgeError::InvalidOperatorResponse(
                "draft activation unexpectedly allocated a response ID".to_string(),
            ));
        }
        let mut response = previous.response.clone();
        response.lifecycle = ResponseLifecycle::Active;
        response.supersedes = supersedes;
        response.audit = ResponseAuditMetadata {
            transaction_id: transaction_id.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        response.validate()?;
        self.commit_response_transaction(
            project,
            transaction_id,
            OperatorHistoryOperation::Activate,
            response.clone(),
            Some(previous.transaction_id.clone()),
        )?;
        Ok(response)
    }

    pub fn retire_operator_response(
        &self,
        project: &str,
        id: &OperatorResponseId,
    ) -> Result<OperatorResponse, KnowledgeError> {
        self.transition_active_response(
            project,
            id,
            ResponseLifecycle::Retired,
            OperatorHistoryOperation::Retire,
        )
    }

    pub fn withdraw_operator_response(
        &self,
        project: &str,
        id: &OperatorResponseId,
    ) -> Result<OperatorResponse, KnowledgeError> {
        self.transition_active_response(
            project,
            id,
            ResponseLifecycle::Withdrawn,
            OperatorHistoryOperation::Withdraw,
        )
    }

    pub fn reaffirm_operator_response(
        &self,
        project: &str,
        id: &OperatorResponseId,
        current_target: crate::OperatorTargetBinding,
    ) -> Result<OperatorResponse, KnowledgeError> {
        validate_project_name(project)?;
        current_target.validate()?;
        let _lock = self.operator_lock(project)?;
        let current = self.replay_operator_responses(project)?;
        let previous = current
            .get(id)
            .ok_or_else(|| KnowledgeError::OperatorResponseNotFound(id.to_string()))?;
        if previous.response.lifecycle == ResponseLifecycle::Draft {
            return Err(KnowledgeError::InvalidResponseTransition(
                "draft responses must be activated, not reaffirmed".to_string(),
            ));
        }
        let superseded = superseded_response_ids(current.values().map(|entry| &entry.response));
        if superseded.contains(id) {
            return Err(KnowledgeError::InvalidResponseTransition(
                "a superseded response cannot be reaffirmed".to_string(),
            ));
        }
        if previous.response.payload.is_governing() {
            validate_governing_response(
                current
                    .values()
                    .map(|entry| &entry.response)
                    .filter(|response| !superseded.contains(&response.id)),
                &current_target,
                &previous.response.payload,
                Some(id),
            )?;
        }
        let (transaction_id, response_id) = self.allocate_operator_ids(project, true)?;
        let mut response = previous.response.clone();
        response.id = response_id.ok_or_else(|| {
            KnowledgeError::InvalidOperatorResponse(
                "reaffirmation did not allocate a response ID".to_string(),
            )
        })?;
        response.target = current_target;
        response.lifecycle = ResponseLifecycle::Active;
        response.supersedes = Some(id.clone());
        response.audit = ResponseAuditMetadata {
            transaction_id: transaction_id.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        response.validate()?;
        self.commit_response_transaction(
            project,
            transaction_id,
            OperatorHistoryOperation::Reaffirm,
            response.clone(),
            None,
        )?;
        Ok(response)
    }

    pub fn list_operator_responses(
        &self,
        project: &str,
    ) -> Result<Vec<OperatorResponse>, KnowledgeError> {
        validate_project_name(project)?;
        let current = self.replay_operator_responses(project)?;
        let superseded = current
            .values()
            .filter_map(|entry| entry.response.supersedes.clone())
            .collect::<Vec<_>>();
        let mut responses = current
            .into_values()
            .map(|entry| entry.response)
            .collect::<Vec<_>>();
        for response in &mut responses {
            if superseded.contains(&response.id) && response.lifecycle == ResponseLifecycle::Active
            {
                response.lifecycle = ResponseLifecycle::Superseded;
            }
        }
        responses.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(responses)
    }

    pub fn get_operator_response(
        &self,
        project: &str,
        id: &OperatorResponseId,
    ) -> Result<Option<OperatorResponse>, KnowledgeError> {
        Ok(self
            .list_operator_responses(project)?
            .into_iter()
            .find(|response| response.id == *id))
    }

    pub fn operator_response_history(
        &self,
        project: &str,
    ) -> Result<Vec<OperatorResponseHistoryEntry>, KnowledgeError> {
        validate_project_name(project)?;
        let transactions = self.committed_operator_transactions(project)?;
        transactions
            .into_iter()
            .map(|manifest| {
                let response = match &manifest.mutation {
                    TransactionMutation::Put { response_id } => Some(
                        self.read_response_revision(
                            project,
                            response_id,
                            &manifest.transaction_id,
                        )?
                        .response,
                    ),
                    TransactionMutation::DeleteDraft { .. } => None,
                };
                Ok(OperatorResponseHistoryEntry {
                    transaction_id: manifest.transaction_id,
                    operation: manifest.operation,
                    response,
                })
            })
            .collect()
    }

    fn operator_lock(&self, project: &str) -> Result<OperatorStoreLock, KnowledgeError> {
        let directory = self.operator_transactions_path(project)?;
        fs::create_dir_all(&directory).map_err(|source| KnowledgeError::Io {
            path: directory.clone(),
            source,
        })?;
        let path = directory.join(".lock");
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(mut file) => {
                file.write_all(b"operator transaction lock\n")
                    .map_err(|source| KnowledgeError::Io {
                        path: path.clone(),
                        source,
                    })?;
                file.sync_all().map_err(|source| KnowledgeError::Io {
                    path: path.clone(),
                    source,
                })?;
                Ok(OperatorStoreLock { path })
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Err(KnowledgeError::OperatorStoreBusy)
            }
            Err(source) => Err(KnowledgeError::Io { path, source }),
        }
    }

    fn allocate_operator_ids(
        &self,
        project: &str,
        allocate_response: bool,
    ) -> Result<(OperatorTransactionId, Option<OperatorResponseId>), KnowledgeError> {
        let path = self.operator_sequence_path(project)?;
        let mut sequence = if path.exists() {
            self.read_versioned::<OperatorSequenceState>(&path, OPERATOR_RESPONSE_SCHEMA_VERSION)?
        } else {
            OperatorSequenceState::default()
        };
        sequence.last_transaction = sequence.last_transaction.checked_add(1).ok_or_else(|| {
            KnowledgeError::InvalidOperatorResponse(
                "operator transaction sequence exhausted".to_string(),
            )
        })?;
        let response_id = if allocate_response {
            sequence.last_response = sequence.last_response.checked_add(1).ok_or_else(|| {
                KnowledgeError::InvalidOperatorResponse(
                    "operator response sequence exhausted".to_string(),
                )
            })?;
            Some(OperatorResponseId::from_sequence(sequence.last_response))
        } else {
            None
        };
        self.write_json_atomic(&path, &sequence)?;
        Ok((
            OperatorTransactionId::from_sequence(sequence.last_transaction),
            response_id,
        ))
    }

    fn commit_response_transaction(
        &self,
        project: &str,
        transaction_id: OperatorTransactionId,
        operation: OperatorHistoryOperation,
        response: OperatorResponse,
        expected_transaction: Option<OperatorTransactionId>,
    ) -> Result<(), KnowledgeError> {
        let manifest = TransactionManifest {
            schema_version: OPERATOR_RESPONSE_SCHEMA_VERSION,
            transaction_id: transaction_id.clone(),
            project: project.to_string(),
            sequence: parse_sequence(transaction_id.as_str())?,
            operation,
            mutation: TransactionMutation::Put {
                response_id: response.id.clone(),
            },
            expected_transaction,
        };
        let transaction_path = self.operator_transaction_path(project, &transaction_id)?;
        self.write_json_atomic(&transaction_path.join("manifest.json"), &manifest)?;
        let revision = StoredResponseRevision {
            schema_version: OPERATOR_RESPONSE_SCHEMA_VERSION,
            transaction_id: transaction_id.clone(),
            response,
        };
        let revision_path = self.operator_response_revision_path(
            project,
            manifest.mutation.response_id(),
            &transaction_id,
        )?;
        self.write_json_atomic(&revision_path, &revision)?;
        self.write_json_atomic(
            &transaction_path.join("commit.json"),
            &TransactionCommit {
                schema_version: OPERATOR_RESPONSE_SCHEMA_VERSION,
                transaction_id,
            },
        )
    }

    fn commit_delete_transaction(
        &self,
        project: &str,
        transaction_id: OperatorTransactionId,
        response_id: OperatorResponseId,
        expected_transaction: OperatorTransactionId,
    ) -> Result<(), KnowledgeError> {
        let manifest = TransactionManifest {
            schema_version: OPERATOR_RESPONSE_SCHEMA_VERSION,
            transaction_id: transaction_id.clone(),
            project: project.to_string(),
            sequence: parse_sequence(transaction_id.as_str())?,
            operation: OperatorHistoryOperation::DeleteDraft,
            mutation: TransactionMutation::DeleteDraft { response_id },
            expected_transaction: Some(expected_transaction),
        };
        let transaction_path = self.operator_transaction_path(project, &transaction_id)?;
        self.write_json_atomic(&transaction_path.join("manifest.json"), &manifest)?;
        self.write_json_atomic(
            &transaction_path.join("commit.json"),
            &TransactionCommit {
                schema_version: OPERATOR_RESPONSE_SCHEMA_VERSION,
                transaction_id,
            },
        )
    }

    fn transition_active_response(
        &self,
        project: &str,
        id: &OperatorResponseId,
        lifecycle: ResponseLifecycle,
        operation: OperatorHistoryOperation,
    ) -> Result<OperatorResponse, KnowledgeError> {
        validate_project_name(project)?;
        let _lock = self.operator_lock(project)?;
        let current = self.replay_operator_responses(project)?;
        let previous = current
            .get(id)
            .ok_or_else(|| KnowledgeError::OperatorResponseNotFound(id.to_string()))?;
        let superseded = superseded_response_ids(current.values().map(|entry| &entry.response));
        if previous.response.lifecycle != ResponseLifecycle::Active || superseded.contains(id) {
            return Err(KnowledgeError::InvalidResponseTransition(
                "only an effective active response may transition".to_string(),
            ));
        }
        let (transaction_id, response_id) = self.allocate_operator_ids(project, true)?;
        let mut response = previous.response.clone();
        response.id = response_id.ok_or_else(|| {
            KnowledgeError::InvalidOperatorResponse(
                "lifecycle transition did not allocate a response ID".to_string(),
            )
        })?;
        response.lifecycle = lifecycle;
        response.supersedes = Some(id.clone());
        response.audit = ResponseAuditMetadata {
            transaction_id: transaction_id.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        response.validate()?;
        self.commit_response_transaction(
            project,
            transaction_id,
            operation,
            response.clone(),
            None,
        )?;
        Ok(response)
    }

    fn replay_operator_responses(
        &self,
        project: &str,
    ) -> Result<BTreeMap<OperatorResponseId, CurrentResponse>, KnowledgeError> {
        let mut current = BTreeMap::new();
        for manifest in self.committed_operator_transactions(project)? {
            let response_id = manifest.mutation.response_id().clone();
            let actual_previous = current
                .get(&response_id)
                .map(|entry: &CurrentResponse| entry.transaction_id.clone());
            if actual_previous != manifest.expected_transaction {
                return Err(KnowledgeError::Corrupt {
                    path: self.operator_transaction_path(project, &manifest.transaction_id)?,
                    message: "operator transaction expected state does not match history"
                        .to_string(),
                });
            }
            match manifest.mutation {
                TransactionMutation::Put { response_id } => {
                    let revision = self.read_response_revision(
                        project,
                        &response_id,
                        &manifest.transaction_id,
                    )?;
                    revision.response.validate()?;
                    if revision.response.id != response_id
                        || revision.response.project != project
                        || revision.transaction_id != manifest.transaction_id
                        || revision.response.audit.transaction_id != manifest.transaction_id
                    {
                        return Err(KnowledgeError::Corrupt {
                            path: self
                                .operator_transaction_path(project, &manifest.transaction_id)?,
                            message: "operator response revision does not match transaction"
                                .to_string(),
                        });
                    }
                    current.insert(
                        response_id,
                        CurrentResponse {
                            response: revision.response,
                            transaction_id: manifest.transaction_id,
                        },
                    );
                }
                TransactionMutation::DeleteDraft { response_id } => {
                    current.remove(&response_id);
                }
            }
        }
        Ok(current)
    }

    fn committed_operator_transactions(
        &self,
        project: &str,
    ) -> Result<Vec<TransactionManifest>, KnowledgeError> {
        let directory = self.operator_transactions_path(project)?;
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut directories = fs::read_dir(&directory)
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
        directories.sort();
        let mut manifests = Vec::new();
        for transaction_path in directories {
            if !transaction_path.is_dir() || !transaction_path.join("commit.json").exists() {
                continue;
            }
            let manifest = self.read_versioned::<TransactionManifest>(
                &transaction_path.join("manifest.json"),
                OPERATOR_RESPONSE_SCHEMA_VERSION,
            )?;
            let commit = self.read_versioned::<TransactionCommit>(
                &transaction_path.join("commit.json"),
                OPERATOR_RESPONSE_SCHEMA_VERSION,
            )?;
            if commit.transaction_id != manifest.transaction_id
                || manifest.project != project
                || manifest.sequence != parse_sequence(manifest.transaction_id.as_str())?
            {
                return Err(KnowledgeError::Corrupt {
                    path: transaction_path,
                    message: "operator transaction manifest and commit do not match".to_string(),
                });
            }
            manifests.push(manifest);
        }
        manifests.sort_by(|left, right| {
            left.sequence
                .cmp(&right.sequence)
                .then_with(|| left.transaction_id.cmp(&right.transaction_id))
        });
        Ok(manifests)
    }

    fn read_response_revision(
        &self,
        project: &str,
        response_id: &OperatorResponseId,
        transaction_id: &OperatorTransactionId,
    ) -> Result<StoredResponseRevision, KnowledgeError> {
        let path = self.operator_response_revision_path(project, response_id, transaction_id)?;
        self.read_versioned(&path, OPERATOR_RESPONSE_SCHEMA_VERSION)
    }

    fn read_versioned<T: for<'de> Deserialize<'de>>(
        &self,
        path: &Path,
        supported: u32,
    ) -> Result<T, KnowledgeError> {
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
        if schema_version != supported {
            return Err(KnowledgeError::UnsupportedOperatorResponseSchema {
                found: schema_version,
                supported,
            });
        }
        serde_json::from_value(raw).map_err(|source| KnowledgeError::Serialization {
            path: path.to_path_buf(),
            source,
        })
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

    fn operator_project_path(&self, project: &str) -> Result<PathBuf, KnowledgeError> {
        validate_project_name(project)?;
        Ok(self.root.join("projects").join(project))
    }

    fn operator_transactions_path(&self, project: &str) -> Result<PathBuf, KnowledgeError> {
        Ok(self.operator_project_path(project)?.join("transactions"))
    }

    fn operator_transaction_path(
        &self,
        project: &str,
        transaction_id: &OperatorTransactionId,
    ) -> Result<PathBuf, KnowledgeError> {
        transaction_id.validate()?;
        Ok(self
            .operator_transactions_path(project)?
            .join(transaction_id.as_str()))
    }

    fn operator_sequence_path(&self, project: &str) -> Result<PathBuf, KnowledgeError> {
        Ok(self
            .operator_transactions_path(project)?
            .join("sequence.json"))
    }

    fn operator_response_revision_path(
        &self,
        project: &str,
        response_id: &OperatorResponseId,
        transaction_id: &OperatorTransactionId,
    ) -> Result<PathBuf, KnowledgeError> {
        response_id.validate()?;
        transaction_id.validate()?;
        Ok(self
            .operator_project_path(project)?
            .join("operator-responses")
            .join(response_id.as_str())
            .join(format!("{}.json", transaction_id.as_str())))
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

fn parse_sequence(id: &str) -> Result<u64, KnowledgeError> {
    id.rsplit('-')
        .next()
        .and_then(|digits| digits.parse::<u64>().ok())
        .filter(|sequence| *sequence > 0)
        .ok_or_else(|| {
            KnowledgeError::InvalidOperatorResponse(format!(
                "identifier has no valid sequence: {id}"
            ))
        })
}

fn superseded_response_ids<'a>(
    responses: impl Iterator<Item = &'a OperatorResponse>,
) -> Vec<OperatorResponseId> {
    let mut ids = responses
        .filter_map(|response| response.supersedes.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

fn validate_governing_response<'a>(
    responses: impl Iterator<Item = &'a OperatorResponse>,
    target: &crate::OperatorTargetBinding,
    payload: &crate::OperatorResponsePayload,
    supersedes: Option<&OperatorResponseId>,
) -> Result<(), KnowledgeError> {
    if payload.is_annotation() {
        if supersedes.is_some() {
            return Err(KnowledgeError::InvalidOperatorResponse(
                "annotations coexist and cannot supersede governing responses".to_string(),
            ));
        }
        return Ok(());
    }
    let active = responses
        .filter(|response| {
            response.lifecycle == ResponseLifecycle::Active
                && response.payload.is_governing()
                && response.target.governing_key() == target.governing_key()
        })
        .collect::<Vec<_>>();
    match (active.as_slice(), supersedes) {
        ([], None) => Ok(()),
        ([], Some(id)) => Err(KnowledgeError::InvalidResponseTransition(format!(
            "superseded response is not active for this target: {id}"
        ))),
        ([existing], Some(id)) if existing.id == *id => Ok(()),
        ([existing], _) => Err(KnowledgeError::GoverningResponseConflict(
            existing.id.to_string(),
        )),
        _ => Err(KnowledgeError::GoverningResponseConflict(
            target.governing_key().to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        confidence::Confidence,
        evidence::{Evidence, EvidenceKind},
        operator::{
            AcceptancePayload, AnnotationPayload, AnnotationScope, CorrectionPayload,
            DisputePayload, NewOperatorResponse, OperatorConfidence, OperatorIdentity,
            OperatorIntent, OperatorResponsePayload, OperatorTargetBinding,
            OperatorTargetClassification, OperatorTargetKind,
        },
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

    fn operator_target() -> OperatorTargetBinding {
        OperatorTargetBinding::new(
            "pi-insight-v1-demo",
            OperatorTargetKind::Insight,
            OperatorTargetClassification::Derived,
            Some("PI-006".to_string()),
            "The project has multiple workspace packages.",
            vec!["context_field:workspace.packages".to_string()],
            vec!["pi-entity-v1-package".to_string()],
        )
        .expect("operator target")
    }

    fn operator_request(payload: OperatorResponsePayload) -> NewOperatorResponse {
        NewOperatorResponse::new(
            "demo",
            operator_target(),
            OperatorIdentity::local("Local operator").expect("operator"),
            payload,
        )
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

    #[test]
    fn operator_transactions_allocate_stable_ids_and_replay_committed_revisions() {
        let (directory, store) = store();
        let annotation = store
            .create_operator_response(operator_request(OperatorResponsePayload::Annotation(
                AnnotationPayload {
                    statement: "Implementation boundary context.".to_string(),
                    intent: OperatorIntent::Context,
                    scope: AnnotationScope::Persistent,
                    confidence: None,
                },
            )))
            .expect("annotation");
        let acceptance = store
            .create_operator_response(operator_request(OperatorResponsePayload::Acceptance(
                AcceptancePayload {
                    reason: Some("Evidence is sufficient.".to_string()),
                    confidence: None,
                },
            )))
            .expect("acceptance");

        assert_eq!(annotation.id.as_str(), "or-response-v1-000001");
        assert_eq!(acceptance.id.as_str(), "or-response-v1-000002");
        assert_eq!(
            annotation.audit.transaction_id.as_str(),
            "or-transaction-v1-000001"
        );
        let first = store.list_operator_responses("demo").expect("responses");
        let second = store.list_operator_responses("demo").expect("responses");
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert_eq!(
            store
                .operator_response_history("demo")
                .expect("history")
                .len(),
            2
        );
        assert!(directory
            .path()
            .join("projects/demo/transactions/or-transaction-v1-000001/commit.json")
            .exists());
        assert!(!directory
            .path()
            .join("projects/demo/transactions/.lock")
            .exists());
    }

    #[test]
    fn prepared_transactions_are_ignored_and_remain_recoverable() {
        let (directory, store) = store();
        let prepared = directory
            .path()
            .join("projects/demo/transactions/or-transaction-v1-999999");
        fs::create_dir_all(&prepared).expect("prepared directory");
        fs::write(prepared.join("manifest.json"), b"{\"incomplete\":true}\n")
            .expect("prepared manifest");

        assert!(store
            .list_operator_responses("demo")
            .expect("responses")
            .is_empty());
        assert!(prepared.join("manifest.json").exists());
    }

    #[test]
    fn draft_edits_activation_and_deletion_preserve_transaction_history() {
        let (_directory, store) = store();
        let draft = store
            .create_operator_response(operator_request(OperatorResponsePayload::Correction(
                CorrectionPayload {
                    replacement_statement: "One integrated system.".to_string(),
                    reason: None,
                    intent: OperatorIntent::Architecture,
                    confidence: None,
                },
            )))
            .expect("draft");
        assert_eq!(draft.lifecycle, ResponseLifecycle::Draft);

        let edited = store
            .edit_operator_response_draft(
                "demo",
                &draft.id,
                OperatorResponsePayload::Dispute(DisputePayload {
                    reason: "More evidence is required.".to_string(),
                    intent: Some(OperatorIntent::Context),
                    confidence: Some(OperatorConfidence::Tentative),
                }),
            )
            .expect("edited draft");
        assert_eq!(edited.id, draft.id);
        assert_ne!(edited.audit.transaction_id, draft.audit.transaction_id);

        let changed_target = OperatorTargetBinding::new(
            "pi-insight-v1-changed",
            OperatorTargetKind::Insight,
            OperatorTargetClassification::Derived,
            Some("PI-006".to_string()),
            "Changed interpretation.",
            vec!["context_field:changed".to_string()],
            Vec::new(),
        )
        .expect("changed target");
        assert!(matches!(
            store.activate_operator_response("demo", &draft.id, &changed_target, None),
            Err(KnowledgeError::OperatorTargetChanged)
        ));
        let active = store
            .activate_operator_response("demo", &draft.id, &operator_target(), None)
            .expect("active response");
        assert_eq!(active.id, draft.id);
        assert_eq!(active.lifecycle, ResponseLifecycle::Active);
        assert_eq!(
            store
                .operator_response_history("demo")
                .expect("history")
                .len(),
            3
        );

        let disposable = store
            .create_operator_response(
                operator_request(OperatorResponsePayload::Correction(CorrectionPayload {
                    replacement_statement: "Temporary draft.".to_string(),
                    reason: None,
                    intent: OperatorIntent::Experiment,
                    confidence: None,
                }))
                .with_supersedes(active.id),
            )
            .expect("disposable draft");
        store
            .delete_operator_response_draft("demo", &disposable.id)
            .expect("delete draft");
        assert!(store
            .get_operator_response("demo", &disposable.id)
            .expect("response")
            .is_none());
        assert!(store
            .operator_response_history("demo")
            .expect("history")
            .iter()
            .any(|entry| entry.operation == OperatorHistoryOperation::DeleteDraft));
    }

    #[test]
    fn active_governance_requires_explicit_supersession_and_preserves_old_response() {
        let (_directory, store) = store();
        let accepted = store
            .create_operator_response(operator_request(OperatorResponsePayload::Acceptance(
                AcceptancePayload {
                    reason: None,
                    confidence: Some(OperatorConfidence::High),
                },
            )))
            .expect("acceptance");
        assert!(matches!(
            store.create_operator_response(operator_request(OperatorResponsePayload::Acceptance(
                AcceptancePayload {
                    reason: Some("Second opinion.".to_string()),
                    confidence: None,
                }
            ))),
            Err(KnowledgeError::GoverningResponseConflict(_))
        ));

        let replacement = store
            .create_operator_response(
                operator_request(OperatorResponsePayload::Acceptance(AcceptancePayload {
                    reason: Some("Reaffirmed.".to_string()),
                    confidence: Some(OperatorConfidence::Certain),
                }))
                .with_supersedes(accepted.id.clone()),
            )
            .expect("replacement");
        let responses = store.list_operator_responses("demo").expect("responses");
        assert_eq!(responses.len(), 2);
        assert_eq!(
            responses
                .iter()
                .find(|response| response.id == accepted.id)
                .expect("old response")
                .lifecycle,
            ResponseLifecycle::Superseded
        );
        assert_eq!(replacement.lifecycle, ResponseLifecycle::Active);

        let retired = store
            .retire_operator_response("demo", &replacement.id)
            .expect("retired");
        assert_eq!(retired.lifecycle, ResponseLifecycle::Retired);
        assert_eq!(retired.supersedes, Some(replacement.id));
    }

    #[test]
    fn reaffirmation_rebinds_without_rewriting_history() {
        let (_directory, store) = store();
        let accepted = store
            .create_operator_response(operator_request(OperatorResponsePayload::Acceptance(
                AcceptancePayload {
                    reason: None,
                    confidence: None,
                },
            )))
            .expect("acceptance");
        let new_target = OperatorTargetBinding::new(
            "pi-insight-v1-new",
            OperatorTargetKind::Insight,
            OperatorTargetClassification::Derived,
            Some("PI-006".to_string()),
            "Updated package interpretation.",
            vec!["context_field:workspace.packages.updated".to_string()],
            vec!["pi-entity-v1-package".to_string()],
        )
        .expect("new target");
        let reaffirmed = store
            .reaffirm_operator_response("demo", &accepted.id, new_target.clone())
            .expect("reaffirmed");
        assert_ne!(reaffirmed.id, accepted.id);
        assert_eq!(reaffirmed.target, new_target);
        assert_eq!(reaffirmed.supersedes, Some(accepted.id));
        assert_eq!(
            store
                .operator_response_history("demo")
                .expect("history")
                .len(),
            2
        );
    }
}
