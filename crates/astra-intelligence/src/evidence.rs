/// Safe references to structured inputs used to explain intelligence output.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "source")]
pub enum IntelligenceEvidenceRef {
    ContextField { field: String },
    ContextPackage { package: String },
    Action { action_id: String },
    ExecutionCapability { action_id: String },
    KnowledgeClaim { claim_id: String },
    Verification { claim_id: String },
    Input { field: String },
}

impl IntelligenceEvidenceRef {
    pub(crate) fn context(field: impl Into<String>) -> Self {
        Self::ContextField {
            field: field.into(),
        }
    }

    pub fn canonical_key(&self) -> String {
        match self {
            Self::ContextField { field } => format!("context_field:{field}"),
            Self::ContextPackage { package } => format!("context_package:{package}"),
            Self::Action { action_id } => format!("action:{action_id}"),
            Self::ExecutionCapability { action_id } => format!("capability:{action_id}"),
            Self::KnowledgeClaim { claim_id } => format!("knowledge_claim:{claim_id}"),
            Self::Verification { claim_id } => format!("verification:{claim_id}"),
            Self::Input { field } => format!("input:{field}"),
        }
    }
}
