use crate::{PolicyDecision, ProjectAction, ProjectReference};
use serde::{Deserialize, Serialize};

/// Version of the serialized dry-run report contract.
pub const ACTION_DRY_RUN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub project: ProjectReference,
    pub action: ProjectAction,
    pub policy: PolicyDecision,
}

impl ExecutionPlan {
    pub fn new(project: ProjectReference, action: ProjectAction, policy: PolicyDecision) -> Self {
        Self {
            project,
            action,
            policy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DryRunMode {
    DryRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DryRunExecutionState {
    pub mode: DryRunMode,
    pub process_started: bool,
}

impl Default for DryRunExecutionState {
    fn default() -> Self {
        Self {
            mode: DryRunMode::DryRun,
            process_started: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DryRunReport {
    pub schema_version: u32,
    #[serde(flatten)]
    pub plan: ExecutionPlan,
    pub execution: DryRunExecutionState,
}

impl DryRunReport {
    pub fn new(plan: ExecutionPlan) -> Self {
        Self {
            schema_version: ACTION_DRY_RUN_SCHEMA_VERSION,
            plan,
            execution: DryRunExecutionState::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionId, ActionSource, CommandSpec};
    use astra_context::Confidence;
    use serde_json::json;
    use std::path::PathBuf;

    fn report() -> DryRunReport {
        DryRunReport::new(ExecutionPlan::new(
            ProjectReference {
                name: "demo".to_string(),
                root: PathBuf::from("/tmp/demo"),
            },
            ProjectAction {
                id: ActionId::Check,
                command: CommandSpec {
                    executable: "cargo".to_string(),
                    arguments: vec!["check".to_string(), "--workspace".to_string()],
                    working_directory: PathBuf::from("/tmp/demo"),
                },
                source: ActionSource::ContextEngine,
                confidence: Confidence::High,
            },
            PolicyDecision::Allowed,
        ))
    }

    #[test]
    fn report_preserves_plan_and_marks_process_never_started() {
        let report = report();
        assert_eq!(report.schema_version, ACTION_DRY_RUN_SCHEMA_VERSION);
        assert_eq!(report.plan.action.id, ActionId::Check);
        assert!(!report.execution.process_started);
    }

    #[test]
    fn report_json_is_stable_and_contains_no_runtime_fields() {
        let first = serde_json::to_string_pretty(&report()).expect("first JSON");
        let second = serde_json::to_string_pretty(&report()).expect("second JSON");
        assert_eq!(first, second);
        let value: serde_json::Value = serde_json::from_str(&first).expect("report JSON");
        assert_eq!(
            value,
            json!({
                "schema_version": 1,
                "project": {"name": "demo", "root": "/tmp/demo"},
                "action": {
                    "id": "check",
                    "executable": "cargo",
                    "arguments": ["check", "--workspace"],
                    "working_directory": "/tmp/demo",
                    "source": "context_engine",
                    "confidence": "high"
                },
                "policy": {"decision": "allowed"},
                "execution": {"mode": "dry_run", "process_started": false}
            })
        );
        assert!(!first.contains("timestamp"));
    }
}
