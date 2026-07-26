use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandInvocation {
    pub(crate) executable: String,
    pub(crate) arguments: Vec<String>,
    pub(crate) current_directory: Option<PathBuf>,
}

impl CommandInvocation {
    pub(crate) fn new(
        executable: impl Into<String>,
        arguments: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            executable: executable.into(),
            arguments: arguments.into_iter().map(Into::into).collect(),
            current_directory: None,
        }
    }

    pub(crate) fn render(&self) -> String {
        let executable =
            serde_json::to_string(&self.executable).unwrap_or_else(|_| "\"<invalid>\"".to_string());
        let arguments = serde_json::to_string(&self.arguments)
            .unwrap_or_else(|_| "[\"<invalid>\"]".to_string());
        let directory = self
            .current_directory
            .as_ref()
            .map(|path| serde_json::to_string(&path.to_string_lossy()))
            .transpose()
            .unwrap_or_else(|_| Some("\"<invalid>\"".to_string()))
            .unwrap_or_else(|| "null".to_string());

        format!("exec={executable} args={arguments} cwd={directory}")
    }
}

#[derive(Debug)]
pub(crate) struct ProcessOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) timed_out: bool,
}

pub(crate) trait ProcessRunner {
    fn executable_available(&self, executable: &str) -> bool;
    fn run(&mut self, invocation: &CommandInvocation) -> io::Result<ProcessOutput>;
    fn start(&mut self, invocation: &CommandInvocation) -> io::Result<()>;
}

pub(crate) struct SystemProcessRunner;

impl ProcessRunner for SystemProcessRunner {
    fn executable_available(&self, executable: &str) -> bool {
        executable_available(executable)
    }

    fn run(&mut self, invocation: &CommandInvocation) -> io::Result<ProcessOutput> {
        run_bounded(invocation)
    }

    fn start(&mut self, invocation: &CommandInvocation) -> io::Result<()> {
        let mut command = command(invocation);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
    }
}

fn command(invocation: &CommandInvocation) -> Command {
    let mut command = Command::new(&invocation.executable);
    command.args(&invocation.arguments);
    if let Some(directory) = &invocation.current_directory {
        command.current_dir(directory);
    }
    command
}

fn run_bounded(invocation: &CommandInvocation) -> io::Result<ProcessOutput> {
    let mut command = command(invocation);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("stdout pipe was not available"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("stderr pipe was not available"))?;
    let stdout_thread = thread::spawn(move || read_all(stdout));
    let stderr_thread = thread::spawn(move || read_all(stderr));

    let deadline = Instant::now() + COMMAND_TIMEOUT;
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (status, false);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            break (child.wait()?, true);
        }
        thread::sleep(POLL_INTERVAL);
    };

    Ok(ProcessOutput {
        status,
        stdout: join_reader(stdout_thread)?,
        stderr: join_reader(stderr_thread)?,
        timed_out,
    })
}

fn read_all(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn join_reader(handle: thread::JoinHandle<io::Result<Vec<u8>>>) -> io::Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| io::Error::other("process output reader panicked"))?
}

fn executable_available(executable: &str) -> bool {
    let path = Path::new(executable);
    if path.components().count() > 1 {
        return is_executable_file(path);
    }

    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .any(|directory| is_executable_file(&directory.join(executable)))
}

fn is_executable_file(path: &Path) -> bool {
    path.metadata()
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
        && executable_bit_is_set(path)
}

#[cfg(unix)]
fn executable_bit_is_set(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn executable_bit_is_set(_path: &Path) -> bool {
    true
}

pub(crate) fn status_label(status: ExitStatus) -> String {
    status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "terminated by signal".to_string())
}

pub(crate) fn bounded_text(bytes: &[u8]) -> String {
    const MAX_ERROR_BYTES: usize = 4096;
    let end = bytes.len().min(MAX_ERROR_BYTES);
    let mut rendered = String::from_utf8_lossy(&bytes[..end]).trim().to_string();
    if bytes.len() > MAX_ERROR_BYTES {
        rendered.push('…');
    }
    if rendered.is_empty() {
        rendered.push_str("no error output");
    }
    rendered
}
