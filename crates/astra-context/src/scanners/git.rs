use crate::{
    facts::{Fact, FactGraphBuilder, FactProvenance, RepositoryFact},
    policy::{excluded_path, normalized_relative, sensitive_path},
    process::{CommandInvocation, ProcessOutput, ProcessRunner},
    scanner::metadata,
    scope::SemanticScope,
    Confidence, Diagnostic, DiagnosticSeverity, Evidence, EvidenceSource, ScanOptions,
    ScannerResult, ScannerStatus,
};
use std::{io, path::Path};

const MAX_REPOSITORY_ROOT_CHARS: usize = 4_096;
const MAX_BRANCH_CHARS: usize = 1_024;
const MAX_STATUS_PATH_CHARS: usize = 4_096;
const MAX_COMMIT_SUBJECT_CHARS: usize = 512;
const MAX_RFC3339_CHARS: usize = 64;

pub(crate) struct GitOutput {
    pub(crate) result: ScannerResult,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

pub(crate) fn scan(
    root: &Path,
    options: &ScanOptions,
    runner: &dyn ProcessRunner,
    builder: &mut FactGraphBuilder,
) -> GitOutput {
    if find_git_marker(root).is_none() {
        add_repository_fact(builder, RepositoryFact::State("not_repository".to_string()));
        return GitOutput {
            result: result(ScannerStatus::NotApplicable, 1, &[]),
            diagnostics: Vec::new(),
        };
    }

    let mut diagnostics = Vec::new();
    let mut findings = 0;
    let root_output = match execute(runner, root, options, &["rev-parse", "--show-toplevel"]) {
        Ok(output) if output.success => output,
        Ok(output) => {
            diagnostics.push(command_diagnostic("git.root_failed", &output));
            add_repository_fact(builder, RepositoryFact::State("partial".to_string()));
            return finish(ScannerStatus::Partial, 1, diagnostics);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            diagnostics.push(Diagnostic::new(
                "git",
                "git.executable_unavailable",
                DiagnosticSeverity::Warning,
                None,
                "Git executable is unavailable",
            ));
            add_repository_fact(
                builder,
                RepositoryFact::State("git_unavailable".to_string()),
            );
            return finish(ScannerStatus::Unavailable, 1, diagnostics);
        }
        Err(error) => {
            diagnostics.push(Diagnostic::new(
                "git",
                "git.execution_failed",
                DiagnosticSeverity::Warning,
                None,
                bounded_text(error.to_string().as_bytes()),
            ));
            add_repository_fact(builder, RepositoryFact::State("partial".to_string()));
            return finish(ScannerStatus::Partial, 1, diagnostics);
        }
    };
    if root_output.truncated {
        diagnostics.push(truncated_diagnostic("repository root"));
        add_repository_fact(builder, RepositoryFact::State("partial".to_string()));
        return finish(ScannerStatus::Partial, 1, diagnostics);
    }

    let repository_root = match bounded_line(&root_output.stdout, MAX_REPOSITORY_ROOT_CHARS, false)
    {
        Some(value) => value,
        None => {
            diagnostics.push(Diagnostic::new(
                "git",
                "git.malformed_root",
                DiagnosticSeverity::Warning,
                None,
                "Git returned an empty or malformed repository root",
            ));
            add_repository_fact(builder, RepositoryFact::State("partial".to_string()));
            return finish(ScannerStatus::Partial, 1, diagnostics);
        }
    };
    let repository_root_path = Path::new(&repository_root);
    if !repository_root_path.is_absolute() || !root.starts_with(repository_root_path) {
        diagnostics.push(Diagnostic::new(
            "git",
            "git.invalid_root",
            DiagnosticSeverity::Warning,
            None,
            "Git returned a repository root that does not contain the selected project root",
        ));
        add_repository_fact(builder, RepositoryFact::State("partial".to_string()));
        return finish(ScannerStatus::Partial, 1, diagnostics);
    }
    let selected_prefix = root
        .strip_prefix(repository_root_path)
        .ok()
        .and_then(normalized_relative)
        .unwrap_or_default();
    add_repository_fact(builder, RepositoryFact::State("git".to_string()));
    add_repository_fact(builder, RepositoryFact::Root(repository_root));
    findings += 2;

    match execute(runner, root, options, &["branch", "--show-current"]) {
        Ok(output) if output.success => {
            if output.truncated {
                diagnostics.push(truncated_diagnostic("branch"));
            } else if let Some(branch) = bounded_line(&output.stdout, MAX_BRANCH_CHARS, true) {
                if !branch.is_empty() {
                    add_repository_fact(builder, RepositoryFact::Branch(branch));
                    findings += 1;
                }
            } else {
                diagnostics.push(Diagnostic::new(
                    "git",
                    "git.malformed_branch",
                    DiagnosticSeverity::Warning,
                    None,
                    "Git returned a malformed branch name",
                ));
            }
        }
        Ok(output) => diagnostics.push(command_diagnostic("git.branch_failed", &output)),
        Err(error) => diagnostics.push(io_diagnostic("git.branch_failed", &error)),
    }

    match execute(runner, root, options, &["rev-parse", "HEAD"]) {
        Ok(output) if output.success => {
            if output.truncated {
                diagnostics.push(truncated_diagnostic("HEAD"));
            } else if let Some(head) =
                bounded_line(&output.stdout, 64, false).filter(|head| valid_object_id(head))
            {
                add_repository_fact(builder, RepositoryFact::Head(head));
                findings += 1;
            } else {
                diagnostics.push(Diagnostic::new(
                    "git",
                    "git.malformed_head",
                    DiagnosticSeverity::Warning,
                    None,
                    "Git returned an empty or malformed HEAD identifier",
                ));
            }
        }
        Ok(output) => diagnostics.push(command_diagnostic("git.head_failed", &output)),
        Err(error) => diagnostics.push(io_diagnostic("git.head_failed", &error)),
    }

    match execute(
        runner,
        root,
        options,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
        ],
    ) {
        Ok(output) if output.success && output.truncated => {
            diagnostics.push(truncated_diagnostic("working-tree status"));
        }
        Ok(output) if output.success => match parse_status(&output.stdout) {
            Ok(changes) => {
                let clean = changes.is_empty();
                add_repository_fact(builder, RepositoryFact::Clean(clean));
                findings += 1;
                for (status, path) in changes {
                    let Some(path) = selected_status_path(&path, &selected_prefix) else {
                        diagnostics.push(Diagnostic::new(
                            "git",
                            "git.path_outside_root",
                            DiagnosticSeverity::Warning,
                            None,
                            "Git returned a change outside the selected project root",
                        ));
                        continue;
                    };
                    let project_path = Path::new(&path);
                    if sensitive_path(project_path) || excluded_path(project_path) {
                        continue;
                    }
                    add_repository_fact(builder, RepositoryFact::Change { path, status });
                    findings += 1;
                }
            }
            Err(message) => diagnostics.push(Diagnostic::new(
                "git",
                "git.malformed_status",
                DiagnosticSeverity::Warning,
                None,
                message,
            )),
        },
        Ok(output) => diagnostics.push(command_diagnostic("git.status_failed", &output)),
        Err(error) => diagnostics.push(io_diagnostic("git.status_failed", &error)),
    }

    let limit = options.recent_commit_limit.to_string();
    match execute(
        runner,
        root,
        options,
        &[
            "log",
            "-n",
            &limit,
            "--format=%H%x1f%aI%x1f%s%x1e",
            "--",
            ".",
        ],
    ) {
        Ok(output) if output.success && output.truncated => {
            diagnostics.push(truncated_diagnostic("recent commit log"));
        }
        Ok(output) if output.success => match parse_log(&output.stdout) {
            Ok(commits) => {
                for (ordinal, (id, authored_at, subject)) in commits.into_iter().enumerate() {
                    add_repository_fact(
                        builder,
                        RepositoryFact::Commit {
                            ordinal,
                            id,
                            authored_at,
                            subject,
                        },
                    );
                    findings += 1;
                }
            }
            Err(message) => diagnostics.push(Diagnostic::new(
                "git",
                "git.malformed_log",
                DiagnosticSeverity::Warning,
                None,
                message,
            )),
        },
        Ok(output) => diagnostics.push(command_diagnostic("git.log_failed", &output)),
        Err(error) => diagnostics.push(io_diagnostic("git.log_failed", &error)),
    }

    let status = if diagnostics.is_empty() {
        ScannerStatus::Complete
    } else {
        add_repository_fact(builder, RepositoryFact::State("partial".to_string()));
        ScannerStatus::Partial
    };
    finish(status, findings, diagnostics)
}

fn execute(
    runner: &dyn ProcessRunner,
    root: &Path,
    options: &ScanOptions,
    arguments: &[&str],
) -> io::Result<ProcessOutput> {
    let mut invocation_arguments = vec![
        "--no-optional-locks".to_string(),
        "-c".to_string(),
        "core.fsmonitor=false".to_string(),
        "-c".to_string(),
        "core.untrackedCache=false".to_string(),
        "-c".to_string(),
        "status.relativePaths=false".to_string(),
        "-C".to_string(),
        root.to_string_lossy().into_owned(),
    ];
    invocation_arguments.extend(arguments.iter().map(|argument| (*argument).to_string()));
    runner.run(
        &CommandInvocation {
            executable: "git".to_string(),
            arguments: invocation_arguments,
            current_directory: root.to_path_buf(),
        },
        options.git_timeout,
        options.max_git_output_bytes,
    )
}

fn find_git_marker(root: &Path) -> Option<&Path> {
    root.ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
}

fn bounded_line(bytes: &[u8], max_chars: usize, allow_empty: bool) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let text = text.strip_suffix('\n').unwrap_or(text);
    let text = text.strip_suffix('\r').unwrap_or(text);
    safe_bounded_value(text, max_chars, allow_empty).then(|| text.to_string())
}

fn parse_status(bytes: &[u8]) -> Result<Vec<(String, String)>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let Some(records_bytes) = bytes.strip_suffix(&[0]) else {
        return Err("Git status output was not NUL-terminated".to_string());
    };
    let mut records = records_bytes.split(|byte| *byte == 0);
    let mut changes = Vec::new();
    while let Some(record) = records.next() {
        if record.is_empty() {
            return Err("Git status contained an empty record".to_string());
        }
        if record.len() < 4 || record.get(2) != Some(&b' ') {
            return Err("invalid Git status record shape".to_string());
        }
        let status_bytes = &record[..2];
        if !valid_status_code(status_bytes) {
            return Err("Git status contained an unsupported XY status code".to_string());
        }
        let status = std::str::from_utf8(status_bytes)
            .map_err(|_| "Git status code was not ASCII".to_string())?
            .to_string();
        let path = parse_status_path(&record[3..])?;
        if status_bytes.contains(&b'R') || status_bytes.contains(&b'C') {
            let old_path = records
                .next()
                .ok_or_else(|| "rename status omitted its source path".to_string())?;
            parse_status_path(old_path)?;
        }
        changes.push((status, path));
    }
    changes.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
    Ok(changes)
}

fn selected_status_path(path: &str, selected_prefix: &str) -> Option<String> {
    let repository_relative = normalized_relative(Path::new(path))?;
    if selected_prefix.is_empty() {
        return (!repository_relative.is_empty()).then_some(repository_relative);
    }
    repository_relative
        .strip_prefix(selected_prefix)?
        .strip_prefix('/')
        .filter(|relative| !relative.is_empty())
        .map(str::to_string)
}

fn parse_log(bytes: &[u8]) -> Result<Vec<(String, String, String)>, String> {
    let text =
        std::str::from_utf8(bytes).map_err(|error| format!("Git log was not UTF-8: {error}"))?;
    let text = text.trim_end_matches(['\r', '\n']);
    if text.is_empty() {
        return Ok(Vec::new());
    }
    if !text.ends_with('\u{1e}') {
        return Err("Git log output was not record-terminated".to_string());
    }

    let mut commits = Vec::new();
    for record in text.split_terminator('\u{1e}') {
        let record = record.trim_start_matches(['\r', '\n']);
        let mut fields = record.split('\u{1f}');
        let Some(id) = fields.next() else {
            return Err("Git log record did not contain id, timestamp, and subject".to_string());
        };
        let Some(authored_at) = fields.next() else {
            return Err("Git log record did not contain id, timestamp, and subject".to_string());
        };
        let Some(subject) = fields.next() else {
            return Err("Git log record did not contain id, timestamp, and subject".to_string());
        };
        if fields.next().is_some()
            || !valid_object_id(id)
            || !valid_rfc3339(authored_at)
            || !safe_bounded_value(subject, MAX_COMMIT_SUBJECT_CHARS, true)
        {
            return Err("Git log record did not contain id, timestamp, and subject".to_string());
        }
        commits.push((id.to_string(), authored_at.to_string(), subject.to_string()));
    }
    Ok(commits)
}

fn parse_status_path(bytes: &[u8]) -> Result<String, String> {
    let path = std::str::from_utf8(bytes)
        .map_err(|error| format!("Git status path was not UTF-8: {error}"))?;
    if !safe_bounded_value(path, MAX_STATUS_PATH_CHARS, false) {
        return Err("Git status contained an empty, unsafe, or oversized path".to_string());
    }
    Ok(path.to_string())
}

fn valid_status_code(status: &[u8]) -> bool {
    if matches!(
        status,
        b"??" | b"!!" | b"DD" | b"AU" | b"UD" | b"UA" | b"DU" | b"AA" | b"UU"
    ) {
        return true;
    }
    let [index, worktree] = status else {
        return false;
    };
    b" MTADRC".contains(index)
        && b" MTADRC".contains(worktree)
        && (*index != b' ' || *worktree != b' ')
}

fn valid_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_rfc3339(value: &str) -> bool {
    if !safe_bounded_value(value, MAX_RFC3339_CHARS, false) {
        return false;
    }
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || !matches!(bytes.get(10), Some(b'T' | b't'))
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return false;
    }

    let Some(year) = parse_decimal(bytes, 0, 4) else {
        return false;
    };
    let Some(month) = parse_decimal(bytes, 5, 2) else {
        return false;
    };
    let Some(day) = parse_decimal(bytes, 8, 2) else {
        return false;
    };
    let Some(hour) = parse_decimal(bytes, 11, 2) else {
        return false;
    };
    let Some(minute) = parse_decimal(bytes, 14, 2) else {
        return false;
    };
    let Some(second) = parse_decimal(bytes, 17, 2) else {
        return false;
    };
    if !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return false;
    }

    let mut zone_start = 19;
    if bytes.get(zone_start) == Some(&b'.') {
        zone_start += 1;
        let fraction_start = zone_start;
        while matches!(bytes.get(zone_start), Some(byte) if byte.is_ascii_digit()) {
            zone_start += 1;
        }
        if zone_start == fraction_start {
            return false;
        }
    }

    match bytes.get(zone_start) {
        Some(b'Z' | b'z') => zone_start + 1 == bytes.len(),
        Some(b'+' | b'-') => {
            bytes.len() == zone_start + 6
                && bytes.get(zone_start + 3) == Some(&b':')
                && parse_decimal(bytes, zone_start + 1, 2).is_some_and(|hours| hours <= 23)
                && parse_decimal(bytes, zone_start + 4, 2).is_some_and(|minutes| minutes <= 59)
        }
        _ => false,
    }
}

fn parse_decimal(bytes: &[u8], start: usize, length: usize) -> Option<u32> {
    let digits = bytes.get(start..start.checked_add(length)?)?;
    digits.iter().try_fold(0_u32, |value, byte| {
        byte.is_ascii_digit()
            .then(|| value * 10 + u32::from(*byte - b'0'))
    })
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn safe_bounded_value(value: &str, max_chars: usize, allow_empty: bool) -> bool {
    (allow_empty || !value.is_empty())
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control)
}

fn add_repository_fact(builder: &mut FactGraphBuilder, fact: RepositoryFact) {
    let (source, rule) = if matches!(
        &fact,
        RepositoryFact::State(state) if state == "not_repository"
    ) {
        (EvidenceSource::Convention, "git.marker_absent")
    } else {
        (EvidenceSource::Git, "git.machine_readable_query")
    };
    builder.add_fact(
        Fact::Repository(fact),
        FactProvenance {
            scanner: "git".to_string(),
            scope: SemanticScope::Primary,
            confidence: Confidence::Certain,
            evidence: vec![Evidence {
                source,
                path: None,
                locator: None,
                rule: rule.to_string(),
            }],
        },
    );
}

fn command_diagnostic(code: &str, output: &ProcessOutput) -> Diagnostic {
    let message = if output.timed_out {
        "Git command timed out".to_string()
    } else {
        format!(
            "Git command failed with status {}: {}",
            output
                .status_code
                .map_or_else(|| "unknown".to_string(), |code| code.to_string()),
            bounded_text(&output.stderr)
        )
    };
    Diagnostic::new("git", code, DiagnosticSeverity::Warning, None, message)
}

fn truncated_diagnostic(operation: &str) -> Diagnostic {
    Diagnostic::new(
        "git",
        "git.output_truncated",
        DiagnosticSeverity::Warning,
        None,
        format!("Git {operation} output exceeded the configured limit"),
    )
}

fn io_diagnostic(code: &str, error: &io::Error) -> Diagnostic {
    Diagnostic::new(
        "git",
        code,
        DiagnosticSeverity::Warning,
        None,
        bounded_text(error.to_string().as_bytes()),
    )
}

fn bounded_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .take(512)
        .collect::<String>()
}

fn finish(status: ScannerStatus, findings: usize, diagnostics: Vec<Diagnostic>) -> GitOutput {
    let mut diagnostic_codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.clone())
        .collect::<Vec<_>>();
    diagnostic_codes.sort();
    diagnostic_codes.dedup();
    GitOutput {
        result: result(status, findings, &diagnostic_codes),
        diagnostics,
    }
}

fn result(status: ScannerStatus, findings: usize, diagnostic_codes: &[String]) -> ScannerResult {
    ScannerResult {
        metadata: metadata(
            "git",
            1,
            "Collects bounded machine-readable Git repository observations",
        ),
        status,
        findings,
        diagnostic_codes: diagnostic_codes.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        facts::{FactGraphBuilder, FactKind},
        process::tests::{output, FakeProcessRunner},
    };
    use std::{fs, path::PathBuf};

    const TEST_OBJECT_ID: &str = "0123456789abcdef0123456789abcdef01234567";

    fn isolated_tempdir() -> tempfile::TempDir {
        let mut bases = Vec::new();
        #[cfg(unix)]
        {
            bases.push(PathBuf::from("/tmp"));
            bases.push(PathBuf::from("/var/tmp"));
        }
        bases.push(std::env::temp_dir());

        for base in bases {
            let Ok(base) = base.canonicalize() else {
                continue;
            };
            if base
                .ancestors()
                .any(|ancestor| ancestor.join(".git").exists())
            {
                continue;
            }
            if let Ok(directory) = tempfile::Builder::new()
                .prefix("astra-context-git-test-")
                .tempdir_in(base)
            {
                return directory;
            }
        }

        panic!("no writable temporary directory without a .git ancestor was available");
    }

    fn repository_root_output(path: &Path) -> ProcessOutput {
        output(true, &format!("{}\n", path.display()), "")
    }

    fn head_output() -> ProcessOutput {
        output(true, &format!("{TEST_OBJECT_ID}\n"), "")
    }

    fn log_output() -> ProcessOutput {
        output(
            true,
            &format!("{TEST_OBJECT_ID}\u{1f}2026-01-02T03:04:05Z\u{1f}message\u{1e}"),
            "",
        )
    }

    #[test]
    fn non_repository_does_not_execute_git() {
        let directory = isolated_tempdir();
        let runner = FakeProcessRunner::default();
        let mut builder = FactGraphBuilder::new();

        let result = scan(
            directory.path(),
            &ScanOptions::default(),
            &runner,
            &mut builder,
        );
        assert_eq!(result.result.status, ScannerStatus::NotApplicable);
        assert!(runner.invocations().is_empty());
    }

    #[test]
    fn parses_repository_state_without_real_git() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::create_dir(directory.path().join(".git")).expect("git marker");
        let runner = FakeProcessRunner::with_outputs(vec![
            repository_root_output(directory.path()),
            output(true, "main\n", ""),
            head_output(),
            output(true, " M src/lib.rs\0?? new.txt\0", ""),
            log_output(),
        ]);
        let mut builder = FactGraphBuilder::new();

        let result = scan(
            directory.path(),
            &ScanOptions::default(),
            &runner,
            &mut builder,
        );
        let graph = builder.finish().expect("graph");
        assert_eq!(result.result.status, ScannerStatus::Complete);
        assert!(graph.facts_of_kind(FactKind::Repository).len() >= 7);
        let invocations = runner.invocations();
        assert_eq!(invocations.len(), 5);
        assert!(invocations
            .iter()
            .all(|invocation| invocation.executable == "git"
                && invocation.current_directory == directory.path()));
        assert!(invocations[3]
            .arguments
            .ends_with(&["--".to_string(), ".".to_string()]));
        assert!(invocations.iter().all(|invocation| invocation
            .arguments
            .windows(2)
            .any(|arguments| arguments == ["-c", "status.relativePaths=false"])));
        assert!(invocations[4]
            .arguments
            .ends_with(&["--".to_string(), ".".to_string()]));
    }

    #[test]
    fn status_and_log_parsers_reject_malformed_output() {
        assert!(parse_status(b"bad\0").is_err());
        assert!(parse_log(b"only-one-field\x1e").is_err());
    }

    #[test]
    fn object_ids_accept_sha1_and_sha256_only() {
        assert!(valid_object_id(TEST_OBJECT_ID));
        assert!(valid_object_id(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert!(!valid_object_id("0123456789abcdef"));
        assert!(!valid_object_id("g123456789abcdef0123456789abcdef01234567"));
    }

    #[test]
    fn rfc3339_validation_checks_calendar_time_and_offset() {
        for timestamp in [
            "2026-01-02T03:04:05Z",
            "2024-02-29t23:59:60z",
            "2026-01-02T03:04:05.123456+05:30",
            "2026-01-02T03:04:05-00:00",
        ] {
            assert!(valid_rfc3339(timestamp), "{timestamp}");
        }
        for timestamp in [
            "2026-02-29T03:04:05Z",
            "2026-13-02T03:04:05Z",
            "2026-01-02T24:04:05Z",
            "2026-01-02T03:04:05+24:00",
            "2026-01-02T03:04:05",
            "2026-01-02T03:04:05.Z",
        ] {
            assert!(!valid_rfc3339(timestamp), "{timestamp}");
        }
    }

    #[test]
    fn status_parser_validates_codes_paths_and_nul_framing() {
        let changes = parse_status(
            b" M modified\0A  added\0R  renamed\0old-name\0 A intent\0 R moved\0old-moved\0 C copied\0old-copied\0UU conflict\0?? untracked\0!! ignored\0",
        )
        .expect("valid porcelain records");
        assert_eq!(changes.len(), 9);
        assert!(parse_status(b"ZZ unsupported\0").is_err());
        assert!(parse_status(b" M not-terminated").is_err());
        assert!(parse_status(b"R  missing-old\0").is_err());
        assert!(parse_status(b" M unsafe\npath\0").is_err());
    }

    #[test]
    fn log_parser_validates_ids_timestamps_and_safe_subjects() {
        let sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let records = format!(
            "{TEST_OBJECT_ID}\u{1f}2026-01-02T03:04:05Z\u{1f}\u{1e}\n\
             {sha256}\u{1f}2026-01-02T03:04:05+00:00\u{1f}second\u{1e}\n"
        );
        let commits = parse_log(records.as_bytes()).expect("valid records");
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].2, "");

        let short_id = "abc\u{1f}2026-01-02T03:04:05Z\u{1f}subject\u{1e}";
        assert!(parse_log(short_id.as_bytes()).is_err());
        let bad_timestamp =
            format!("{TEST_OBJECT_ID}\u{1f}2026-02-30T03:04:05Z\u{1f}subject\u{1e}");
        assert!(parse_log(bad_timestamp.as_bytes()).is_err());
        let unsafe_subject =
            format!("{TEST_OBJECT_ID}\u{1f}2026-01-02T03:04:05Z\u{1f}line\nbreak\u{1e}");
        assert!(parse_log(unsafe_subject.as_bytes()).is_err());
        let oversized_subject = format!(
            "{TEST_OBJECT_ID}\u{1f}2026-01-02T03:04:05Z\u{1f}{}\u{1e}",
            "x".repeat(MAX_COMMIT_SUBJECT_CHARS + 1)
        );
        assert!(parse_log(oversized_subject.as_bytes()).is_err());
    }

    #[test]
    fn bounded_lines_preserve_spaces_and_reject_unsafe_values() {
        assert_eq!(
            bounded_line(b"feature/a b\n", MAX_BRANCH_CHARS, false).as_deref(),
            Some("feature/a b")
        );
        assert_eq!(
            bounded_line(b"\n", MAX_BRANCH_CHARS, true),
            Some(String::new())
        );
        assert!(bounded_line(b"main\nextra\n", MAX_BRANCH_CHARS, false).is_none());
        assert!(bounded_line(b"main\tbranch\n", MAX_BRANCH_CHARS, false).is_none());
        assert!(bounded_line(
            "x".repeat(MAX_BRANCH_CHARS + 1).as_bytes(),
            MAX_BRANCH_CHARS,
            false
        )
        .is_none());
    }

    #[test]
    fn missing_git_executable_is_recoverable() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::create_dir(directory.path().join(".git")).expect("git marker");
        let runner = FakeProcessRunner::with_results(vec![Err(io::Error::new(
            io::ErrorKind::NotFound,
            "git unavailable",
        ))]);
        let mut builder = FactGraphBuilder::new();

        let output = scan(
            directory.path(),
            &ScanOptions::default(),
            &runner,
            &mut builder,
        );
        assert_eq!(output.result.status, ScannerStatus::Unavailable);
        assert_eq!(
            output.result.diagnostic_codes,
            ["git.executable_unavailable"]
        );
    }

    #[test]
    fn timeout_is_reported_as_a_partial_scan() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::create_dir(directory.path().join(".git")).expect("git marker");
        let mut timed_out = output(false, "", "");
        timed_out.timed_out = true;
        let runner = FakeProcessRunner::with_outputs(vec![timed_out]);
        let mut builder = FactGraphBuilder::new();

        let output = scan(
            directory.path(),
            &ScanOptions::default(),
            &runner,
            &mut builder,
        );
        assert_eq!(output.result.status, ScannerStatus::Partial);
        assert!(output.diagnostics[0].message.contains("timed out"));
    }

    #[test]
    fn detached_clean_repository_is_valid() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::create_dir(directory.path().join(".git")).expect("git marker");
        let runner = FakeProcessRunner::with_outputs(vec![
            repository_root_output(directory.path()),
            output(true, "\n", ""),
            head_output(),
            output(true, "", ""),
            output(true, "", ""),
        ]);
        let mut builder = FactGraphBuilder::new();

        let output = scan(
            directory.path(),
            &ScanOptions::default(),
            &runner,
            &mut builder,
        );
        let graph = builder.finish().expect("graph");
        assert_eq!(output.result.status, ScannerStatus::Complete);
        assert!(graph
            .facts_of_kind(FactKind::Repository)
            .into_iter()
            .any(|stored| matches!(&stored.fact, Fact::Repository(RepositoryFact::Clean(true)))));
        assert!(!graph
            .facts_of_kind(FactKind::Repository)
            .into_iter()
            .any(|stored| matches!(&stored.fact, Fact::Repository(RepositoryFact::Branch(_)))));
    }

    #[test]
    fn truncated_optional_output_degrades_without_parsing_it() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::create_dir(directory.path().join(".git")).expect("git marker");
        let mut status = output(true, " M incomplete", "");
        status.truncated = true;
        let runner = FakeProcessRunner::with_outputs(vec![
            repository_root_output(directory.path()),
            output(true, "main\n", ""),
            head_output(),
            status,
            output(true, "", ""),
        ]);
        let mut builder = FactGraphBuilder::new();

        let output = scan(
            directory.path(),
            &ScanOptions::default(),
            &runner,
            &mut builder,
        );
        let graph = builder.finish().expect("graph");
        assert_eq!(output.result.status, ScannerStatus::Partial);
        assert!(output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "git.output_truncated"));
        assert!(!graph
            .facts_of_kind(FactKind::Repository)
            .into_iter()
            .any(|stored| matches!(&stored.fact, Fact::Repository(RepositoryFact::Clean(_)))));
    }

    #[test]
    fn changes_outside_selected_root_are_not_exposed() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::create_dir(directory.path().join(".git")).expect("git marker");
        let selected = directory.path().join("sub");
        fs::create_dir(&selected).expect("selected root");
        let runner = FakeProcessRunner::with_outputs(vec![
            repository_root_output(directory.path()),
            output(true, "main\n", ""),
            head_output(),
            output(true, " M sibling.txt\0 M sub/inside.txt\0", ""),
            output(true, "", ""),
        ]);
        let mut builder = FactGraphBuilder::new();

        let output = scan(&selected, &ScanOptions::default(), &runner, &mut builder);
        let graph = builder.finish().expect("graph");
        let changes = graph
            .facts_of_kind(FactKind::Repository)
            .into_iter()
            .filter_map(|stored| match &stored.fact {
                Fact::Repository(RepositoryFact::Change { path, .. }) => Some(path.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(changes, ["inside.txt"]);
        assert_eq!(output.result.status, ScannerStatus::Partial);
        assert!(output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "git.path_outside_root"));
    }
}
