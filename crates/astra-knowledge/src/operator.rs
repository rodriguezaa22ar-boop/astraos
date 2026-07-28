use crate::KnowledgeError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

pub const OPERATOR_RESPONSE_SCHEMA_VERSION: u32 = 1;
const RESPONSE_ID_PREFIX: &str = "or-response-v1-";
const TRANSACTION_ID_PREFIX: &str = "or-transaction-v1-";
const OPERATOR_ID_PREFIX: &str = "or-operator-v1-";
const MAX_TEXT_LENGTH: usize = 4_096;

macro_rules! sequence_id {
    ($name:ident, $prefix:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub(crate) fn from_sequence(sequence: u64) -> Self {
                Self(format!("{}{:06}", $prefix, sequence))
            }

            pub fn parse(value: impl Into<String>) -> Result<Self, KnowledgeError> {
                let value = value.into();
                let digits = value.strip_prefix($prefix).ok_or_else(|| {
                    KnowledgeError::InvalidOperatorResponse(format!("invalid identifier: {value}"))
                })?;
                if digits.len() < 6
                    || !digits.bytes().all(|byte| byte.is_ascii_digit())
                    || digits
                        .parse::<u64>()
                        .ok()
                        .filter(|value| *value > 0)
                        .is_none()
                {
                    return Err(KnowledgeError::InvalidOperatorResponse(format!(
                        "invalid identifier: {value}"
                    )));
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub(crate) fn validate(&self) -> Result<(), KnowledgeError> {
                Self::parse(self.0.clone()).map(|_| ())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

sequence_id!(OperatorResponseId, RESPONSE_ID_PREFIX);
sequence_id!(OperatorTransactionId, TRANSACTION_ID_PREFIX);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperatorId(String);

impl OperatorId {
    fn local() -> Self {
        Self(format!("{OPERATOR_ID_PREFIX}local"))
    }

    fn named(stable_key: &str) -> Self {
        let mut hasher = Sha256::new();
        write_field(&mut hasher, "version", b"operator-id-v1");
        write_field(&mut hasher, "kind", b"named_operator");
        write_field(&mut hasher, "stable_key", stable_key.as_bytes());
        Self(format!("{OPERATOR_ID_PREFIX}{:x}", hasher.finalize()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OperatorId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorIdentityKind {
    LocalOperator,
    NamedOperator,
    Team,
    Service,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorIdentity {
    pub id: OperatorId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stable_key: Option<String>,
    pub display_name: String,
    pub kind: OperatorIdentityKind,
}

impl OperatorIdentity {
    pub fn local(display_name: impl Into<String>) -> Result<Self, KnowledgeError> {
        let display_name = display_name.into();
        validate_identifier("operator display name", &display_name)?;
        Ok(Self {
            id: OperatorId::local(),
            stable_key: None,
            display_name,
            kind: OperatorIdentityKind::LocalOperator,
        })
    }

    pub fn named(
        stable_key: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Result<Self, KnowledgeError> {
        let stable_key = stable_key.into();
        let display_name = display_name.into();
        validate_identifier("operator stable key", &stable_key)?;
        validate_identifier("operator display name", &display_name)?;
        Ok(Self {
            id: OperatorId::named(&stable_key),
            stable_key: Some(stable_key),
            display_name,
            kind: OperatorIdentityKind::NamedOperator,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorConfidence {
    Certain,
    High,
    Medium,
    Tentative,
}

impl OperatorConfidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Certain => "certain",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Tentative => "tentative",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorIntent {
    Architecture,
    Decision,
    Preference,
    TemporaryConstraint,
    Experiment,
    Context,
}

impl OperatorIntent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Architecture => "architecture",
            Self::Decision => "decision",
            Self::Preference => "preference",
            Self::TemporaryConstraint => "temporary_constraint",
            Self::Experiment => "experiment",
            Self::Context => "context",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationScope {
    Persistent,
    StateBound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseLifecycle {
    Draft,
    Active,
    Superseded,
    Retired,
    Withdrawn,
    Expired,
    ReviewRequired,
    Orphaned,
}

impl ResponseLifecycle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Retired => "retired",
            Self::Withdrawn => "withdrawn",
            Self::Expired => "expired",
            Self::ReviewRequired => "review_required",
            Self::Orphaned => "orphaned",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorTargetKind {
    Insight,
    Entity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorTargetClassification {
    Observed,
    Derived,
    OperatorDecided,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorTargetBinding {
    pub target_id: String,
    pub target_kind: OperatorTargetKind,
    pub classification: OperatorTargetClassification,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    pub statement: String,
    pub evidence_fingerprint: String,
    pub evidence_references: Vec<String>,
    pub related_entities: Vec<String>,
}

impl OperatorTargetBinding {
    pub fn new(
        target_id: impl Into<String>,
        target_kind: OperatorTargetKind,
        classification: OperatorTargetClassification,
        rule_id: Option<String>,
        statement: impl Into<String>,
        mut evidence_references: Vec<String>,
        mut related_entities: Vec<String>,
    ) -> Result<Self, KnowledgeError> {
        let target_id = target_id.into();
        let statement = statement.into();
        validate_identifier("target ID", &target_id)?;
        validate_identifier("target statement", &statement)?;
        if let Some(rule_id) = &rule_id {
            validate_identifier("rule ID", rule_id)?;
        }
        if evidence_references.is_empty() {
            return Err(KnowledgeError::InvalidOperatorResponse(
                "target binding requires evidence".to_string(),
            ));
        }
        for evidence in &evidence_references {
            validate_identifier("evidence reference", evidence)?;
        }
        for entity in &related_entities {
            validate_identifier("related entity", entity)?;
        }
        evidence_references.sort();
        evidence_references.dedup();
        related_entities.sort();
        related_entities.dedup();
        let evidence_fingerprint = target_fingerprint(
            &target_id,
            rule_id.as_deref(),
            &statement,
            &evidence_references,
            &related_entities,
        );
        Ok(Self {
            target_id,
            target_kind,
            classification,
            rule_id,
            statement,
            evidence_fingerprint,
            evidence_references,
            related_entities,
        })
    }

    pub fn exact_match(&self, other: &Self) -> bool {
        self == other
    }

    pub fn governing_key(&self) -> &str {
        self.rule_id.as_deref().unwrap_or(&self.target_id)
    }

    pub(crate) fn validate(&self) -> Result<(), KnowledgeError> {
        let rebuilt = Self::new(
            self.target_id.clone(),
            self.target_kind,
            self.classification,
            self.rule_id.clone(),
            self.statement.clone(),
            self.evidence_references.clone(),
            self.related_entities.clone(),
        )?;
        if rebuilt.evidence_fingerprint != self.evidence_fingerprint {
            return Err(KnowledgeError::InvalidOperatorResponse(
                "target evidence fingerprint does not match its binding".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationPayload {
    pub statement: String,
    pub intent: OperatorIntent,
    pub scope: AnnotationScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<OperatorConfidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptancePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<OperatorConfidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectionPayload {
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<OperatorIntent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<OperatorConfidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectionPayload {
    pub replacement_statement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub intent: OperatorIntent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<OperatorConfidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisputePayload {
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<OperatorIntent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<OperatorConfidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "details", rename_all = "snake_case")]
pub enum OperatorResponsePayload {
    Annotation(AnnotationPayload),
    Acceptance(AcceptancePayload),
    Rejection(RejectionPayload),
    Correction(CorrectionPayload),
    Dispute(DisputePayload),
}

impl OperatorResponsePayload {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Annotation(_) => "annotation",
            Self::Acceptance(_) => "acceptance",
            Self::Rejection(_) => "rejection",
            Self::Correction(_) => "correction",
            Self::Dispute(_) => "dispute",
        }
    }

    pub fn initial_lifecycle(&self) -> ResponseLifecycle {
        match self {
            Self::Annotation(_) | Self::Acceptance(_) => ResponseLifecycle::Active,
            Self::Rejection(_) | Self::Correction(_) | Self::Dispute(_) => ResponseLifecycle::Draft,
        }
    }

    pub fn is_annotation(&self) -> bool {
        matches!(self, Self::Annotation(_))
    }

    pub fn is_governing(&self) -> bool {
        !self.is_annotation()
    }

    pub(crate) fn validate(&self) -> Result<(), KnowledgeError> {
        match self {
            Self::Annotation(payload) => validate_text("annotation statement", &payload.statement),
            Self::Acceptance(payload) => {
                validate_optional_text("acceptance reason", payload.reason.as_deref())
            }
            Self::Rejection(payload) => validate_text("rejection reason", &payload.reason),
            Self::Correction(payload) => {
                validate_text("replacement statement", &payload.replacement_statement)?;
                validate_optional_text("correction reason", payload.reason.as_deref())
            }
            Self::Dispute(payload) => validate_text("dispute reason", &payload.reason),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseAuditMetadata {
    pub transaction_id: OperatorTransactionId,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorResponse {
    pub schema_version: u32,
    pub id: OperatorResponseId,
    pub project: String,
    pub target: OperatorTargetBinding,
    pub operator: OperatorIdentity,
    pub lifecycle: ResponseLifecycle,
    pub payload: OperatorResponsePayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<OperatorResponseId>,
    pub audit: ResponseAuditMetadata,
}

impl OperatorResponse {
    pub fn is_effective(&self) -> bool {
        self.lifecycle == ResponseLifecycle::Active
    }

    pub(crate) fn validate(&self) -> Result<(), KnowledgeError> {
        if self.schema_version != OPERATOR_RESPONSE_SCHEMA_VERSION {
            return Err(KnowledgeError::UnsupportedOperatorResponseSchema {
                found: self.schema_version,
                supported: OPERATOR_RESPONSE_SCHEMA_VERSION,
            });
        }
        self.id.validate()?;
        self.audit.transaction_id.validate()?;
        validate_identifier("project", &self.project)?;
        self.target.validate()?;
        self.payload.validate()?;
        if self.payload.is_governing()
            && self.target.classification != OperatorTargetClassification::Derived
        {
            return Err(KnowledgeError::ObservedTargetGovernance);
        }
        if self.supersedes.as_ref() == Some(&self.id) {
            return Err(KnowledgeError::InvalidOperatorResponse(
                "a response cannot supersede itself".to_string(),
            ));
        }
        validate_identifier("operator display name", &self.operator.display_name)?;
        match self.operator.kind {
            OperatorIdentityKind::LocalOperator
                if self.operator.id == OperatorId::local()
                    && self.operator.stable_key.is_none() => {}
            OperatorIdentityKind::NamedOperator => {
                let stable_key = self.operator.stable_key.as_deref().ok_or_else(|| {
                    KnowledgeError::InvalidOperatorResponse(
                        "named operator requires a stable key".to_string(),
                    )
                })?;
                validate_identifier("operator stable key", stable_key)?;
                if self.operator.id != OperatorId::named(stable_key) {
                    return Err(KnowledgeError::InvalidOperatorResponse(
                        "operator ID does not match its stable key".to_string(),
                    ));
                }
            }
            _ => {
                return Err(KnowledgeError::InvalidOperatorResponse(
                    "unsupported or malformed operator identity".to_string(),
                ));
            }
        }
        validate_identifier("created_at", &self.audit.created_at)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOperatorResponse {
    pub project: String,
    pub target: OperatorTargetBinding,
    pub operator: OperatorIdentity,
    pub payload: OperatorResponsePayload,
    pub supersedes: Option<OperatorResponseId>,
}

impl NewOperatorResponse {
    pub fn new(
        project: impl Into<String>,
        target: OperatorTargetBinding,
        operator: OperatorIdentity,
        payload: OperatorResponsePayload,
    ) -> Self {
        Self {
            project: project.into(),
            target,
            operator,
            payload,
            supersedes: None,
        }
    }

    pub fn with_supersedes(mut self, supersedes: OperatorResponseId) -> Self {
        self.supersedes = Some(supersedes);
        self
    }
}

fn target_fingerprint(
    target_id: &str,
    rule_id: Option<&str>,
    statement: &str,
    evidence: &[String],
    related_entities: &[String],
) -> String {
    let mut hasher = Sha256::new();
    write_field(&mut hasher, "version", b"operator-target-v1");
    write_field(&mut hasher, "target_id", target_id.as_bytes());
    write_field(
        &mut hasher,
        "rule_id",
        rule_id.unwrap_or_default().as_bytes(),
    );
    write_field(&mut hasher, "statement", statement.as_bytes());
    for item in evidence {
        write_field(&mut hasher, "evidence", item.as_bytes());
    }
    for entity in related_entities {
        write_field(&mut hasher, "related_entity", entity.as_bytes());
    }
    format!("or-target-v1-{:x}", hasher.finalize())
}

fn write_field(hasher: &mut Sha256, label: &str, value: &[u8]) {
    hasher.update((label.len() as u64).to_be_bytes());
    hasher.update(label.as_bytes());
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn validate_optional_text(label: &str, value: Option<&str>) -> Result<(), KnowledgeError> {
    value.map_or(Ok(()), |value| validate_text(label, value))
}

fn validate_identifier(label: &str, value: &str) -> Result<(), KnowledgeError> {
    if value.trim().is_empty()
        || value.len() > MAX_TEXT_LENGTH
        || value.chars().any(char::is_control)
        || contains_absolute_path(value)
    {
        return Err(KnowledgeError::InvalidOperatorResponse(format!(
            "{label} is invalid"
        )));
    }
    Ok(())
}

fn validate_text(label: &str, value: &str) -> Result<(), KnowledgeError> {
    validate_identifier(label, value)?;
    let normalized = value.to_ascii_lowercase().replace(['-', ' '], "_");
    let compact = normalized.replace('_', "");
    if [
        "password",
        "token",
        "secret",
        "authorization",
        "privatekey",
        "apikey",
        "accesstoken",
        "refreshtoken",
    ]
    .iter()
    .any(|term| compact.contains(term))
    {
        return Err(KnowledgeError::SensitiveOperatorResponse(label.to_string()));
    }
    Ok(())
}

fn contains_absolute_path(value: &str) -> bool {
    value.starts_with('/')
        || value.contains(" /")
        || value.as_bytes().windows(3).any(|window| {
            window[0].is_ascii_alphabetic() && window[1] == b':' && window[2] == b'\\'
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(classification: OperatorTargetClassification) -> OperatorTargetBinding {
        OperatorTargetBinding::new(
            "pi-insight-v1-demo",
            OperatorTargetKind::Insight,
            classification,
            Some("PI-006".to_string()),
            "Multiple workspace packages.",
            vec!["context_field:workspace.packages".to_string()],
            vec!["pi-entity-v1-package".to_string()],
        )
        .expect("target")
    }

    #[test]
    fn identities_are_stable_independent_of_display_name() {
        let first = OperatorIdentity::named("alice", "Alice").expect("identity");
        let renamed = OperatorIdentity::named("alice", "A. Operator").expect("identity");
        assert_eq!(first.id, renamed.id);
        assert_ne!(first.display_name, renamed.display_name);
        assert_eq!(
            OperatorIdentity::local("Local").expect("local").id,
            OperatorIdentity::local("Developer").expect("local").id
        );
    }

    #[test]
    fn target_fingerprints_are_deterministic_and_order_independent() {
        let first = target(OperatorTargetClassification::Derived);
        let second = OperatorTargetBinding::new(
            first.target_id.clone(),
            first.target_kind,
            first.classification,
            first.rule_id.clone(),
            first.statement.clone(),
            first.evidence_references.iter().rev().cloned().collect(),
            first.related_entities.iter().rev().cloned().collect(),
        )
        .expect("target");
        assert_eq!(first, second);
    }

    #[test]
    fn payloads_have_typed_lifecycle_and_observations_cannot_be_governed() {
        let correction = OperatorResponsePayload::Correction(CorrectionPayload {
            replacement_statement: "One integrated system.".to_string(),
            reason: None,
            intent: OperatorIntent::Architecture,
            confidence: Some(OperatorConfidence::High),
        });
        assert_eq!(correction.initial_lifecycle(), ResponseLifecycle::Draft);
        let response = OperatorResponse {
            schema_version: OPERATOR_RESPONSE_SCHEMA_VERSION,
            id: OperatorResponseId::from_sequence(1),
            project: "demo".to_string(),
            target: target(OperatorTargetClassification::Observed),
            operator: OperatorIdentity::local("Local operator").expect("operator"),
            lifecycle: ResponseLifecycle::Draft,
            payload: correction,
            supersedes: None,
            audit: ResponseAuditMetadata {
                transaction_id: OperatorTransactionId::from_sequence(1),
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
        };
        assert!(matches!(
            response.validate(),
            Err(KnowledgeError::ObservedTargetGovernance)
        ));
    }

    #[test]
    fn obvious_sensitive_text_and_absolute_paths_are_rejected() {
        assert!(matches!(
            OperatorResponsePayload::Annotation(AnnotationPayload {
                statement: "api token".to_string(),
                intent: OperatorIntent::Context,
                scope: AnnotationScope::Persistent,
                confidence: None,
            })
            .validate(),
            Err(KnowledgeError::SensitiveOperatorResponse(_))
        ));
        assert!(OperatorTargetBinding::new(
            "pi-insight-v1-demo",
            OperatorTargetKind::Insight,
            OperatorTargetClassification::Derived,
            None,
            "See /Users/example/private",
            vec!["context_field:demo".to_string()],
            Vec::new(),
        )
        .is_err());
    }
}
