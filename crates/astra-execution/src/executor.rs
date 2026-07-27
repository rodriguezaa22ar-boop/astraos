use crate::{
    error::ExecutionError,
    git::GitStateCapture,
    plan::{action_fingerprint, plan_fingerprint, AuthorizedExecutionPlan},
    process::{ExecutionOutputMode, ProcessCompletion, ProcessLauncher, SystemProcessLauncher},
};
use astra_actions::{
    ActionId, ActionPolicy, PolicyDecision, ProjectAction, ProjectReference, ACTION_POLICY_VERSION,
};
use std::time::Instant;

/// Version of the serialized execution-result contract.
pub const EXECUTION_RESULT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationVerdict {
    VerifiedCheck,
    CommandFailed,
    SourceStateChanged,
    CommandFailedAndSourceStateChanged,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StateComparison {
    pub before: crate::ProjectStateBinding,
    pub after: crate::ProjectStateBinding,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProcessExecutionSummary {
    pub process_started: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub interrupted: bool,
    pub verdict: VerificationVerdict,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionResult {
    pub schema_version: u32,
    pub plan_fingerprint: crate::Fingerprint,
    pub action_fingerprint: crate::Fingerprint,
    pub project: ProjectReference,
    pub action: ProjectAction,
    pub state: StateComparison,
    pub execution: ProcessExecutionSummary,
}

pub struct ExecutionEngine {
    git: GitStateCapture,
    launcher: Box<dyn ProcessLauncher>,
}

impl Default for ExecutionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionEngine {
    pub fn new() -> Self {
        Self {
            git: GitStateCapture::default(),
            launcher: Box::<SystemProcessLauncher>::default(),
        }
    }

    pub fn plan(
        &self,
        project: &ProjectReference,
        action: &ProjectAction,
    ) -> Result<AuthorizedExecutionPlan, ExecutionError> {
        let canonical_root = canonical_root(&project.root)?;
        let project = ProjectReference {
            name: project.name.clone(),
            root: canonical_root,
        };
        let evaluation = ActionPolicy.evaluate(&project, action);
        if let PolicyDecision::Rejected { reason } = evaluation.decision {
            return Err(ExecutionError::PolicyRejected(reason));
        }
        if evaluation.action.id != ActionId::Check {
            return Err(ExecutionError::UnsupportedExecutionAction);
        }
        let source_state = self.git.capture(&project.root)?;
        Ok(AuthorizedExecutionPlan::new(
            project,
            evaluation.action,
            source_state,
        ))
    }

    pub fn execute(
        &self,
        plan: &AuthorizedExecutionPlan,
        output_mode: ExecutionOutputMode,
    ) -> Result<ExecutionResult, ExecutionError> {
        self.revalidate_plan(plan)?;

        let started = Instant::now();
        let completion = self.launcher.launch(plan, output_mode)?;
        let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let after = self
            .git
            .capture(&plan.project.root)
            .map_err(|error| ExecutionError::PostExecutionStateCapture(error.to_string()))?;
        let changed = after != plan.source_state;
        let verdict = verdict(completion, changed);

        Ok(ExecutionResult {
            schema_version: EXECUTION_RESULT_SCHEMA_VERSION,
            plan_fingerprint: plan.plan_fingerprint.clone(),
            action_fingerprint: plan.action_fingerprint.clone(),
            project: plan.project.clone(),
            action: plan.action.clone(),
            state: StateComparison {
                before: plan.source_state.clone(),
                after,
                changed,
            },
            execution: ProcessExecutionSummary {
                process_started: true,
                exit_code: completion.exit_code,
                duration_ms,
                interrupted: completion.interrupted,
                verdict,
            },
        })
    }

    fn revalidate_plan(&self, plan: &AuthorizedExecutionPlan) -> Result<(), ExecutionError> {
        if plan.schema_version != crate::AUTHORIZED_EXECUTION_PLAN_SCHEMA_VERSION {
            return Err(ExecutionError::UnsupportedPlanSchema(plan.schema_version));
        }
        if plan.policy_version != ACTION_POLICY_VERSION {
            return Err(ExecutionError::UnsupportedPolicyVersion(
                plan.policy_version,
            ));
        }
        if plan.action.id != ActionId::Check {
            return Err(ExecutionError::UnsupportedExecutionAction);
        }

        let canonical_root = canonical_root(&plan.project.root)?;
        if canonical_root != plan.source_state.canonical_root {
            return Err(ExecutionError::ProjectRootChanged);
        }
        let evaluation = ActionPolicy.evaluate(
            &ProjectReference {
                name: plan.project.name.clone(),
                root: canonical_root,
            },
            &plan.action,
        );
        match evaluation.decision {
            PolicyDecision::Allowed if evaluation.action == plan.action => {}
            PolicyDecision::Rejected { reason } => {
                return Err(ExecutionError::PolicyRejected(reason));
            }
            PolicyDecision::Allowed => return Err(ExecutionError::ActionFingerprintMismatch),
        }
        if action_fingerprint(&plan.action, plan.policy_version) != plan.action_fingerprint {
            return Err(ExecutionError::ActionFingerprintMismatch);
        }
        let current_state = self.git.capture(&plan.project.root)?;
        if current_state != plan.source_state {
            return Err(ExecutionError::StateFingerprintMismatch);
        }
        if plan_fingerprint(
            &plan.project,
            &plan.action_fingerprint,
            &plan.source_state,
            plan.schema_version,
            plan.policy_version,
        ) != plan.plan_fingerprint
        {
            return Err(ExecutionError::PlanFingerprintMismatch);
        }
        Ok(())
    }
}

fn canonical_root(path: &std::path::Path) -> Result<std::path::PathBuf, ExecutionError> {
    let canonical = path
        .canonicalize()
        .map_err(|_| ExecutionError::InvalidProjectRoot(path.to_path_buf()))?;
    if !canonical.is_dir() {
        return Err(ExecutionError::InvalidProjectRoot(path.to_path_buf()));
    }
    Ok(canonical)
}

fn verdict(completion: ProcessCompletion, state_changed: bool) -> VerificationVerdict {
    if completion.interrupted {
        VerificationVerdict::Interrupted
    } else if completion.exit_code == Some(0) {
        if state_changed {
            VerificationVerdict::SourceStateChanged
        } else {
            VerificationVerdict::VerifiedCheck
        }
    } else if state_changed {
        VerificationVerdict::CommandFailedAndSourceStateChanged
    } else {
        VerificationVerdict::CommandFailed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessLauncher;
    use std::{cell::Cell, fs, path::Path, process::Command, rc::Rc};
    use tempfile::TempDir;

    #[derive(Debug)]
    struct FakeLauncher {
        calls: Rc<Cell<usize>>,
        exit_code: Option<i32>,
        mutate: Option<std::path::PathBuf>,
    }

    impl ProcessLauncher for FakeLauncher {
        fn launch(
            &self,
            _plan: &AuthorizedExecutionPlan,
            _output_mode: ExecutionOutputMode,
        ) -> Result<ProcessCompletion, ExecutionError> {
            self.calls.set(self.calls.get() + 1);
            if let Some(path) = &self.mutate {
                fs::write(path, "mutated during execution\n")
                    .map_err(|source| ExecutionError::GitStateCapture(source.to_string()))?;
            }
            Ok(ProcessCompletion {
                exit_code: self.exit_code,
                interrupted: self.exit_code.is_none(),
            })
        }
    }

    fn git_fixture() -> (TempDir, std::path::PathBuf) {
        let directory = tempfile::tempdir().expect("Git fixture");
        fs::create_dir_all(directory.path().join("src")).expect("source directory");
        fs::write(directory.path().join("Cargo.toml"), "[workspace]\n").expect("manifest");
        fs::write(directory.path().join("src/lib.rs"), "pub fn value() {}\n").expect("source");
        git(directory.path(), &["init", "-q"]);
        git(directory.path(), &["config", "user.name", "Astra Test"]);
        git(
            directory.path(),
            &["config", "user.email", "astra@example.invalid"],
        );
        git(directory.path(), &["add", "."]);
        git(directory.path(), &["commit", "-qm", "initial"]);
        let root = directory.path().to_path_buf();
        (directory, root)
    }

    fn git(root: &Path, arguments: &[&str]) {
        let status = Command::new("git")
            .args(arguments)
            .current_dir(root)
            .status()
            .expect("Git command");
        assert!(status.success(), "Git command failed: {arguments:?}");
    }

    fn action(root: &Path) -> ProjectAction {
        ProjectAction {
            id: ActionId::Check,
            command: astra_actions::CommandSpec {
                executable: "cargo".to_string(),
                arguments: vec!["check".to_string(), "--workspace".to_string()],
                working_directory: root.to_path_buf(),
            },
            source: astra_actions::ActionSource::ContextEngine,
            confidence: serde_json::from_str("\"high\"").expect("confidence"),
        }
    }

    #[test]
    fn unchanged_dirty_state_can_execute_without_replanning() {
        let (_fixture, root) = git_fixture();
        fs::write(root.join("src/unstaged.rs"), "pub fn staged() {}\n").expect("unstaged source");
        git(&root, &["add", "src/unstaged.rs"]);
        fs::write(root.join("src/lib.rs"), "pub fn value() { let _ = 1; }\n")
            .expect("dirty source");
        fs::write(
            root.join("src/untracked file.rs"),
            "pub fn untracked() {}\n",
        )
        .expect("untracked source");
        let calls = Rc::new(Cell::new(0));
        let launcher = FakeLauncher {
            calls: Rc::clone(&calls),
            exit_code: Some(0),
            mutate: None,
        };
        let engine = ExecutionEngine {
            git: GitStateCapture::default(),
            launcher: Box::new(launcher),
        };
        let plan = engine
            .plan(
                &ProjectReference {
                    name: "fixture".to_string(),
                    root: root.clone(),
                },
                &action(&root),
            )
            .expect("plan");
        let result = engine
            .execute(&plan, ExecutionOutputMode::Json)
            .expect("execution");
        assert_eq!(result.execution.verdict, VerificationVerdict::VerifiedCheck);
        assert!(!result.state.changed);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn stale_plan_is_rejected_before_the_sentinel_launcher_runs() {
        let (_fixture, root) = git_fixture();
        let calls = Rc::new(Cell::new(0));
        let launcher = FakeLauncher {
            calls: Rc::clone(&calls),
            exit_code: Some(0),
            mutate: None,
        };
        let engine = ExecutionEngine {
            git: GitStateCapture::default(),
            launcher: Box::new(launcher),
        };
        let plan = engine
            .plan(
                &ProjectReference {
                    name: "fixture".to_string(),
                    root: root.clone(),
                },
                &action(&root),
            )
            .expect("plan");
        fs::write(root.join("src/lib.rs"), "pub fn value() { let _ = 2; }\n").expect("mutation");
        let result = engine.execute(&plan, ExecutionOutputMode::Json);
        assert!(matches!(
            result,
            Err(ExecutionError::StateFingerprintMismatch)
        ));
        assert_eq!(calls.get(), 0, "stale plans must not start the launcher");
    }

    #[test]
    fn verdicts_distinguish_failure_and_state_change() {
        assert_eq!(
            verdict(
                ProcessCompletion {
                    exit_code: Some(1),
                    interrupted: false
                },
                false
            ),
            VerificationVerdict::CommandFailed
        );
        assert_eq!(
            verdict(
                ProcessCompletion {
                    exit_code: Some(0),
                    interrupted: false
                },
                true
            ),
            VerificationVerdict::SourceStateChanged
        );
        assert_eq!(
            verdict(
                ProcessCompletion {
                    exit_code: None,
                    interrupted: true
                },
                true
            ),
            VerificationVerdict::Interrupted
        );
    }
}
