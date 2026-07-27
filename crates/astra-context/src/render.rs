use crate::{CommandSpec, ContextError, DiagnosticSeverity, ProjectContext, ScanReport};
use std::fmt::Write;

pub fn render_json(report: &ScanReport) -> Result<String, ContextError> {
    let mut output = serde_json::to_string_pretty(report)?;
    output.push('\n');
    Ok(output)
}

pub fn render_text(report: &ScanReport) -> String {
    let context = &report.context;
    let mut output = String::new();
    let _ = writeln!(output, "Project: {}", safe_text(&context.identity.name));
    let _ = writeln!(
        output,
        "Root: {}",
        safe_text(context.identity.root.as_str())
    );
    let _ = writeln!(
        output,
        "Repository: {}{}",
        repository_state(context),
        context
            .repository
            .branch
            .as_ref()
            .map(|branch| format!(" ({})", safe_text(&branch.value)))
            .unwrap_or_default()
    );
    let _ = writeln!(
        output,
        "Size: {} files, {}",
        context.size.value.files,
        human_bytes(context.size.value.bytes)
    );

    section_values(
        &mut output,
        "Languages",
        context.languages.iter().map(|language| {
            format!(
                "{} — {} files, {}",
                language.value.id,
                language.value.file_count,
                human_bytes(language.value.bytes)
            )
        }),
    );
    section_values(
        &mut output,
        "Packages",
        context.workspace.packages.iter().map(|package| {
            format!(
                "{} [{}] — {}",
                package.value.name, package.value.ecosystem, package.value.path
            )
        }),
    );
    section_values(
        &mut output,
        "Build systems",
        context
            .tooling
            .build_systems
            .iter()
            .map(|tool| tool.value.id.clone()),
    );
    section_values(
        &mut output,
        "Testing",
        context
            .tooling
            .testing_frameworks
            .iter()
            .map(|tool| tool.value.id.clone()),
    );
    section_values(
        &mut output,
        "Validation commands",
        context
            .validation_commands
            .iter()
            .map(|command| display_command(&command.value)),
    );
    section_values(
        &mut output,
        "Documentation",
        context
            .documentation
            .iter()
            .map(|document| document.value.path.to_string()),
    );
    section_values(
        &mut output,
        "Insights",
        report.insights.iter().map(|insight| {
            format!(
                "{}: {}",
                severity_name(insight.value.severity),
                insight.value.observation
            )
        }),
    );
    output
}

pub fn render_tree(report: &ScanReport) -> String {
    let context = &report.context;
    let mut output = String::new();
    let _ = writeln!(output, "{}", safe_text(&context.identity.name));
    let _ = writeln!(output, "├── repository");
    let _ = writeln!(output, "│   └── {}", repository_state(context));
    let _ = writeln!(output, "├── workspaces");
    if context.workspace.kinds.is_empty() {
        let _ = writeln!(output, "│   └── none detected");
    } else {
        for (index, workspace) in context.workspace.kinds.iter().enumerate() {
            let connector = branch(index + 1 == context.workspace.kinds.len());
            let _ = writeln!(output, "│   {connector} {}", safe_text(&workspace.value));
        }
    }
    let _ = writeln!(output, "├── packages");
    if context.workspace.packages.is_empty() {
        let _ = writeln!(output, "│   └── none detected");
    } else {
        for (index, package) in context.workspace.packages.iter().enumerate() {
            let connector = branch(index + 1 == context.workspace.packages.len());
            let _ = writeln!(
                output,
                "│   {connector} {} ({})",
                safe_text(&package.value.name),
                safe_text(package.value.path.as_str())
            );
        }
    }
    let _ = writeln!(output, "├── languages");
    if context.languages.is_empty() {
        let _ = writeln!(output, "│   └── none detected");
    } else {
        for (index, language) in context.languages.iter().enumerate() {
            let connector = branch(index + 1 == context.languages.len());
            let _ = writeln!(
                output,
                "│   {connector} {} ({} files)",
                safe_text(&language.value.id),
                language.value.file_count
            );
        }
    }
    let _ = writeln!(output, "└── entry points");
    if context.entry_points.is_empty() {
        let _ = writeln!(output, "    └── none detected");
    } else {
        for (index, entry) in context.entry_points.iter().enumerate() {
            let connector = branch(index + 1 == context.entry_points.len());
            let _ = writeln!(
                output,
                "    {connector} {} [{}]",
                safe_text(entry.value.path.as_str()),
                entry_point_kind(entry.value.kind)
            );
        }
    }
    output
}

fn entry_point_kind(kind: crate::EntryPointKind) -> &'static str {
    match kind {
        crate::EntryPointKind::Binary => "binary",
        crate::EntryPointKind::Library => "library",
        crate::EntryPointKind::Application => "application",
        crate::EntryPointKind::Script => "script",
    }
}

fn repository_state(context: &ProjectContext) -> &'static str {
    match context.repository.state.value {
        crate::RepositoryState::Git => {
            if context
                .repository
                .clean
                .as_ref()
                .is_some_and(|clean| clean.value)
            {
                "Git, clean"
            } else if context.repository.clean.is_some() {
                "Git, modified"
            } else {
                "Git"
            }
        }
        crate::RepositoryState::NotRepository => "not a Git repository",
        crate::RepositoryState::GitUnavailable => "Git unavailable",
        crate::RepositoryState::Partial => "Git information partial",
    }
}

fn section_values(output: &mut String, heading: &str, values: impl IntoIterator<Item = String>) {
    let _ = writeln!(output, "\n{heading}:");
    let mut any = false;
    for value in values {
        any = true;
        let _ = writeln!(output, "  - {}", safe_text(&value));
    }
    if !any {
        let _ = writeln!(output, "  unavailable");
    }
}

fn display_command(command: &CommandSpec) -> String {
    let mut values = Vec::with_capacity(command.arguments.len() + 1);
    values.push(shell_quote(&command.executable));
    values.extend(
        command
            .arguments
            .iter()
            .map(|argument| shell_quote(argument)),
    );
    format!("{} (from {})", values.join(" "), command.working_directory)
}

fn shell_quote(value: &str) -> String {
    let value = safe_text(value);
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./:=@".contains(character))
    {
        return value;
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn safe_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_control() {
            output.extend(character.escape_default());
        } else {
            output.push(character);
        }
    }
    output
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn severity_name(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Info => "info",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Error => "error",
    }
}

fn branch(last: bool) -> &'static str {
    if last {
        "└──"
    } else {
        "├──"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LicenseSummary, ProjectIdentity, ProjectPath, ProjectSize, RepositoryContext, ScanReport,
        ToolingSummary, WorkspaceSummary, PROJECT_CONTEXT_SCHEMA_VERSION,
    };
    use std::time::Duration;

    fn report() -> ScanReport {
        ScanReport {
            schema_version: PROJECT_CONTEXT_SCHEMA_VERSION,
            context: ProjectContext {
                identity: ProjectIdentity {
                    root: ProjectPath("/tmp/project with spaces".to_string()),
                    name: "project".to_string(),
                    repository_root: None,
                },
                repository: RepositoryContext::default(),
                languages: Vec::new(),
                workspace: WorkspaceSummary::default(),
                tooling: ToolingSummary::default(),
                dependencies: Vec::new(),
                documentation: Vec::new(),
                ci: Vec::new(),
                configuration: Vec::new(),
                entry_points: Vec::new(),
                development_commands: Vec::new(),
                validation_commands: Vec::new(),
                size: crate::Detected {
                    value: ProjectSize {
                        files: 0,
                        bytes: 0,
                        source_files: 0,
                        test_files: 0,
                        documentation_files: 0,
                        configuration_files: 0,
                        truncated: false,
                    },
                    confidence: crate::Confidence::Low,
                    evidence: Vec::new(),
                },
                license: LicenseSummary::default(),
            },
            scanners: Vec::new(),
            diagnostics: Vec::new(),
            insights: Vec::new(),
            duration: Duration::from_secs(99),
        }
    }

    #[test]
    fn json_excludes_runtime_duration_and_private_facts() {
        let json = render_json(&report()).expect("JSON");
        assert!(!json.contains("duration"));
        assert!(!json.contains("FactGraph"));
        assert!(json.contains("\"schema_version\": 1"));
    }

    #[test]
    fn json_top_level_schema_is_explicit_and_stable() {
        let json = serde_json::to_value(report()).expect("JSON value");
        let keys = json
            .as_object()
            .expect("report object")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            [
                "context",
                "diagnostics",
                "insights",
                "scanners",
                "schema_version"
            ]
        );
        assert!(json.get("duration").is_none());
        let size = json
            .pointer("/context/size")
            .and_then(serde_json::Value::as_object)
            .expect("detected size");
        assert!(size.contains_key("value"));
        assert!(size.contains_key("confidence"));
        assert!(size.contains_key("evidence"));
    }

    #[test]
    fn schema_round_trip_restores_runtime_duration_to_zero() {
        let serialized = serde_json::to_string(&report()).expect("serialized report");
        let restored: ScanReport = serde_json::from_str(&serialized).expect("deserialized report");
        assert_eq!(restored.duration, Duration::ZERO);
        assert_eq!(restored.schema_version, PROJECT_CONTEXT_SCHEMA_VERSION);
    }

    #[test]
    fn renderers_are_deterministic() {
        let report = report();
        assert_eq!(render_text(&report), render_text(&report));
        assert_eq!(render_tree(&report), render_tree(&report));
        assert_eq!(
            render_json(&report).expect("JSON"),
            render_json(&report).expect("JSON")
        );
    }

    #[test]
    fn command_arguments_are_visibly_separated() {
        let command = CommandSpec {
            executable: "tool path".to_string(),
            arguments: vec!["arg with spaces".to_string(), "--flag".to_string()],
            working_directory: ProjectPath(".".to_string()),
            purpose: crate::CommandPurpose::Validate,
        };
        assert_eq!(
            display_command(&command),
            "'tool path' 'arg with spaces' --flag (from .)"
        );
    }

    #[test]
    fn terminal_rendering_escapes_control_sequences() {
        assert_eq!(safe_text("safe\u{1b}[31m\nnext"), "safe\\u{1b}[31m\\nnext");
        let command = CommandSpec {
            executable: "tool\nname".to_string(),
            arguments: vec!["\u{1b}[2J".to_string()],
            working_directory: ProjectPath(".".to_string()),
            purpose: crate::CommandPurpose::Validate,
        };
        let rendered = display_command(&command);
        assert!(!rendered.contains('\u{1b}'));
        assert!(!rendered.contains('\n'));
    }
}
