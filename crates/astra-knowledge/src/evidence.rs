use serde::{Deserialize, Serialize};

/// Structured source categories accepted as provenance for a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    ContextFact,
    ManifestObservation,
    RepositoryObservation,
    AdrDecision,
    ExecutionResult,
    UserDecision,
    RoadmapGoal,
}

impl EvidenceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ContextFact => "context_fact",
            Self::ManifestObservation => "manifest_observation",
            Self::RepositoryObservation => "repository_observation",
            Self::AdrDecision => "adr_decision",
            Self::ExecutionResult => "execution_result",
            Self::UserDecision => "user_decision",
            Self::RoadmapGoal => "roadmap_goal",
        }
    }
}

/// A compact reference to the reason a claim exists.
///
/// Evidence deliberately contains identifiers, locations, and fingerprints;
/// it does not contain source contents, diffs, credentials, or terminal output.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub identifier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fingerprints: Vec<String>,
}

impl Evidence {
    pub fn new(kind: EvidenceKind, identifier: impl Into<String>) -> Self {
        Self {
            kind,
            identifier: identifier.into(),
            locator: None,
            fingerprints: Vec::new(),
        }
    }

    pub fn with_locator(mut self, locator: impl Into<String>) -> Self {
        self.locator = Some(locator.into());
        self
    }

    pub fn with_fingerprints(mut self, fingerprints: impl IntoIterator<Item = String>) -> Self {
        self.fingerprints = fingerprints.into_iter().collect();
        self.fingerprints.sort();
        self.fingerprints.dedup();
        self
    }
}
