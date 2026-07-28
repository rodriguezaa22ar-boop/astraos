use crate::{
    OperatorResponse, OperatorResponseId, OperatorTransactionId, OPERATOR_RESPONSE_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorHistoryOperation {
    Create,
    EditDraft,
    Activate,
    DeleteDraft,
    Retire,
    Withdraw,
    Reaffirm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorResponseHistoryEntry {
    pub transaction_id: OperatorTransactionId,
    pub operation: OperatorHistoryOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<OperatorResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransactionManifest {
    pub schema_version: u32,
    pub transaction_id: OperatorTransactionId,
    pub project: String,
    pub sequence: u64,
    pub operation: OperatorHistoryOperation,
    pub mutation: TransactionMutation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_transaction: Option<OperatorTransactionId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum TransactionMutation {
    Put { response_id: OperatorResponseId },
    DeleteDraft { response_id: OperatorResponseId },
}

impl TransactionMutation {
    pub fn response_id(&self) -> &OperatorResponseId {
        match self {
            Self::Put { response_id } | Self::DeleteDraft { response_id } => response_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoredResponseRevision {
    pub schema_version: u32,
    pub transaction_id: OperatorTransactionId,
    pub response: OperatorResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransactionCommit {
    pub schema_version: u32,
    pub transaction_id: OperatorTransactionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OperatorSequenceState {
    pub schema_version: u32,
    pub last_transaction: u64,
    pub last_response: u64,
}

impl Default for OperatorSequenceState {
    fn default() -> Self {
        Self {
            schema_version: OPERATOR_RESPONSE_SCHEMA_VERSION,
            last_transaction: 0,
            last_response: 0,
        }
    }
}
