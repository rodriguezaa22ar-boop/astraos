use serde::{Deserialize, Serialize};

/// Current status of a historical knowledge claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Validity {
    Current,
    Stale,
    Invalidated,
    Unknown,
}

impl Validity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Stale => "stale",
            Self::Invalidated => "invalidated",
            Self::Unknown => "unknown",
        }
    }
}

/// Conditions against which a claim can be checked without storing source
/// contents or command output.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ValidityCondition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_fingerprint: Option<String>,
}

impl ValidityCondition {
    pub fn state_bound(
        state_fingerprint: impl Into<String>,
        action_fingerprint: impl Into<String>,
        plan_fingerprint: impl Into<String>,
    ) -> Self {
        Self {
            state_fingerprint: Some(state_fingerprint.into()),
            action_fingerprint: Some(action_fingerprint.into()),
            plan_fingerprint: Some(plan_fingerprint.into()),
        }
    }
}
