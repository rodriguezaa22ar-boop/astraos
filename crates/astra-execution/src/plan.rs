use crate::{fingerprint::hash_fields, state::ProjectStateBinding, Fingerprint};
use astra_actions::{ActionSource, ProjectAction, ProjectReference, ACTION_POLICY_VERSION};
use serde::{Deserialize, Serialize};

/// Version of the serialized state-bound execution-plan contract.
pub const AUTHORIZED_EXECUTION_PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizedExecutionPlan {
    pub schema_version: u32,
    pub policy_version: u32,
    pub project: ProjectReference,
    pub action: ProjectAction,
    pub source_state: ProjectStateBinding,
    pub action_fingerprint: Fingerprint,
    pub plan_fingerprint: Fingerprint,
}

impl AuthorizedExecutionPlan {
    pub(crate) fn new(
        project: ProjectReference,
        action: ProjectAction,
        source_state: ProjectStateBinding,
    ) -> Self {
        let action_fingerprint = action_fingerprint(&action, ACTION_POLICY_VERSION);
        let plan_fingerprint = plan_fingerprint(
            &project,
            &action_fingerprint,
            &source_state,
            AUTHORIZED_EXECUTION_PLAN_SCHEMA_VERSION,
            ACTION_POLICY_VERSION,
        );
        Self {
            schema_version: AUTHORIZED_EXECUTION_PLAN_SCHEMA_VERSION,
            policy_version: ACTION_POLICY_VERSION,
            project,
            action,
            source_state,
            action_fingerprint,
            plan_fingerprint,
        }
    }
}

pub(crate) fn action_fingerprint(action: &ProjectAction, policy_version: u32) -> Fingerprint {
    let policy_version = policy_version.to_string();
    let action_id = action.id.as_str();
    let source = match action.source {
        ActionSource::ContextEngine => "context_engine",
    };
    let confidence =
        serde_json::to_string(&action.confidence).unwrap_or_else(|_| "\"unknown\"".to_string());
    let working_directory = action
        .command
        .working_directory
        .to_string_lossy()
        .into_owned();
    let argument_count = action.command.arguments.len().to_string();
    let mut fields = vec![
        ("policy_version", policy_version.as_bytes()),
        ("action_id", action_id.as_bytes()),
        ("executable", action.command.executable.as_bytes()),
        ("argument_count", argument_count.as_bytes()),
    ];
    for argument in &action.command.arguments {
        fields.push(("argument", argument.as_bytes()));
    }
    fields.extend([
        ("working_directory", working_directory.as_bytes()),
        ("source", source.as_bytes()),
        ("confidence", confidence.trim_matches('"').as_bytes()),
    ]);
    hash_fields("astra-action-v1", &fields)
}

pub(crate) fn plan_fingerprint(
    project: &ProjectReference,
    action_fingerprint: &Fingerprint,
    source_state: &ProjectStateBinding,
    schema_version: u32,
    policy_version: u32,
) -> Fingerprint {
    let schema_version = schema_version.to_string();
    let policy_version = policy_version.to_string();
    let root = project.root.to_string_lossy().into_owned();
    hash_fields(
        "astra-execution-plan-v1",
        &[
            ("schema_version", schema_version.as_bytes()),
            ("policy_version", policy_version.as_bytes()),
            ("project_name", project.name.as_bytes()),
            ("project_root", root.as_bytes()),
            ("action", action_fingerprint.as_str().as_bytes()),
            (
                "source_state",
                source_state.combined_fingerprint.as_str().as_bytes(),
            ),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ProjectStateBinding;
    use astra_actions::ActionId;
    use std::path::PathBuf;

    fn state() -> ProjectStateBinding {
        let fingerprint = crate::fingerprint::hash_fields("test", &[("value", b"state")]);
        ProjectStateBinding::new(
            PathBuf::from("/project"),
            PathBuf::from("/repo"),
            "0123456789012345678901234567890123456789".to_string(),
            fingerprint.clone(),
            fingerprint.clone(),
            fingerprint,
        )
    }

    fn action() -> ProjectAction {
        ProjectAction {
            id: ActionId::Check,
            command: astra_actions::CommandSpec {
                executable: "cargo".to_string(),
                arguments: vec!["check".to_string(), "--workspace".to_string()],
                working_directory: PathBuf::from("/project"),
            },
            source: ActionSource::ContextEngine,
            confidence: serde_json::from_str("\"high\"").expect("confidence"),
        }
    }

    #[test]
    fn identical_action_and_state_produce_identical_plan_fingerprints() {
        let project = ProjectReference {
            name: "demo".to_string(),
            root: PathBuf::from("/project"),
        };
        let first = AuthorizedExecutionPlan::new(project.clone(), action(), state());
        let second = AuthorizedExecutionPlan::new(project, action(), state());
        assert_eq!(first, second);
    }

    #[test]
    fn action_and_plan_fingerprints_bind_all_material_fields() {
        let mut changed_action = action();
        changed_action.command.arguments.reverse();
        assert_ne!(
            action_fingerprint(&action(), 1),
            action_fingerprint(&changed_action, 1)
        );
        assert_ne!(
            action_fingerprint(&action(), 1),
            action_fingerprint(&action(), 2)
        );

        let project = ProjectReference {
            name: "demo".to_string(),
            root: PathBuf::from("/project"),
        };
        let first = AuthorizedExecutionPlan::new(project.clone(), action(), state());
        let mut changed_project = project;
        changed_project.name = "other".to_string();
        let second = AuthorizedExecutionPlan::new(changed_project, action(), state());
        assert_ne!(first.plan_fingerprint, second.plan_fingerprint);
    }

    #[test]
    fn plan_serialization_contains_no_source_content() {
        let plan = AuthorizedExecutionPlan::new(
            ProjectReference {
                name: "demo".to_string(),
                root: PathBuf::from("/project"),
            },
            action(),
            state(),
        );
        let json = serde_json::to_string(&plan).expect("plan JSON");
        assert!(json.contains("plan_fingerprint"));
        assert!(!json.contains("Cargo.toml"));
        assert!(!json.contains("source contents"));
    }
}
