/// Explicit status for information that may be unavailable or unknown.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "value")]
pub enum Availability<T> {
    Available(T),
    Unavailable,
    Unknown,
}

impl<T> Availability<T> {
    pub fn available(value: T) -> Self {
        Self::Available(value)
    }
}

/// Normalized confidence used by the intelligence model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntelligenceConfidence {
    Certain,
    High,
    Medium,
    Low,
    Unknown,
}

impl IntelligenceConfidence {
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

impl std::fmt::Display for IntelligenceConfidence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Projected validity of a historical verification claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationValidity {
    Current,
    Stale,
    Invalidated,
    Unknown,
}
