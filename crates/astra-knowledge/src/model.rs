use serde::{Deserialize, Serialize};

/// The initial bounded vocabulary of knowledge claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeCategory {
    Fact,
    Decision,
    Verification,
    Goal,
}

impl KnowledgeCategory {
    pub const ALL: [Self; 4] = [Self::Fact, Self::Decision, Self::Verification, Self::Goal];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fact => "facts",
            Self::Decision => "decisions",
            Self::Verification => "verifications",
            Self::Goal => "goals",
        }
    }

    pub fn singular(self) -> &'static str {
        match self {
            Self::Fact => "fact",
            Self::Decision => "decision",
            Self::Verification => "verification",
            Self::Goal => "goal",
        }
    }
}

/// Storage namespace. Global claims are supported even though the first CLI
/// integration focuses on project-scoped verification knowledge.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum KnowledgeNamespace {
    Global,
    Project(String),
}

impl KnowledgeNamespace {
    pub fn project(name: impl Into<String>) -> Self {
        Self::Project(name.into())
    }
}
