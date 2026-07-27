use std::{
    io::{self, Read},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandInvocation {
    pub(crate) executable: String,
    pub(crate) arguments: Vec<String>,
    pub(crate) current_directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessOutput {
    pub(crate) status_code: Option<i32>,
    pub(crate) success: bool,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) timed_out: bool,
    pub(crate) truncated: bool,
}

struct ReaderOutputs {
    stdout: Vec<u8>,
    stdout_truncated: bool,
    stderr: Vec<u8>,
    stderr_truncated: bool,
    timed_out: bool,
}

pub(crate) trait ProcessRunner {
    fn run(
        &self,
        invocation: &CommandInvocation,
        timeout: Duration,
        output_limit: usize,
    ) -> io::Result<ProcessOutput>;
}

#[derive(Debug, Default)]
pub(crate) struct SystemProcessRunner;

impl ProcessRunner for SystemProcessRunner {
    fn run(
        &self,
        invocation: &CommandInvocation,
        timeout: Duration,
        output_limit: usize,
    ) -> io::Result<ProcessOutput> {
        run_bounded(invocation, timeout, output_limit)
    }
}

fn run_bounded(
    invocation: &CommandInvocation,
    timeout: Duration,
    output_limit: usize,
) -> io::Result<ProcessOutput> {
    let mut command = configured_command(invocation);
    let mut child = command.spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("stdout pipe unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("stderr pipe unavailable"))?;
    let stdout_thread = thread::spawn(move || read_bounded(stdout, output_limit));
    let stderr_thread = thread::spawn(move || read_bounded(stderr, output_limit));

    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "timeout is too large"))?;
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (status, false);
        }
        if Instant::now() >= deadline {
            if let Err(error) = child.kill() {
                if child.try_wait()?.is_none() {
                    return Err(error);
                }
            }
            break (child.wait()?, true);
        }
        thread::sleep(
            Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())),
        );
    };

    let readers = join_readers_until_deadline(stdout_thread, stderr_thread, deadline)?;
    let timed_out = timed_out || readers.timed_out;

    Ok(ProcessOutput {
        status_code: status.code(),
        success: status.success() && !timed_out,
        stdout: readers.stdout,
        stderr: readers.stderr,
        timed_out,
        truncated: readers.stdout_truncated || readers.stderr_truncated || readers.timed_out,
    })
}

fn configured_command(invocation: &CommandInvocation) -> Command {
    let mut command = Command::new(&invocation.executable);
    command
        .args(&invocation.arguments)
        .current_dir(&invocation.current_directory)
        .env_clear()
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null_device())
        .env("GIT_PAGER", "cat")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    command
}

fn read_bounded(mut reader: impl Read, output_limit: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut bytes = Vec::with_capacity(output_limit.min(8192));
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = output_limit.saturating_sub(bytes.len());
        let retained = remaining.min(read);
        bytes.extend_from_slice(&buffer[..retained]);
        truncated |= retained < read;
    }
    Ok((bytes, truncated))
}

fn join_reader(
    handle: thread::JoinHandle<io::Result<(Vec<u8>, bool)>>,
) -> io::Result<(Vec<u8>, bool)> {
    handle
        .join()
        .map_err(|_| io::Error::other("process output reader panicked"))?
}

fn join_readers_until_deadline(
    stdout: thread::JoinHandle<io::Result<(Vec<u8>, bool)>>,
    stderr: thread::JoinHandle<io::Result<(Vec<u8>, bool)>>,
    deadline: Instant,
) -> io::Result<ReaderOutputs> {
    while !stdout.is_finished() || !stderr.is_finished() {
        if Instant::now() >= deadline {
            let (stdout, stdout_truncated) = join_reader_if_finished(stdout)?;
            let (stderr, stderr_truncated) = join_reader_if_finished(stderr)?;
            return Ok(ReaderOutputs {
                stdout,
                stdout_truncated,
                stderr,
                stderr_truncated,
                timed_out: true,
            });
        }
        thread::sleep(
            Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())),
        );
    }

    let (stdout, stdout_truncated) = join_reader(stdout)?;
    let (stderr, stderr_truncated) = join_reader(stderr)?;
    Ok(ReaderOutputs {
        stdout,
        stdout_truncated,
        stderr,
        stderr_truncated,
        timed_out: false,
    })
}

fn join_reader_if_finished(
    handle: thread::JoinHandle<io::Result<(Vec<u8>, bool)>>,
) -> io::Result<(Vec<u8>, bool)> {
    if handle.is_finished() {
        join_reader(handle)
    } else {
        drop(handle);
        Ok((Vec::new(), false))
    }
}

fn null_device() -> &'static str {
    if cfg!(windows) {
        "NUL"
    } else {
        "/dev/null"
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque, ffi::OsStr};

    #[derive(Debug, Default)]
    pub(crate) struct FakeProcessRunner {
        outputs: RefCell<VecDeque<io::Result<ProcessOutput>>>,
        invocations: RefCell<Vec<CommandInvocation>>,
    }

    impl FakeProcessRunner {
        pub(crate) fn with_outputs(outputs: Vec<ProcessOutput>) -> Self {
            Self {
                outputs: RefCell::new(outputs.into_iter().map(Ok).collect()),
                invocations: RefCell::new(Vec::new()),
            }
        }

        pub(crate) fn with_results(outputs: Vec<io::Result<ProcessOutput>>) -> Self {
            Self {
                outputs: RefCell::new(outputs.into()),
                invocations: RefCell::new(Vec::new()),
            }
        }

        pub(crate) fn invocations(&self) -> Vec<CommandInvocation> {
            self.invocations.borrow().clone()
        }
    }

    impl ProcessRunner for FakeProcessRunner {
        fn run(
            &self,
            invocation: &CommandInvocation,
            _timeout: Duration,
            _output_limit: usize,
        ) -> io::Result<ProcessOutput> {
            self.invocations.borrow_mut().push(invocation.clone());
            self.outputs
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| Err(io::Error::other("unexpected invocation")))
        }
    }

    pub(crate) fn output(success: bool, stdout: &str, stderr: &str) -> ProcessOutput {
        ProcessOutput {
            status_code: Some(if success { 0 } else { 1 }),
            success,
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            timed_out: false,
            truncated: false,
        }
    }

    #[test]
    fn bounded_reader_retains_a_prefix_and_drains_the_rest() {
        let input = io::Cursor::new(b"0123456789");
        let (bytes, truncated) = read_bounded(input, 4).expect("bounded read");
        assert_eq!(bytes, b"0123");
        assert!(truncated);
    }

    #[test]
    fn git_process_environment_disables_network_and_interactive_state() {
        let invocation = CommandInvocation {
            executable: "git".to_string(),
            arguments: vec!["status".to_string()],
            current_directory: PathBuf::from("."),
        };
        let command = configured_command(&invocation);
        let environment = command.get_envs().collect::<Vec<_>>();
        for (name, expected) in [
            ("GIT_NO_LAZY_FETCH", "1"),
            ("GIT_OPTIONAL_LOCKS", "0"),
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_CONFIG_NOSYSTEM", "1"),
        ] {
            assert!(environment.iter().any(|(key, value)| {
                *key == OsStr::new(name) && *value == Some(OsStr::new(expected))
            }));
        }
    }

    #[test]
    fn reader_completion_cannot_outlive_the_command_deadline() {
        let stdout = thread::spawn(|| Ok((b"ready".to_vec(), false)));
        let stderr = thread::spawn(|| {
            thread::sleep(Duration::from_millis(100));
            Ok((b"late".to_vec(), false))
        });
        let deadline = Instant::now() + Duration::from_millis(5);

        let readers =
            join_readers_until_deadline(stdout, stderr, deadline).expect("bounded readers");
        assert!(readers.timed_out);
        assert!(readers.stderr.is_empty());
    }
}
