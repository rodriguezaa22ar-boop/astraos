use crate::{
    error::ExecutionError,
    fingerprint::{hash_fields, Fingerprint},
    state::{ProjectStateBinding, StateCaptureLimits, MAX_GIT_DIFF_BYTES},
};
use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitOutput {
    status_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    truncated: bool,
}

trait GitRunner {
    fn run(
        &self,
        root: &Path,
        arguments: &[String],
        output_limit: usize,
    ) -> Result<GitOutput, ExecutionError>;
}

#[derive(Debug, Default)]
struct SystemGitRunner;

impl GitRunner for SystemGitRunner {
    fn run(
        &self,
        root: &Path,
        arguments: &[String],
        output_limit: usize,
    ) -> Result<GitOutput, ExecutionError> {
        run_bounded_git(root, arguments, output_limit)
    }
}

pub(crate) struct GitStateCapture {
    runner: Box<dyn GitRunner>,
    limits: StateCaptureLimits,
}

impl Default for GitStateCapture {
    fn default() -> Self {
        Self {
            runner: Box::<SystemGitRunner>::default(),
            limits: StateCaptureLimits::default(),
        }
    }
}

impl GitStateCapture {
    #[cfg(test)]
    fn with_runner(runner: Box<dyn GitRunner>, limits: StateCaptureLimits) -> Self {
        Self { runner, limits }
    }

    pub(crate) fn capture(
        &self,
        project_root: &Path,
    ) -> Result<ProjectStateBinding, ExecutionError> {
        let canonical_root = canonical_directory(project_root)?;
        let repository_root = self.repository_root(&canonical_root)?;
        if !canonical_root.starts_with(&repository_root) {
            return Err(ExecutionError::NonGitExecutionUnsupported);
        }
        let project_relative = canonical_root
            .strip_prefix(&repository_root)
            .map_err(|_| ExecutionError::NonGitExecutionUnsupported)?;
        let project_pathspec = pathspec(project_relative);

        let repository_head = self.repository_head(&canonical_root)?;
        let index = self.diff_fingerprint(&canonical_root, true, project_pathspec.as_deref())?;
        let worktree =
            self.diff_fingerprint(&canonical_root, false, project_pathspec.as_deref())?;
        let untracked = self.untracked_fingerprint(&canonical_root, project_pathspec.as_deref())?;

        Ok(ProjectStateBinding::new(
            canonical_root,
            repository_root,
            repository_head,
            index,
            worktree,
            untracked,
        ))
    }

    fn repository_root(&self, root: &Path) -> Result<PathBuf, ExecutionError> {
        let output = self.run(
            root,
            vec!["rev-parse".to_string(), "--show-toplevel".to_string()],
            MAX_GIT_DIFF_BYTES,
        )?;
        if output.truncated {
            return Err(ExecutionError::GitOutputLimitExceeded(
                "repository root".to_string(),
            ));
        }
        if !output_success(&output) {
            if output.status_code == Some(128) {
                return Err(ExecutionError::NonGitExecutionUnsupported);
            }
            return Err(git_command_error("repository root", &output, &self.limits));
        }
        let value = single_line(&output.stdout, "repository root")?;
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return Err(ExecutionError::MalformedGitOutput(
                "Git returned a relative repository root".to_string(),
            ));
        }
        fs::canonicalize(&path).map_err(|error| ExecutionError::GitStateCapture(error.to_string()))
    }

    fn repository_head(&self, root: &Path) -> Result<String, ExecutionError> {
        let output = self.run(
            root,
            vec![
                "rev-parse".into(),
                "--verify".into(),
                "HEAD^{commit}".into(),
            ],
            MAX_GIT_DIFF_BYTES,
        )?;
        if output.truncated {
            return Err(ExecutionError::GitOutputLimitExceeded(
                "repository HEAD".to_string(),
            ));
        }
        if !output_success(&output) {
            return Err(git_command_error("repository HEAD", &output, &self.limits));
        }
        let head = single_line(&output.stdout, "repository HEAD")?;
        if !matches!(head.len(), 40 | 64) || !head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ExecutionError::MalformedGitOutput(
                "Git returned an invalid commit identifier".to_string(),
            ));
        }
        Ok(head)
    }

    fn diff_fingerprint(
        &self,
        root: &Path,
        cached: bool,
        project_pathspec: Option<&str>,
    ) -> Result<Fingerprint, ExecutionError> {
        let mut arguments = vec![
            "diff".to_string(),
            "--no-color".to_string(),
            "--no-ext-diff".to_string(),
            "--no-textconv".to_string(),
            "--binary".to_string(),
        ];
        if cached {
            arguments.push("--cached".to_string());
        }
        arguments.push("--".to_string());
        if let Some(pathspec) = project_pathspec {
            arguments.push(pathspec.to_string());
        }

        let output = self.run(root, arguments, self.limits.max_git_diff_bytes)?;
        if !output_success(&output) {
            return Err(git_command_error(
                if cached {
                    "staged diff"
                } else {
                    "worktree diff"
                },
                &output,
                &self.limits,
            ));
        }
        if output.truncated {
            return Err(ExecutionError::GitOutputLimitExceeded(if cached {
                "staged diff".to_string()
            } else {
                "unstaged diff".to_string()
            }));
        }
        Ok(hash_fields(
            if cached {
                "astra-git-index-v1"
            } else {
                "astra-git-worktree-v1"
            },
            &[("diff", &output.stdout)],
        ))
    }

    fn untracked_fingerprint(
        &self,
        root: &Path,
        project_pathspec: Option<&str>,
    ) -> Result<Fingerprint, ExecutionError> {
        let mut arguments = vec![
            "ls-files".to_string(),
            "--others".to_string(),
            "--exclude-standard".to_string(),
            "-z".to_string(),
            "--".to_string(),
        ];
        if let Some(pathspec) = project_pathspec {
            arguments.push(pathspec.to_string());
        }
        let output = self.run(root, arguments, self.limits.max_git_diff_bytes)?;
        if !output_success(&output) {
            return Err(git_command_error(
                "untracked file discovery",
                &output,
                &self.limits,
            ));
        }
        if output.truncated {
            return Err(ExecutionError::GitOutputLimitExceeded(
                "untracked file discovery".to_string(),
            ));
        }

        let mut paths = parse_nul_paths(&output.stdout)?;
        paths.sort();
        if paths.len() > self.limits.max_untracked_files {
            return Err(ExecutionError::UntrackedStateLimitExceeded);
        }

        let mut total_bytes = 0_u64;
        let mut records = Vec::with_capacity(paths.len());
        for repository_relative in paths {
            let relative = project_relative_path(&repository_relative)?;
            let absolute = root.join(&relative);
            let metadata = fs::symlink_metadata(&absolute).map_err(|source| {
                ExecutionError::UntrackedFileUnreadable {
                    path: relative.clone(),
                    source,
                }
            })?;
            let (kind, size, content_fingerprint) = if metadata.file_type().is_file() {
                let size = metadata.len();
                if size > self.limits.max_untracked_file_bytes
                    || total_bytes.saturating_add(size) > self.limits.max_untracked_total_bytes
                {
                    return Err(ExecutionError::UntrackedStateLimitExceeded);
                }
                let fingerprint = hash_file(&absolute, size, self.limits.max_untracked_file_bytes)
                    .map_err(|source| ExecutionError::UntrackedFileUnreadable {
                        path: relative.clone(),
                        source,
                    })?;
                total_bytes = total_bytes.saturating_add(size);
                ("file", size, fingerprint)
            } else if metadata.file_type().is_symlink() {
                let target = fs::read_link(&absolute).map_err(|source| {
                    ExecutionError::UntrackedFileUnreadable {
                        path: relative.clone(),
                        source,
                    }
                })?;
                let target_bytes = target.to_string_lossy().into_owned().into_bytes();
                let size = target_bytes.len() as u64;
                if size > self.limits.max_untracked_file_bytes
                    || total_bytes.saturating_add(size) > self.limits.max_untracked_total_bytes
                {
                    return Err(ExecutionError::UntrackedStateLimitExceeded);
                }
                let fingerprint =
                    hash_fields("astra-untracked-symlink-v1", &[("target", &target_bytes)]);
                total_bytes = total_bytes.saturating_add(size);
                ("symlink", size, fingerprint)
            } else {
                return Err(ExecutionError::UntrackedFileUnreadable {
                    path: relative,
                    source: io::Error::other("unsupported untracked file type"),
                });
            };
            records.push((
                relative.to_string_lossy().into_owned(),
                kind,
                size,
                content_fingerprint,
            ));
        }

        let mut field_values = Vec::with_capacity(records.len() * 4);
        for (path, kind, size, content_fingerprint) in &records {
            let size = size.to_string();
            field_values.push(("path".to_string(), path.as_bytes().to_vec()));
            field_values.push(("kind".to_string(), kind.as_bytes().to_vec()));
            field_values.push(("size".to_string(), size.into_bytes()));
            field_values.push((
                "content".to_string(),
                content_fingerprint.as_str().as_bytes().to_vec(),
            ));
        }
        let fields = field_values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_slice()))
            .collect::<Vec<_>>();
        Ok(hash_fields("astra-git-untracked-v1", &fields))
    }

    fn run(
        &self,
        root: &Path,
        arguments: Vec<String>,
        output_limit: usize,
    ) -> Result<GitOutput, ExecutionError> {
        self.runner.run(root, &arguments, output_limit)
    }
}

fn canonical_directory(path: &Path) -> Result<PathBuf, ExecutionError> {
    let canonical = fs::canonicalize(path)
        .map_err(|_| ExecutionError::InvalidProjectRoot(path.to_path_buf()))?;
    if !fs::metadata(&canonical)
        .map_err(|_| ExecutionError::InvalidProjectRoot(path.to_path_buf()))?
        .is_dir()
    {
        return Err(ExecutionError::InvalidProjectRoot(path.to_path_buf()));
    }
    Ok(canonical)
}

fn pathspec(relative: &Path) -> Option<String> {
    let _ = relative;
    Some(".".to_string())
}

fn output_success(output: &GitOutput) -> bool {
    output.status_code == Some(0)
}

fn single_line(bytes: &[u8], label: &str) -> Result<String, ExecutionError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ExecutionError::MalformedGitOutput(format!("{label} was not UTF-8")))?;
    let text = text.strip_suffix('\n').unwrap_or(text);
    let text = text.strip_suffix('\r').unwrap_or(text);
    if text.is_empty() || text.contains(['\n', '\r', '\0']) {
        return Err(ExecutionError::MalformedGitOutput(format!(
            "Git returned an invalid {label}"
        )));
    }
    Ok(text.to_string())
}

fn parse_nul_paths(bytes: &[u8]) -> Result<Vec<String>, ExecutionError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if !bytes.ends_with(&[0]) {
        return Err(ExecutionError::MalformedGitOutput(
            "untracked file output was not NUL-terminated".to_string(),
        ));
    }
    bytes[..bytes.len() - 1]
        .split(|byte| *byte == 0)
        .map(|path| {
            let path = std::str::from_utf8(path).map_err(|_| {
                ExecutionError::MalformedGitOutput("untracked path was not valid UTF-8".to_string())
            })?;
            if path.is_empty() {
                return Err(ExecutionError::MalformedGitOutput(
                    "untracked output contained an empty path".to_string(),
                ));
            }
            Ok(path.to_string())
        })
        .collect()
}

fn project_relative_path(repository_relative: &str) -> Result<PathBuf, ExecutionError> {
    let repository_path = Path::new(repository_relative);
    if repository_path.is_absolute()
        || repository_path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(ExecutionError::MalformedGitOutput(
            "Git returned an unsafe untracked path".to_string(),
        ));
    }
    Ok(repository_path.to_path_buf())
}

fn hash_file(path: &Path, size: u64, limit: u64) -> io::Result<Fingerprint> {
    if size > limit {
        return Err(io::Error::other(
            "untracked file exceeds the supported size",
        ));
    }
    let mut file = File::open(path)?;
    let mut bytes = Vec::with_capacity(size as usize);
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() as u64 > limit {
            return Err(io::Error::other(
                "untracked file exceeds the supported size",
            ));
        }
    }
    Ok(hash_fields(
        "astra-untracked-file-v1",
        &[("content", &bytes)],
    ))
}

fn git_command_error(
    operation: &str,
    output: &GitOutput,
    limits: &StateCaptureLimits,
) -> ExecutionError {
    let stderr = String::from_utf8_lossy(&output.stderr)
        .chars()
        .take(limits.max_git_error_bytes)
        .collect::<String>();
    ExecutionError::GitStateCapture(format!(
        "{operation} failed with status {}{}",
        output
            .status_code
            .map_or_else(|| "unknown".to_string(), |status| status.to_string()),
        if stderr.is_empty() {
            String::new()
        } else {
            format!(": {stderr}")
        }
    ))
}

fn run_bounded_git(
    root: &Path,
    arguments: &[String],
    output_limit: usize,
) -> Result<GitOutput, ExecutionError> {
    let mut command = Command::new("git");
    command
        .args(arguments)
        .current_dir(root)
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
    let mut child = command
        .spawn()
        .map_err(|source| ExecutionError::GitStateCapture(source.to_string()))?;
    let stdout = child.stdout.take().ok_or_else(|| {
        ExecutionError::GitStateCapture("Git stdout pipe unavailable".to_string())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        ExecutionError::GitStateCapture("Git stderr pipe unavailable".to_string())
    })?;
    let stdout_thread = thread::spawn(move || read_bounded(stdout, output_limit));
    let stderr_thread = thread::spawn(move || read_bounded(stderr, output_limit));
    let deadline = Instant::now()
        .checked_add(crate::state::MAX_GIT_COMMAND_DURATION)
        .ok_or_else(|| ExecutionError::GitStateCapture("Git timeout overflow".to_string()))?;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|source| ExecutionError::GitStateCapture(source.to_string()))?
        {
            break status;
        }
        if Instant::now() >= deadline {
            if let Err(error) = child.kill() {
                if child
                    .try_wait()
                    .map_err(|source| ExecutionError::GitStateCapture(source.to_string()))?
                    .is_none()
                {
                    return Err(ExecutionError::GitStateCapture(error.to_string()));
                }
            }
            let _ = child
                .wait()
                .map_err(|source| ExecutionError::GitStateCapture(source.to_string()))?;
            join_reader_if_finished(stdout_thread);
            join_reader_if_finished(stderr_thread);
            return Err(ExecutionError::GitCommandTimedOut(
                arguments
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "command".to_string()),
            ));
        }
        thread::sleep(
            Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())),
        );
    };
    let (stdout, stdout_truncated) = stdout_thread
        .join()
        .map_err(|_| ExecutionError::GitStateCapture("Git stdout reader panicked".to_string()))?
        .map_err(|source| ExecutionError::GitStateCapture(source.to_string()))?;
    let (stderr, stderr_truncated) = stderr_thread
        .join()
        .map_err(|_| ExecutionError::GitStateCapture("Git stderr reader panicked".to_string()))?
        .map_err(|source| ExecutionError::GitStateCapture(source.to_string()))?;
    Ok(GitOutput {
        status_code: status.code(),
        stdout,
        stderr,
        truncated: stdout_truncated || stderr_truncated,
    })
}

fn join_reader_if_finished(handle: thread::JoinHandle<io::Result<(Vec<u8>, bool)>>) {
    if handle.is_finished() {
        let _ = handle.join();
    } else {
        drop(handle);
    }
}

fn read_bounded(mut reader: impl Read, limit: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut bytes = Vec::with_capacity(limit.min(8192));
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        let retained = remaining.min(read);
        bytes.extend_from_slice(&buffer[..retained]);
        truncated |= retained < read;
    }
    Ok((bytes, truncated))
}

fn null_device() -> &'static str {
    if cfg!(windows) {
        "NUL"
    } else {
        "/dev/null"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, fs, path::Path, process::Command};

    #[derive(Debug)]
    struct FakeGitRunner {
        outputs: std::cell::RefCell<VecDeque<GitOutput>>,
    }

    impl GitRunner for FakeGitRunner {
        fn run(
            &self,
            _root: &Path,
            _arguments: &[String],
            _output_limit: usize,
        ) -> Result<GitOutput, ExecutionError> {
            self.outputs.borrow_mut().pop_front().ok_or_else(|| {
                ExecutionError::GitStateCapture("unexpected Git invocation".to_string())
            })
        }
    }

    #[test]
    fn parses_nul_delimited_paths_without_newline_assumptions() {
        assert_eq!(
            parse_nul_paths(b"one path\0nested/two\0").expect("paths"),
            vec!["one path", "nested/two"]
        );
        assert!(parse_nul_paths(b"missing terminator").is_err());
    }

    #[test]
    fn fake_git_capture_rejects_non_git_state() {
        let runner = FakeGitRunner {
            outputs: std::cell::RefCell::new(VecDeque::from([GitOutput {
                status_code: Some(128),
                stdout: Vec::new(),
                stderr: b"not a git repository".to_vec(),
                truncated: false,
            }])),
        };
        let capture = GitStateCapture::with_runner(Box::new(runner), StateCaptureLimits::default());
        let root = tempfile::tempdir().expect("root");
        assert!(matches!(
            capture.capture(root.path()),
            Err(ExecutionError::NonGitExecutionUnsupported)
        ));
    }

    fn git_fixture() -> tempfile::TempDir {
        let directory = tempfile::tempdir().expect("Git fixture");
        fs::create_dir_all(directory.path().join("src")).expect("source directory");
        fs::write(directory.path().join("src/lib.rs"), "pub fn value() {}\n").expect("source");
        git(directory.path(), &["init", "-q"]);
        git(directory.path(), &["config", "user.name", "Astra Test"]);
        git(
            directory.path(),
            &["config", "user.email", "astra@example.invalid"],
        );
        git(directory.path(), &["add", "."]);
        git(directory.path(), &["commit", "-qm", "initial"]);
        directory
    }

    fn git(root: &Path, arguments: &[&str]) {
        let status = Command::new("git")
            .args(arguments)
            .current_dir(root)
            .status()
            .expect("Git command");
        assert!(status.success(), "Git command failed: {arguments:?}");
    }

    #[test]
    fn staged_and_unstaged_fingerprints_are_distinct_and_stable() {
        let fixture = git_fixture();
        let capture = GitStateCapture::default();
        let clean = capture.capture(fixture.path()).expect("clean state");

        fs::write(fixture.path().join("src/lib.rs"), "pub fn value() { 1; }\n")
            .expect("unstaged change");
        let unstaged = capture.capture(fixture.path()).expect("unstaged state");
        assert_ne!(clean.worktree_fingerprint, unstaged.worktree_fingerprint);
        assert_eq!(clean.index_fingerprint, unstaged.index_fingerprint);

        git(fixture.path(), &["add", "src/lib.rs"]);
        let staged = capture.capture(fixture.path()).expect("staged state");
        assert_ne!(unstaged.index_fingerprint, staged.index_fingerprint);
        assert_ne!(clean.combined_fingerprint, staged.combined_fingerprint);
    }

    #[test]
    fn ignored_files_do_not_change_the_source_binding() {
        let fixture = git_fixture();
        fs::write(fixture.path().join(".gitignore"), "ignored/\n").expect("ignore file");
        git(fixture.path(), &["add", "."]);
        git(fixture.path(), &["commit", "-qm", "ignore generated files"]);
        let capture = GitStateCapture::default();
        let before = capture
            .capture(fixture.path())
            .expect("before ignored file");
        fs::create_dir_all(fixture.path().join("ignored")).expect("ignored directory");
        fs::write(fixture.path().join("ignored/cache.bin"), "cache").expect("ignored file");
        let after = capture.capture(fixture.path()).expect("after ignored file");
        assert_eq!(before, after);
    }

    #[test]
    fn untracked_content_and_path_changes_are_bound() {
        let fixture = git_fixture();
        let capture = GitStateCapture::default();
        let clean = capture.capture(fixture.path()).expect("clean state");
        let path = fixture.path().join("untracked file.txt");
        fs::write(&path, "one").expect("untracked file");
        let added = capture.capture(fixture.path()).expect("added state");
        assert_ne!(clean.untracked_fingerprint, added.untracked_fingerprint);
        fs::write(&path, "two").expect("modified untracked file");
        let modified = capture.capture(fixture.path()).expect("modified state");
        assert_ne!(added.untracked_fingerprint, modified.untracked_fingerprint);
        let renamed = fixture.path().join("renamed file.txt");
        fs::rename(&path, &renamed).expect("renamed untracked file");
        let renamed_state = capture.capture(fixture.path()).expect("renamed state");
        assert_ne!(
            modified.untracked_fingerprint,
            renamed_state.untracked_fingerprint
        );
        fs::remove_file(renamed).expect("removed untracked file");
        let removed = capture.capture(fixture.path()).expect("removed state");
        assert_eq!(removed.untracked_fingerprint, clean.untracked_fingerprint);
    }

    #[test]
    fn repository_head_changes_are_bound_even_when_the_selected_subtree_is_unchanged() {
        let fixture = git_fixture();
        let capture = GitStateCapture::default();
        let before = capture.capture(fixture.path()).expect("before commit");
        fs::write(fixture.path().join("sibling.txt"), "sibling\n").expect("sibling source");
        git(fixture.path(), &["add", "sibling.txt"]);
        git(fixture.path(), &["commit", "-qm", "sibling commit"]);
        let after = capture.capture(fixture.path()).expect("after commit");
        assert_ne!(before.repository_head, after.repository_head);
        assert_ne!(before.combined_fingerprint, after.combined_fingerprint);
    }

    #[test]
    fn nested_project_state_ignores_sibling_changes() {
        let fixture = tempfile::tempdir().expect("Git fixture");
        fs::create_dir_all(fixture.path().join("projects/one/src")).expect("project one");
        fs::create_dir_all(fixture.path().join("projects/two/src")).expect("project two");
        fs::write(fixture.path().join("projects/one/src/lib.rs"), "one\n").expect("one source");
        fs::write(fixture.path().join("projects/two/src/lib.rs"), "two\n").expect("two source");
        git(fixture.path(), &["init", "-q"]);
        git(fixture.path(), &["config", "user.name", "Astra Test"]);
        git(
            fixture.path(),
            &["config", "user.email", "astra@example.invalid"],
        );
        git(fixture.path(), &["add", "."]);
        git(fixture.path(), &["commit", "-qm", "initial"]);
        let selected = fixture.path().join("projects/one");
        let capture = GitStateCapture::default();
        let before = capture.capture(&selected).expect("selected state");
        fs::write(
            fixture.path().join("projects/two/src/lib.rs"),
            "sibling change\n",
        )
        .expect("sibling change");
        let sibling = capture.capture(&selected).expect("sibling state");
        assert_eq!(before, sibling);
        git(fixture.path(), &["add", "projects/two/src/lib.rs"]);
        let staged_sibling = capture.capture(&selected).expect("staged sibling state");
        assert_eq!(before, staged_sibling);
        fs::write(
            fixture.path().join("projects/two/untracked.rs"),
            "untracked sibling\n",
        )
        .expect("untracked sibling change");
        let untracked_sibling = capture.capture(&selected).expect("untracked sibling state");
        assert_eq!(before, untracked_sibling);
        fs::write(selected.join("src/lib.rs"), "selected change\n").expect("selected change");
        let selected_change = capture.capture(&selected).expect("selected change state");
        assert_ne!(before, selected_change);
        git(fixture.path(), &["commit", "-qm", "sibling commit"]);
        let head_changed = capture.capture(&selected).expect("new repository head");
        assert_ne!(
            selected_change.repository_head,
            head_changed.repository_head
        );
        assert_ne!(
            selected_change.combined_fingerprint,
            head_changed.combined_fingerprint
        );
    }

    #[test]
    fn untracked_limits_refuse_oversized_files_and_excess_file_count() {
        let fixture = git_fixture();
        let limits = StateCaptureLimits {
            max_untracked_file_bytes: 4,
            max_untracked_total_bytes: 8,
            max_untracked_files: 1,
            ..StateCaptureLimits::default()
        };
        let capture = GitStateCapture::with_runner(Box::<SystemGitRunner>::default(), limits);
        fs::write(fixture.path().join("large.txt"), "12345").expect("large untracked file");
        assert!(matches!(
            capture.capture(fixture.path()),
            Err(ExecutionError::UntrackedStateLimitExceeded)
        ));
        fs::remove_file(fixture.path().join("large.txt")).expect("large file");
        fs::write(fixture.path().join("one.txt"), "1").expect("first untracked file");
        fs::write(fixture.path().join("two.txt"), "2").expect("second untracked file");
        assert!(matches!(
            capture.capture(fixture.path()),
            Err(ExecutionError::UntrackedStateLimitExceeded)
        ));
    }

    #[test]
    fn unreadable_relevant_untracked_paths_are_not_silently_omitted() {
        let root = tempfile::tempdir().expect("root");
        let root_output = format!("{}\n", root.path().display()).into_bytes();
        let runner = FakeGitRunner {
            outputs: std::cell::RefCell::new(VecDeque::from([
                GitOutput {
                    status_code: Some(0),
                    stdout: root_output,
                    stderr: Vec::new(),
                    truncated: false,
                },
                GitOutput {
                    status_code: Some(0),
                    stdout: b"0123456789012345678901234567890123456789\n".to_vec(),
                    stderr: Vec::new(),
                    truncated: false,
                },
                GitOutput {
                    status_code: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    truncated: false,
                },
                GitOutput {
                    status_code: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    truncated: false,
                },
                GitOutput {
                    status_code: Some(0),
                    stdout: b"missing file\0".to_vec(),
                    stderr: Vec::new(),
                    truncated: false,
                },
            ])),
        };
        let capture = GitStateCapture::with_runner(Box::new(runner), StateCaptureLimits::default());
        assert!(matches!(
            capture.capture(root.path()),
            Err(ExecutionError::UntrackedFileUnreadable { .. })
        ));
    }
}
