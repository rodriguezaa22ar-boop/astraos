use crate::{error::ExecutionError, plan::AuthorizedExecutionPlan};
use std::{
    io::{self, Read, Write},
    process::{Command, Stdio},
    thread,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionOutputMode {
    Human,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProcessCompletion {
    pub(crate) exit_code: Option<i32>,
    pub(crate) interrupted: bool,
}

pub(crate) trait ProcessLauncher {
    fn launch(
        &self,
        plan: &AuthorizedExecutionPlan,
        output_mode: ExecutionOutputMode,
    ) -> Result<ProcessCompletion, ExecutionError>;
}

#[derive(Debug, Default)]
pub(crate) struct SystemProcessLauncher;

impl ProcessLauncher for SystemProcessLauncher {
    fn launch(
        &self,
        plan: &AuthorizedExecutionPlan,
        output_mode: ExecutionOutputMode,
    ) -> Result<ProcessCompletion, ExecutionError> {
        let mut command = Command::new(&plan.action.command.executable);
        command
            .args(&plan.action.command.arguments)
            .current_dir(&plan.action.command.working_directory)
            .stdin(Stdio::inherit());

        if output_mode == ExecutionOutputMode::Human {
            command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
            let status = command
                .spawn()
                .and_then(|mut child| child.wait())
                .map_err(|source| ExecutionError::SpawnFailed {
                    executable: plan.action.command.executable.clone(),
                    source,
                })?;
            return Ok(ProcessCompletion {
                exit_code: status.code(),
                interrupted: !status.success() && status.code().is_none(),
            });
        }

        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|source| ExecutionError::SpawnFailed {
                executable: plan.action.command.executable.clone(),
                source,
            })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            ExecutionError::OutputForwardFailed("child stdout pipe unavailable".to_string())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            ExecutionError::OutputForwardFailed("child stderr pipe unavailable".to_string())
        })?;
        let stdout_thread = thread::spawn(|| forward_to_stderr(stdout));
        let stderr_thread = thread::spawn(|| forward_to_stderr(stderr));
        let status = child.wait().map_err(|source| {
            ExecutionError::OutputForwardFailed(format!(
                "could not wait for child process: {source}"
            ))
        })?;
        stdout_thread
            .join()
            .map_err(|_| {
                ExecutionError::OutputForwardFailed("stdout forwarder panicked".to_string())
            })?
            .map_err(|source| ExecutionError::OutputForwardFailed(source.to_string()))?;
        stderr_thread
            .join()
            .map_err(|_| {
                ExecutionError::OutputForwardFailed("stderr forwarder panicked".to_string())
            })?
            .map_err(|source| ExecutionError::OutputForwardFailed(source.to_string()))?;

        Ok(ProcessCompletion {
            exit_code: status.code(),
            interrupted: !status.success() && status.code().is_none(),
        })
    }
}

fn forward_to_stderr(mut reader: impl Read) -> io::Result<()> {
    let mut stderr = io::stderr();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        stderr.write_all(&buffer[..read])?;
        stderr.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fingerprint::hash_fields, plan::AuthorizedExecutionPlan, state::ProjectStateBinding,
    };
    use astra_actions::{ActionId, ActionSource, CommandSpec, ProjectAction, ProjectReference};
    use std::fs;

    #[cfg(unix)]
    #[test]
    fn system_launcher_uses_exact_argv_and_current_directory_without_a_shell() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = tempfile::tempdir().expect("fixture");
        let output = fixture.path().join("invocation.txt");
        let executable = fixture.path().join("probe");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$PWD\" > '{}'\nprintf '<%s>\\n' \"$1\" >> '{}'\nprintf '<%s>\\n' \"$2\" >> '{}'\nprintf 'stdout\\n'\nprintf 'stderr\\n' >&2\n",
            output.display(),
            output.display(),
            output.display(),
        );
        fs::write(&executable, script).expect("probe");
        let mut permissions = fs::metadata(&executable)
            .expect("probe metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("probe permissions");
        let fingerprint = hash_fields("test", &[("state", b"state")]);
        let state = ProjectStateBinding::new(
            fixture.path().to_path_buf(),
            fixture.path().to_path_buf(),
            "0123456789012345678901234567890123456789".to_string(),
            fingerprint.clone(),
            fingerprint.clone(),
            fingerprint,
        );
        let plan = AuthorizedExecutionPlan::new(
            ProjectReference {
                name: "fixture".to_string(),
                root: fixture.path().to_path_buf(),
            },
            ProjectAction {
                id: ActionId::Check,
                command: CommandSpec {
                    executable: executable.to_string_lossy().into_owned(),
                    arguments: vec!["one argument".to_string(), "two".to_string()],
                    working_directory: fixture.path().to_path_buf(),
                },
                source: ActionSource::ContextEngine,
                confidence: serde_json::from_str("\"high\"").expect("confidence"),
            },
            state,
        );
        SystemProcessLauncher
            .launch(&plan, ExecutionOutputMode::Human)
            .expect("probe launch");
        let invocation = fs::read_to_string(output).expect("invocation output");
        assert!(invocation.contains(&format!("{}\n", fixture.path().display())));
        assert!(invocation.contains("<one argument>\n"));
        assert!(invocation.contains("<two>\n"));
    }

    #[test]
    fn output_modes_are_explicit_and_stable() {
        assert_eq!(ExecutionOutputMode::Human, ExecutionOutputMode::Human);
        assert_ne!(ExecutionOutputMode::Human, ExecutionOutputMode::Json);
    }
}
