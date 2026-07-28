use serde::{Deserialize, Serialize};

/// Confidence expresses the reliability of a knowledge source, not the
/// importance of the claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Certain,
    High,
    Medium,
    Low,
    Unknown,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Certain => "certain",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Unknown => "unknown",
        }
    }
}
