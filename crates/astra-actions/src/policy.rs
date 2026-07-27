use crate::{ProjectAction, ProjectReference};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// The first policy is deliberately narrow: only the workspace validation
/// commands emitted by the current context engine are allowed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActionPolicy;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum PolicyDecision {
    Allowed,
    Rejected { reason: PolicyRejectionReason },
}

impl PolicyDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    pub fn rejection_reason(&self) -> Option<PolicyRejectionReason> {
        match self {
            Self::Allowed => None,
            Self::Rejected { reason } => Some(*reason),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyRejectionReason {
    UnsupportedExecutable,
    UnsupportedSubcommand,
    ActionSubcommandMismatch,
    DisallowedArgument,
    InvalidWorkingDirectory,
    WorkingDirectoryOutsideProject,
}

impl std::fmt::Display for PolicyRejectionReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::UnsupportedExecutable => "unsupported executable",
            Self::UnsupportedSubcommand => "unsupported Cargo subcommand",
            Self::ActionSubcommandMismatch => "action and Cargo subcommand do not agree",
            Self::DisallowedArgument => "disallowed argument",
            Self::InvalidWorkingDirectory => "invalid or missing working directory",
            Self::WorkingDirectoryOutsideProject => "working directory is outside the project root",
        };
        formatter.write_str(message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyEvaluation {
    pub action: ProjectAction,
    pub decision: PolicyDecision,
}

impl ActionPolicy {
    /// Evaluates an action and returns a normalized action for plan creation.
    /// This performs read-only path inspection and never starts a process.
    pub fn evaluate(&self, project: &ProjectReference, action: &ProjectAction) -> PolicyEvaluation {
        let mut normalized_action = action.clone();

        let decision = if action.command.executable != "cargo" {
            PolicyDecision::Rejected {
                reason: PolicyRejectionReason::UnsupportedExecutable,
            }
        } else if let Some(reason) = subcommand_rejection(action) {
            PolicyDecision::Rejected { reason }
        } else if !has_allowed_arguments(action) {
            PolicyDecision::Rejected {
                reason: PolicyRejectionReason::DisallowedArgument,
            }
        } else {
            match normalize_working_directory(&project.root, &mut normalized_action) {
                Ok(()) => PolicyDecision::Allowed,
                Err(reason) => PolicyDecision::Rejected { reason },
            }
        };

        PolicyEvaluation {
            action: normalized_action,
            decision,
        }
    }
}

fn subcommand_rejection(action: &ProjectAction) -> Option<PolicyRejectionReason> {
    let Some(subcommand) = action.command.arguments.first().map(String::as_str) else {
        return Some(PolicyRejectionReason::UnsupportedSubcommand);
    };
    if !matches!(subcommand, "build" | "check" | "test") {
        return Some(PolicyRejectionReason::UnsupportedSubcommand);
    }

    let matches_action = matches!(
        (action.id, subcommand),
        (crate::ActionId::Build, "build")
            | (crate::ActionId::Check, "check")
            | (crate::ActionId::Test, "test")
    );
    (!matches_action).then_some(PolicyRejectionReason::ActionSubcommandMismatch)
}

fn has_allowed_arguments(action: &ProjectAction) -> bool {
    action.command.arguments.len() == 2 && action.command.arguments[1] == "--workspace"
}

fn normalize_working_directory(
    project_root: &Path,
    action: &mut ProjectAction,
) -> Result<(), PolicyRejectionReason> {
    let canonical_root = fs::canonicalize(project_root)
        .map_err(|_| PolicyRejectionReason::InvalidWorkingDirectory)?;
    if !fs::metadata(&canonical_root)
        .map_err(|_| PolicyRejectionReason::InvalidWorkingDirectory)?
        .is_dir()
    {
        return Err(PolicyRejectionReason::InvalidWorkingDirectory);
    }
    let candidate = if action.command.working_directory.is_absolute() {
        action.command.working_directory.clone()
    } else {
        canonical_root.join(&action.command.working_directory)
    };
    let canonical_working_directory =
        fs::canonicalize(&candidate).map_err(|_| PolicyRejectionReason::InvalidWorkingDirectory)?;

    if !canonical_working_directory.starts_with(&canonical_root) {
        return Err(PolicyRejectionReason::WorkingDirectoryOutsideProject);
    }

    action.command.working_directory = canonical_working_directory;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionId, ActionSource, CommandSpec};
    use astra_context::Confidence;
    use std::fs;
    use tempfile::tempdir;

    fn action(
        id: ActionId,
        executable: &str,
        arguments: &[&str],
        directory: &Path,
    ) -> ProjectAction {
        ProjectAction {
            id,
            command: CommandSpec {
                executable: executable.to_string(),
                arguments: arguments.iter().map(|value| (*value).to_string()).collect(),
                working_directory: directory.to_path_buf(),
            },
            source: ActionSource::ContextEngine,
            confidence: Confidence::High,
        }
    }

    fn project(root: &Path) -> ProjectReference {
        ProjectReference {
            name: "fixture".to_string(),
            root: root.to_path_buf(),
        }
    }

    fn allowed(id: ActionId, root: &Path) -> ProjectAction {
        action(id, "cargo", &[id.as_str(), "--workspace"], root)
    }

    #[test]
    fn allows_each_supported_workspace_action() {
        let root = tempdir().expect("temporary project");
        let policy = ActionPolicy;

        for id in [ActionId::Build, ActionId::Check, ActionId::Test] {
            let evaluation = policy.evaluate(&project(root.path()), &allowed(id, root.path()));
            assert_eq!(evaluation.decision, PolicyDecision::Allowed);
            assert_eq!(
                evaluation.action.command.working_directory,
                root.path().canonicalize().expect("canonical root")
            );
        }
    }

    #[test]
    fn rejects_unsupported_executable_and_subcommands() {
        let root = tempdir().expect("temporary project");
        let policy = ActionPolicy;

        assert_eq!(
            policy
                .evaluate(
                    &project(root.path()),
                    &action(ActionId::Test, "bash", &["-c", "cargo test"], root.path())
                )
                .decision,
            PolicyDecision::Rejected {
                reason: PolicyRejectionReason::UnsupportedExecutable
            }
        );
        assert_eq!(
            policy
                .evaluate(
                    &project(root.path()),
                    &action(ActionId::Check, "cargo", &["install", "x"], root.path())
                )
                .decision,
            PolicyDecision::Rejected {
                reason: PolicyRejectionReason::UnsupportedSubcommand
            }
        );
    }

    #[test]
    fn rejects_empty_mismatched_and_unsupported_arguments() {
        let root = tempdir().expect("temporary project");
        let policy = ActionPolicy;

        let cases = [
            (
                ActionId::Build,
                &[][..],
                PolicyRejectionReason::UnsupportedSubcommand,
            ),
            (
                ActionId::Build,
                &["test", "--workspace"][..],
                PolicyRejectionReason::ActionSubcommandMismatch,
            ),
            (
                ActionId::Build,
                &["build"][..],
                PolicyRejectionReason::DisallowedArgument,
            ),
            (
                ActionId::Build,
                &["build", "--features", "danger"][..],
                PolicyRejectionReason::DisallowedArgument,
            ),
        ];
        for (id, arguments, reason) in cases {
            let evaluation = policy.evaluate(
                &project(root.path()),
                &action(id, "cargo", arguments, root.path()),
            );
            assert_eq!(evaluation.decision, PolicyDecision::Rejected { reason });
        }
    }

    #[test]
    fn policy_decisions_serialize_stably() {
        assert_eq!(
            serde_json::to_string(&PolicyDecision::Allowed).expect("allowed JSON"),
            r#"{"decision":"allowed"}"#
        );
        assert_eq!(
            serde_json::to_string(&PolicyDecision::Rejected {
                reason: PolicyRejectionReason::UnsupportedExecutable
            })
            .expect("rejected JSON"),
            r#"{"decision":"rejected","reason":"unsupported_executable"}"#
        );
    }

    #[test]
    fn evaluation_is_deterministic_and_does_not_mutate_files() {
        let root = tempdir().expect("temporary project");
        fs::write(root.path().join("Cargo.toml"), "[workspace]\n").expect("manifest");
        let before = fs::read(root.path().join("Cargo.toml")).expect("manifest bytes");
        let policy = ActionPolicy;
        let first = policy.evaluate(
            &project(root.path()),
            &allowed(ActionId::Check, root.path()),
        );
        let second = policy.evaluate(
            &project(root.path()),
            &allowed(ActionId::Check, root.path()),
        );

        assert_eq!(first, second);
        assert_eq!(
            fs::read(root.path().join("Cargo.toml")).expect("manifest bytes"),
            before
        );
        assert!(first.decision.is_allowed());
    }

    #[test]
    fn allows_root_and_child_but_rejects_outside_missing_and_traversal() {
        let root = tempdir().expect("temporary project");
        let child = root.path().join("crates/example");
        fs::create_dir_all(&child).expect("child directory");
        let outside = tempdir().expect("outside directory");
        let policy = ActionPolicy;

        for directory in [root.path().to_path_buf(), child.clone()] {
            let evaluation =
                policy.evaluate(&project(root.path()), &allowed(ActionId::Build, &directory));
            assert!(evaluation.decision.is_allowed());
            assert!(evaluation.action.command.working_directory.is_absolute());
        }
        for directory in [Path::new("."), Path::new("crates/example")] {
            let action = action(
                ActionId::Check,
                "cargo",
                &["check", "--workspace"],
                directory,
            );
            let evaluation = policy.evaluate(&project(root.path()), &action);
            assert!(evaluation.decision.is_allowed());
            assert!(evaluation.action.command.working_directory.is_absolute());
        }
        assert_eq!(
            policy
                .evaluate(
                    &project(root.path()),
                    &allowed(ActionId::Build, outside.path())
                )
                .decision,
            PolicyDecision::Rejected {
                reason: PolicyRejectionReason::WorkingDirectoryOutsideProject
            }
        );
        assert_eq!(
            policy
                .evaluate(
                    &project(root.path()),
                    &action(
                        ActionId::Build,
                        "cargo",
                        &["build", "--workspace"],
                        &root.path().join("missing")
                    )
                )
                .decision,
            PolicyDecision::Rejected {
                reason: PolicyRejectionReason::InvalidWorkingDirectory
            }
        );
        let traversal = root
            .path()
            .join("..")
            .join(outside.path().file_name().expect("outside name"));
        assert_eq!(
            policy
                .evaluate(
                    &project(root.path()),
                    &action(
                        ActionId::Build,
                        "cargo",
                        &["build", "--workspace"],
                        &traversal
                    )
                )
                .decision,
            PolicyDecision::Rejected {
                reason: PolicyRejectionReason::WorkingDirectoryOutsideProject
            }
        );
        let root_file = root.path().join("not-a-directory");
        fs::write(&root_file, "file").expect("root file");
        assert_eq!(
            policy
                .evaluate(&project(&root_file), &allowed(ActionId::Build, &root_file))
                .decision,
            PolicyDecision::Rejected {
                reason: PolicyRejectionReason::InvalidWorkingDirectory
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_after_canonicalization() {
        use std::os::unix::fs::symlink;

        let root = tempdir().expect("temporary project");
        let outside = tempdir().expect("outside directory");
        symlink(outside.path(), root.path().join("linked")).expect("symlink");
        let policy = ActionPolicy;
        let evaluation = policy.evaluate(
            &project(root.path()),
            &action(
                ActionId::Test,
                "cargo",
                &["test", "--workspace"],
                &root.path().join("linked"),
            ),
        );

        assert_eq!(
            evaluation.decision,
            PolicyDecision::Rejected {
                reason: PolicyRejectionReason::WorkingDirectoryOutsideProject
            }
        );
    }
}
