use crate::{ActionId, ActionSource, CommandSpec, ProjectAction};
use astra_context::{CommandSpec as ContextCommandSpec, Detected};
use std::collections::BTreeMap;

/// Converts recognized context validation commands into read-only project
/// actions. No command is executed by this resolver.
pub fn resolve_actions(commands: &[Detected<ContextCommandSpec>]) -> Vec<ProjectAction> {
    let mut selected = BTreeMap::<ActionId, ProjectAction>::new();

    for detected in commands {
        let Some(action_id) = action_id(&detected.value) else {
            continue;
        };

        let candidate = ProjectAction {
            id: action_id,
            command: CommandSpec {
                executable: detected.value.executable.clone(),
                arguments: detected.value.arguments.clone(),
                working_directory: detected.value.working_directory.as_str().into(),
            },
            source: ActionSource::ContextEngine,
            confidence: detected.confidence,
        };

        let replace = selected
            .get(&action_id)
            .is_none_or(|current| candidate_precedes(&candidate, current));
        if replace {
            selected.insert(action_id, candidate);
        }
    }

    selected.into_values().collect()
}

fn action_id(command: &ContextCommandSpec) -> Option<ActionId> {
    if command.executable != "cargo" {
        return None;
    }

    match command.arguments.first().map(String::as_str) {
        Some("build") => Some(ActionId::Build),
        Some("check") => Some(ActionId::Check),
        Some("test") => Some(ActionId::Test),
        _ => None,
    }
}

fn candidate_precedes(candidate: &ProjectAction, current: &ProjectAction) -> bool {
    confidence_rank(candidate.confidence) > confidence_rank(current.confidence)
        || (candidate.confidence == current.confidence
            && candidate_key(candidate) < candidate_key(current))
}

fn confidence_rank(confidence: astra_context::Confidence) -> u8 {
    match confidence {
        astra_context::Confidence::Certain => 4,
        astra_context::Confidence::High => 3,
        astra_context::Confidence::Medium => 2,
        astra_context::Confidence::Low => 1,
    }
}

fn candidate_key(action: &ProjectAction) -> (String, Vec<String>, String) {
    (
        action
            .command
            .working_directory
            .to_string_lossy()
            .into_owned(),
        action.command.arguments.clone(),
        action.command.executable.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use astra_context::{CommandPurpose, Confidence, ProjectPath};
    use std::path::PathBuf;

    fn command(
        executable: &str,
        arguments: &[&str],
        working_directory: &str,
        confidence: Confidence,
    ) -> Detected<ContextCommandSpec> {
        Detected {
            value: ContextCommandSpec {
                executable: executable.to_string(),
                arguments: arguments.iter().map(|value| value.to_string()).collect(),
                working_directory: ProjectPath(working_directory.to_string()),
                purpose: CommandPurpose::Validate,
            },
            confidence,
            evidence: Vec::new(),
        }
    }

    #[test]
    fn recognizes_supported_cargo_actions_in_semantic_order() {
        let actions = resolve_actions(&[
            command("cargo", &["test", "--workspace"], ".", Confidence::High),
            command("cargo", &["build", "--workspace"], ".", Confidence::High),
            command("cargo", &["check", "--workspace"], ".", Confidence::High),
        ]);

        assert_eq!(
            actions.iter().map(|action| action.id).collect::<Vec<_>>(),
            vec![ActionId::Build, ActionId::Check, ActionId::Test]
        );
    }

    #[test]
    fn ignores_unsupported_commands_and_subcommands() {
        let actions = resolve_actions(&[
            command("npm", &["test"], ".", Confidence::Certain),
            command("cargo", &["fmt"], ".", Confidence::Certain),
        ]);

        assert!(actions.is_empty());
    }

    #[test]
    fn preserves_argv_and_working_directory() {
        let actions = resolve_actions(&[command(
            "cargo",
            &["test", "--workspace", "--", "--exact", "name with spaces"],
            "/tmp/project with spaces",
            Confidence::High,
        )]);

        assert_eq!(actions[0].command.executable, "cargo");
        assert_eq!(
            actions[0].command.arguments,
            ["test", "--workspace", "--", "--exact", "name with spaces"]
        );
        assert_eq!(
            actions[0].command.working_directory,
            PathBuf::from("/tmp/project with spaces")
        );
    }

    #[test]
    fn duplicate_candidates_prefer_confidence_then_lexical_key() {
        let actions = resolve_actions(&[
            command("cargo", &["build", "--workspace"], "/z", Confidence::Low),
            command("cargo", &["build", "--workspace"], "/a", Confidence::High),
            command("cargo", &["build", "--workspace"], "/b", Confidence::High),
        ]);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].confidence, Confidence::High);
        assert_eq!(actions[0].command.working_directory, PathBuf::from("/a"));
    }

    #[test]
    fn action_json_uses_stable_machine_names() {
        let action = resolve_actions(&[command(
            "cargo",
            &["check", "--workspace"],
            ".",
            Confidence::High,
        )])[0]
            .clone();
        let json = serde_json::to_string(&action).expect("action JSON");

        assert!(json.contains("\"id\":\"check\""));
        assert!(json.contains("\"source\":\"context_engine\""));
    }
}
