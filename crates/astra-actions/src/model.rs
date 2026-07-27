use astra_context::Confidence;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, str::FromStr};

/// Version of the serialized project-action report contract.
pub const PROJECT_ACTION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionId {
    Build,
    Check,
    Test,
}

impl ActionId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Check => "check",
            Self::Test => "test",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "build" => Some(Self::Build),
            "check" => Some(Self::Check),
            "test" => Some(Self::Test),
            _ => None,
        }
    }
}

impl std::fmt::Display for ActionId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ActionId {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandSpec {
    pub executable: String,
    pub arguments: Vec<String>,
    pub working_directory: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionSource {
    ContextEngine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAction {
    pub id: ActionId,
    #[serde(flatten)]
    pub command: CommandSpec,
    pub source: ActionSource,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectReference {
    pub name: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectActionReport {
    pub schema_version: u32,
    pub project: ProjectReference,
    pub actions: Vec<ProjectAction>,
}

impl ProjectActionReport {
    pub fn new(project: ProjectReference, actions: Vec<ProjectAction>) -> Self {
        Self {
            schema_version: PROJECT_ACTION_SCHEMA_VERSION,
            project,
            actions,
        }
    }
}
