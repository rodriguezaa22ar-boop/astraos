use crate::{
    confidence::Confidence,
    error::KnowledgeError,
    evidence::Evidence,
    validity::{Validity, ValidityCondition},
    KnowledgeCategory,
};
use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

/// Stable identifier for one category/subject/predicate/value claim.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct KnowledgeId(String);

impl<'de> Deserialize<'de> for KnowledgeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(D::Error::custom)
    }
}

impl KnowledgeId {
    pub fn derive(
        category: KnowledgeCategory,
        subject: &str,
        predicate: &str,
        value: &serde_json::Value,
    ) -> Result<Self, KnowledgeError> {
        let canonical = canonical_json(value)?;
        let mut hasher = Sha256::new();
        write_field(&mut hasher, "version", b"knowledge-id-v1");
        write_field(&mut hasher, "category", category.as_str().as_bytes());
        write_field(&mut hasher, "subject", subject.as_bytes());
        write_field(&mut hasher, "predicate", predicate.as_bytes());
        write_field(&mut hasher, "value", canonical.as_bytes());
        let digest = hasher.finalize();
        let mut text = String::from(crate::version::KNOWLEDGE_ID_PREFIX);
        for byte in digest {
            text.push(hex_digit(byte >> 4));
            text.push(hex_digit(byte & 0x0f));
        }
        Ok(Self(text))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, KnowledgeError> {
        let value = value.into();
        let hex = value
            .strip_prefix(crate::version::KNOWLEDGE_ID_PREFIX)
            .ok_or_else(|| KnowledgeError::InvalidId(value.clone()))?;
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(KnowledgeError::InvalidId(value));
        }
        if hex.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(KnowledgeError::InvalidId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for KnowledgeId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One evidence-backed, versioned knowledge assertion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeClaim {
    pub id: KnowledgeId,
    pub category: KnowledgeCategory,
    pub subject: String,
    pub predicate: String,
    pub value: serde_json::Value,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    pub confidence: Confidence,
    pub validity: Validity,
    #[serde(default)]
    pub validity_conditions: Vec<ValidityCondition>,
    pub created_at: String,
}

impl KnowledgeClaim {
    pub fn new(
        category: KnowledgeCategory,
        subject: impl Into<String>,
        predicate: impl Into<String>,
        value: serde_json::Value,
        evidence: Vec<Evidence>,
        confidence: Confidence,
        validity: Validity,
    ) -> Result<Self, KnowledgeError> {
        let subject = subject.into();
        let predicate = predicate.into();
        validate_text("subject", &subject)?;
        validate_text("predicate", &predicate)?;
        let value = canonical_value(value);
        validate_safe_value(&value, "value")?;
        let id = KnowledgeId::derive(category, &subject, &predicate, &value)?;
        let mut evidence = evidence;
        if evidence.is_empty() {
            return Err(KnowledgeError::InvalidClaim(
                "at least one evidence reference is required".to_string(),
            ));
        }
        for item in &evidence {
            validate_text("evidence identifier", &item.identifier)?;
            if let Some(locator) = &item.locator {
                validate_text("evidence locator", locator)?;
            }
            for fingerprint in &item.fingerprints {
                validate_text("evidence fingerprint", fingerprint)?;
            }
        }
        evidence.sort();
        evidence.dedup();
        Ok(Self {
            id,
            category,
            subject,
            predicate,
            value,
            evidence,
            confidence,
            validity,
            validity_conditions: Vec::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    pub fn with_created_at(
        mut self,
        created_at: impl Into<String>,
    ) -> Result<Self, KnowledgeError> {
        let created_at = created_at.into();
        validate_text("created_at", &created_at)?;
        self.created_at = created_at;
        Ok(self)
    }

    pub fn with_validity_conditions(
        mut self,
        mut validity_conditions: Vec<ValidityCondition>,
    ) -> Self {
        validity_conditions.sort();
        validity_conditions.dedup();
        self.validity_conditions = validity_conditions;
        self
    }

    pub fn observed_state(&self, current_state: Option<&str>) -> Self {
        let mut observed = self.clone();
        if observed.validity == Validity::Invalidated {
            return observed;
        }
        let Some(current_state) = current_state else {
            observed.validity = Validity::Unknown;
            return observed;
        };
        let has_state_condition = observed
            .validity_conditions
            .iter()
            .filter_map(|condition| condition.state_fingerprint.as_deref())
            .any(|state| state != current_state);
        observed.validity = if has_state_condition {
            Validity::Stale
        } else {
            Validity::Current
        };
        observed
    }

    pub fn invalidated(&self) -> Self {
        let mut invalidated = self.clone();
        invalidated.validity = Validity::Invalidated;
        invalidated
    }

    pub(crate) fn validate_identity(&self) -> Result<(), KnowledgeError> {
        validate_text("subject", &self.subject)?;
        validate_text("predicate", &self.predicate)?;
        validate_text("created_at", &self.created_at)?;
        if self.evidence.is_empty() {
            return Err(KnowledgeError::InvalidClaim(
                "at least one evidence reference is required".to_string(),
            ));
        }
        for item in &self.evidence {
            validate_text("evidence identifier", &item.identifier)?;
            if let Some(locator) = &item.locator {
                validate_text("evidence locator", locator)?;
            }
            for fingerprint in &item.fingerprints {
                validate_text("evidence fingerprint", fingerprint)?;
            }
        }
        validate_safe_value(&self.value, "value")?;
        let expected =
            KnowledgeId::derive(self.category, &self.subject, &self.predicate, &self.value)?;
        if expected != self.id {
            return Err(KnowledgeError::InvalidClaim(
                "claim ID does not match its category, subject, predicate, and value".to_string(),
            ));
        }
        Ok(())
    }
}

fn validate_text(label: &str, value: &str) -> Result<(), KnowledgeError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(KnowledgeError::InvalidClaim(format!(
            "{label} must be non-empty and contain no control characters"
        )));
    }
    Ok(())
}

fn validate_safe_value(value: &serde_json::Value, path: &str) -> Result<(), KnowledgeError> {
    match value {
        serde_json::Value::Array(values) => values
            .iter()
            .enumerate()
            .try_for_each(|(index, value)| validate_safe_value(value, &format!("{path}[{index}]"))),
        serde_json::Value::Object(values) => values.iter().try_for_each(|(key, value)| {
            let normalized = key.to_ascii_lowercase().replace('-', "_");
            let compact = normalized.replace('_', "");
            if matches!(
                compact.as_str(),
                "password"
                    | "token"
                    | "secret"
                    | "authorization"
                    | "privatekey"
                    | "apikey"
                    | "accesstoken"
                    | "refreshtoken"
            ) {
                return Err(KnowledgeError::SensitiveField(format!("{path}.{key}")));
            }
            validate_safe_value(value, &format!("{path}.{key}"))
        }),
        _ => Ok(()),
    }
}

fn canonical_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(canonical_value).collect())
        }
        serde_json::Value::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, canonical_value(value)))
                .collect(),
        ),
        value => value,
    }
}

fn canonical_json(value: &serde_json::Value) -> Result<String, KnowledgeError> {
    match value {
        serde_json::Value::Null => Ok("null".to_string()),
        serde_json::Value::Bool(value) => Ok(value.to_string()),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        serde_json::Value::String(value) => serde_json::to_string(value).map_err(|source| {
            KnowledgeError::InvalidClaim(format!("could not canonicalize value: {source}"))
        }),
        serde_json::Value::Array(values) => {
            let values = values
                .iter()
                .map(canonical_json)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("[{}]", values.join(",")))
        }
        serde_json::Value::Object(values) => {
            let mut entries = values
                .iter()
                .map(|(key, value)| {
                    Ok::<_, KnowledgeError>((
                        serde_json::to_string(key).map_err(|source| {
                            KnowledgeError::InvalidClaim(format!(
                                "could not canonicalize object key: {source}"
                            ))
                        })?,
                        canonical_json(value)?,
                    ))
                })
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            Ok(format!(
                "{{{}}}",
                entries
                    .into_iter()
                    .map(|(key, value)| format!("{key}:{value}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ))
        }
    }
}

fn write_field(hasher: &mut Sha256, label: &str, value: &[u8]) {
    hasher.update((label.len() as u64).to_be_bytes());
    hasher.update(label.as_bytes());
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn hex_digit(value: u8) -> char {
    if value < 10 {
        (b'0' + value) as char
    } else {
        (b'a' + value - 10) as char
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{evidence::EvidenceKind, KnowledgeCategory};

    fn claim(value: serde_json::Value) -> KnowledgeClaim {
        KnowledgeClaim::new(
            KnowledgeCategory::Fact,
            "project:astraos",
            "uses_language",
            value,
            vec![Evidence::new(EvidenceKind::ContextFact, "language-rust")],
            Confidence::High,
            Validity::Current,
        )
        .and_then(|claim| claim.with_created_at("2026-01-01T00:00:00Z"))
        .expect("claim")
    }

    #[test]
    fn category_is_part_of_the_deterministic_id() {
        let fact = claim(serde_json::json!("rust"));
        let decision = KnowledgeClaim::new(
            KnowledgeCategory::Decision,
            fact.subject.clone(),
            fact.predicate.clone(),
            fact.value.clone(),
            fact.evidence.clone(),
            fact.confidence,
            fact.validity,
        )
        .and_then(|claim| claim.with_created_at(fact.created_at.clone()))
        .expect("decision");
        assert_ne!(fact.id, decision.id);
    }

    #[test]
    fn equivalent_claims_have_stable_ids() {
        let first = claim(serde_json::json!({"b": 2, "a": 1}));
        let second = claim(serde_json::json!({"a": 1, "b": 2}));
        assert_eq!(first.id, second.id);
    }

    #[test]
    fn state_observation_marks_old_verification_stale_without_deleting_it() {
        let claim = KnowledgeClaim::new(
            KnowledgeCategory::Verification,
            "project:astraos",
            "cargo_check",
            serde_json::json!({"verdict": "verified_check"}),
            vec![Evidence::new(
                EvidenceKind::ExecutionResult,
                "execution:test",
            )],
            Confidence::Certain,
            Validity::Current,
        )
        .map(|claim| {
            claim
                .with_validity_conditions(vec![ValidityCondition::state_bound(
                    "sha256:a", "sha256:b", "sha256:c",
                )])
                .with_created_at("2026-01-01T00:00:00Z")
        })
        .and_then(|claim| claim)
        .expect("verification");
        let observed = claim.observed_state(Some("sha256:changed"));
        assert_eq!(claim.validity, Validity::Current);
        assert_eq!(observed.validity, Validity::Stale);
        assert_eq!(observed.id, claim.id);
    }

    #[test]
    fn obvious_sensitive_fields_are_rejected() {
        let rejected = KnowledgeClaim::new(
            KnowledgeCategory::Fact,
            "project:astraos",
            "credential",
            serde_json::json!({"api_key": "secret"}),
            Vec::new(),
            Confidence::Low,
            Validity::Unknown,
        );
        assert!(matches!(rejected, Err(KnowledgeError::SensitiveField(_))));
    }

    #[test]
    fn claims_require_evidence() {
        let rejected = KnowledgeClaim::new(
            KnowledgeCategory::Fact,
            "project:astraos",
            "language",
            serde_json::json!("rust"),
            Vec::new(),
            Confidence::High,
            Validity::Current,
        );
        assert!(matches!(rejected, Err(KnowledgeError::InvalidClaim(_))));
    }
}
