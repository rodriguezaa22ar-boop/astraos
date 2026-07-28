use crate::{
    context, ProjectCommands, ProjectCommandsArgs, ProjectInspectArgs, ProjectRunArgs,
    ProjectUnderstandArgs,
};
use astra_actions::{
    resolve_actions, select_action, ActionId, ActionPolicy, DryRunReport, ExecutionPlan,
    PolicyDecision, PolicyRejectionReason, ProjectAction, ProjectActionReport, ProjectReference,
};
use astra_config::{load_if_present, Config};
use astra_execution::{
    controlled_execution_capability, ControlledExecutionCapability, ExecutionEngine,
    ExecutionError, ExecutionOutputMode, ExecutionResult, VerificationVerdict,
};
use astra_intelligence::{
    render_text as render_intelligence_text, ActionInput, DeterministicProjectIntelligenceAnalyzer,
    ExecutionCapabilityInput, IntelligenceConfidence, IntelligenceEvidenceRef,
    KnowledgeCategoryInput, KnowledgeClaimInput, ProjectContextInput, ProjectIdentityInput,
    ProjectIntelligenceAnalyzer, ProjectIntelligenceInput, ProjectKnowledgeInput, RepositoryInput,
    VerificationValidity, WorkspacePackageInput,
};
use astra_knowledge::{KnowledgeCategory, KnowledgeNamespace, KnowledgeStore, Validity};
use astra_workspaces::{list_workspaces, workspace_path};
use std::{fmt, fs, path::PathBuf};

#[derive(Debug)]
pub(crate) enum ProjectError {
    Configuration(String),
    UnknownProject(String),
    MissingPath(PathBuf),
    NotDirectory(PathBuf),
    PathInspection(String),
    Context(String),
    Intelligence(String),
    Knowledge(String),
    Serialization(String),
    Output(String),
    DryRunOnly(ActionId),
    InvalidAction(String),
    ActionUnavailable { project: String, action: ActionId },
    PolicyRejected(PolicyRejectionReason),
    Execution(ExecutionError),
    ChildFailed { exit_code: i32 },
    SourceStateChanged,
    CommandFailedAndSourceStateChanged { exit_code: Option<i32> },
    Interrupted,
}

impl fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => {
                write!(formatter, "could not load configuration: {message}")
            }
            Self::UnknownProject(name) => write!(formatter, "unknown project: {name}"),
            Self::MissingPath(path) => {
                write!(formatter, "project path does not exist: {}", path.display())
            }
            Self::NotDirectory(path) => write!(
                formatter,
                "project path is not a directory: {}",
                path.display()
            ),
            Self::PathInspection(message) => {
                write!(formatter, "could not inspect project path: {message}")
            }
            Self::Context(message) => formatter.write_str(message),
            Self::Intelligence(message) => write!(
                formatter,
                "could not build project understanding: {message}"
            ),
            Self::Knowledge(message) => write!(
                formatter,
                "could not persist verification knowledge: {message}"
            ),
            Self::Serialization(message) => {
                write!(formatter, "could not serialize project actions: {message}")
            }
            Self::Output(message) => formatter.write_str(message),
            Self::DryRunOnly(action) => {
                write!(formatter, "action is currently dry-run only: {action}")
            }
            Self::InvalidAction(action) => write!(formatter, "unsupported action: {action}"),
            Self::ActionUnavailable { project, action } => {
                write!(
                    formatter,
                    "project does not expose action: {action} for {project}"
                )
            }
            Self::PolicyRejected(reason) => {
                write!(formatter, "action rejected by policy: {reason}")
            }
            Self::Execution(error) => formatter.write_str(&error.to_string()),
            Self::ChildFailed { exit_code } => {
                write!(formatter, "cargo check failed with exit code {exit_code}")
            }
            Self::SourceStateChanged => {
                formatter.write_str("source state changed during execution; check is not verified")
            }
            Self::CommandFailedAndSourceStateChanged { exit_code } => write!(
                formatter,
                "cargo check failed{} and source state changed during execution",
                exit_code.map_or_else(String::new, |code| format!(" with exit code {code}"))
            ),
            Self::Interrupted => formatter.write_str("cargo check was interrupted"),
        }
    }
}

impl ProjectError {
    pub(crate) fn exit_code(&self) -> u8 {
        match self {
            Self::ChildFailed { exit_code } => u8::try_from(*exit_code)
                .ok()
                .filter(|code| *code != 0)
                .unwrap_or(1),
            Self::CommandFailedAndSourceStateChanged {
                exit_code: Some(exit_code),
            } => u8::try_from(*exit_code)
                .ok()
                .filter(|code| *code != 0)
                .unwrap_or(1),
            _ => 1,
        }
    }
}

pub(crate) fn run(command: ProjectCommands) -> Result<(), ProjectError> {
    match command {
        ProjectCommands::List => list(),
        ProjectCommands::Inspect(arguments) => inspect(arguments),
        ProjectCommands::Commands(arguments) => commands(arguments),
        ProjectCommands::Run(arguments) => run_action(arguments),
        ProjectCommands::Understand(arguments) => understand(arguments),
        ProjectCommands::Create { kind, name } => {
            crate::create_project(&kind, &name).map_err(ProjectError::Output)
        }
    }
}

fn run_action(arguments: ProjectRunArgs) -> Result<(), ProjectError> {
    let action_id = ActionId::parse(&arguments.action)
        .ok_or_else(|| ProjectError::InvalidAction(arguments.action.clone()))?;
    if !arguments.dry_run {
        return if action_id == ActionId::Check {
            run_check(arguments)
        } else {
            Err(ProjectError::DryRunOnly(action_id))
        };
    }
    run_dry_run(arguments, action_id)
}

fn run_dry_run(arguments: ProjectRunArgs, action_id: ActionId) -> Result<(), ProjectError> {
    let config = load_project_config()?;
    let root = resolve_registered_project(&config, &arguments.name)?;
    let report = context::analyze_without_processes(&root).map_err(ProjectError::Context)?;
    let actions = absolutize_actions(resolve_actions(&report.context.validation_commands), &root);
    let action =
        select_action(&actions, action_id).ok_or_else(|| ProjectError::ActionUnavailable {
            project: arguments.name.clone(),
            action: action_id,
        })?;
    let project = ProjectReference {
        name: arguments.name.clone(),
        root,
    };
    let evaluation = ActionPolicy.evaluate(&project, &action);
    let plan = ExecutionPlan::new(project, evaluation.action, evaluation.decision);
    let dry_run = DryRunReport::new(plan);

    if arguments.json {
        let rendered = serde_json::to_string_pretty(&dry_run)
            .map_err(|error| ProjectError::Serialization(error.to_string()))?;
        println!("{rendered}");
    } else {
        print_dry_run(&dry_run);
    }

    if let PolicyDecision::Rejected { reason } = dry_run.plan.policy {
        return Err(ProjectError::PolicyRejected(reason));
    }

    Ok(())
}

fn run_check(arguments: ProjectRunArgs) -> Result<(), ProjectError> {
    let config = load_project_config()?;
    let root = resolve_registered_project(&config, &arguments.name)?;
    let report = context::analyze_without_processes(&root).map_err(ProjectError::Context)?;
    let actions = absolutize_actions(resolve_actions(&report.context.validation_commands), &root);
    let action = select_action(&actions, ActionId::Check).ok_or_else(|| {
        ProjectError::ActionUnavailable {
            project: arguments.name.clone(),
            action: ActionId::Check,
        }
    })?;
    let project_name = arguments.name.clone();
    let project = ProjectReference {
        name: project_name,
        root,
    };
    let engine = ExecutionEngine::new();
    let plan = engine
        .plan(&project, &action)
        .map_err(ProjectError::Execution)?;

    if !arguments.json {
        print_execution_plan(&plan);
        println!("\nExecuting approved plan...\n");
    }

    let result = engine
        .execute(
            &plan,
            if arguments.json {
                ExecutionOutputMode::Json
            } else {
                ExecutionOutputMode::Human
            },
        )
        .map_err(ProjectError::Execution)?;

    crate::knowledge::record_verification(&arguments.name, &result)
        .map_err(ProjectError::Knowledge)?;

    if arguments.json {
        let rendered = serde_json::to_string_pretty(&result)
            .map_err(|error| ProjectError::Serialization(error.to_string()))?;
        println!("{rendered}");
    } else {
        print_execution_result(&result);
    }

    match result.execution.verdict {
        VerificationVerdict::VerifiedCheck => Ok(()),
        VerificationVerdict::CommandFailed => Err(ProjectError::ChildFailed {
            exit_code: result.execution.exit_code.unwrap_or(1),
        }),
        VerificationVerdict::SourceStateChanged => Err(ProjectError::SourceStateChanged),
        VerificationVerdict::CommandFailedAndSourceStateChanged => {
            Err(ProjectError::CommandFailedAndSourceStateChanged {
                exit_code: result.execution.exit_code,
            })
        }
        VerificationVerdict::Interrupted => Err(ProjectError::Interrupted),
    }
}

fn load_project_config() -> Result<Config, ProjectError> {
    load_if_present()
        .map_err(|error| ProjectError::Configuration(error.to_string()))
        .map(|config| config.unwrap_or_default())
}

fn list() -> Result<(), ProjectError> {
    let config = load_project_config()?;
    let mut projects = list_workspaces(&config);
    projects.sort();

    if projects.is_empty() {
        println!("No registered projects.");
        return Ok(());
    }

    let name_width = projects
        .iter()
        .map(|(name, _)| name.chars().count())
        .max()
        .unwrap_or(0)
        .max("PROJECT".len());
    println!("{:<name_width$}  PATH", "PROJECT");
    for (name, path) in projects {
        println!("{name:<name_width$}  {path}");
    }
    Ok(())
}

fn inspect(arguments: ProjectInspectArgs) -> Result<(), ProjectError> {
    let config = load_project_config()?;
    let path = resolve_registered_project(&config, &arguments.name)?;
    let format = if arguments.json {
        context::OutputFormat::Json
    } else {
        context::OutputFormat::Text
    };
    context::inspect(&path, format).map_err(ProjectError::Context)
}

fn commands(arguments: ProjectCommandsArgs) -> Result<(), ProjectError> {
    let config = load_project_config()?;
    let path = resolve_registered_project(&config, &arguments.name)?;
    let report = context::analyze(&path).map_err(ProjectError::Context)?;
    let root = PathBuf::from(report.context.identity.root.as_str());
    let actions = absolutize_actions(resolve_actions(&report.context.validation_commands), &root);
    let action_report = ProjectActionReport::new(
        ProjectReference {
            name: arguments.name.clone(),
            root,
        },
        actions,
    );

    if arguments.json {
        let rendered = serde_json::to_string_pretty(&action_report)
            .map_err(|error| ProjectError::Serialization(error.to_string()))?;
        println!("{rendered}");
    } else {
        print_actions(&arguments.name, &action_report.actions);
    }

    Ok(())
}

fn understand(arguments: ProjectUnderstandArgs) -> Result<(), ProjectError> {
    let config = load_project_config()?;
    let root = resolve_registered_project(&config, &arguments.name)?;
    // Understanding is intentionally based on the no-process context mode. A
    // separate bounded Git-state capture is used only to project the validity
    // of an already persisted verification; no project action is launched.
    let report = context::analyze_without_processes(&root).map_err(ProjectError::Context)?;
    let actions = absolutize_actions(resolve_actions(&report.context.validation_commands), &root);
    let project = ProjectReference {
        name: arguments.name.clone(),
        root: root.clone(),
    };
    let current_state = current_state_fingerprint(&project, &actions);
    let input = intelligence_input(
        &arguments.name,
        &report.context,
        &project,
        actions,
        current_state,
    )
    .map_err(ProjectError::Intelligence)?;
    let intelligence = DeterministicProjectIntelligenceAnalyzer
        .analyze(&input)
        .map_err(|error| ProjectError::Intelligence(error.to_string()))?;

    if arguments.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&intelligence)
                .map_err(|error| ProjectError::Serialization(error.to_string()))?
        );
    } else {
        print!("{}", render_intelligence_text(&intelligence));
    }
    Ok(())
}

fn current_state_fingerprint(
    project: &ProjectReference,
    actions: &[ProjectAction],
) -> Option<String> {
    let action = select_action(actions, ActionId::Check)?;
    if !matches!(
        controlled_execution_capability(action.id),
        ControlledExecutionCapability::Allowed
    ) {
        return None;
    }
    ExecutionEngine::new()
        .plan(project, &action)
        .ok()
        .map(|plan| plan.source_state.combined_fingerprint.to_string())
}

fn intelligence_input(
    project_name: &str,
    context: &astra_context::ProjectContext,
    project: &ProjectReference,
    actions: Vec<ProjectAction>,
    current_state: Option<String>,
) -> Result<ProjectIntelligenceInput, String> {
    let action_inputs = actions
        .iter()
        .map(|action| ActionInput {
            id: action.id.as_str().to_string(),
            confidence: intelligence_confidence(action.confidence),
            evidence: vec![IntelligenceEvidenceRef::Action {
                action_id: action.id.as_str().to_string(),
            }],
        })
        .collect::<Vec<_>>();
    let mut discovered_actions = Vec::new();
    let mut controlled_actions = Vec::new();
    let mut dry_run_only_actions = Vec::new();
    let mut unsupported_actions = Vec::new();
    for action in &actions {
        let id = action.id.as_str().to_string();
        discovered_actions.push(id.clone());
        let policy = ActionPolicy.evaluate(project, action);
        if !policy.decision.is_allowed() {
            unsupported_actions.push(id);
        } else if matches!(
            controlled_execution_capability(action.id),
            ControlledExecutionCapability::Allowed
        ) {
            controlled_actions.push(id);
        } else {
            dry_run_only_actions.push(id);
        }
    }
    let knowledge = projected_knowledge(project_name, current_state.as_deref())?;
    let project_type = project_type(context);
    let repository_state = current_state
        .as_ref()
        .map(|_| "git".to_string())
        .unwrap_or_else(|| repository_state(context.repository.state.value).to_string());
    Ok(ProjectIntelligenceInput {
        project: ProjectIdentityInput {
            name: project_name.to_string(),
            project_type,
        },
        context: ProjectContextInput {
            workspace_kinds: context
                .workspace
                .kinds
                .iter()
                .map(|kind| kind.value.clone())
                .collect(),
            packages: context
                .workspace
                .packages
                .iter()
                .map(|package| {
                    Ok::<WorkspacePackageInput, String>(WorkspacePackageInput {
                        name: package.value.name.clone(),
                        ecosystem: package.value.ecosystem.clone(),
                        relative_path: safe_relative_path(package.value.path.as_str())?,
                        confidence: intelligence_confidence(package.confidence),
                        evidence: vec![IntelligenceEvidenceRef::ContextPackage {
                            package: package.value.name.clone(),
                        }],
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            languages: context
                .languages
                .iter()
                .map(|language| language.value.id.clone())
                .collect(),
            build_systems: context
                .tooling
                .build_systems
                .iter()
                .map(|tool| tool.value.id.clone())
                .collect(),
            continuous_integration: context
                .ci
                .iter()
                .map(|ci| ci.value.provider.clone())
                .collect(),
            repository: RepositoryInput {
                state: Some(repository_state),
                clean: context.repository.clean.as_ref().map(|clean| clean.value),
            },
        },
        actions: action_inputs,
        execution_capabilities: ExecutionCapabilityInput {
            discovered_actions,
            controlled_actions,
            dry_run_only_actions,
            unsupported_actions,
        },
        knowledge,
    })
}

fn projected_knowledge(
    project_name: &str,
    current_state: Option<&str>,
) -> Result<ProjectKnowledgeInput, String> {
    let store = KnowledgeStore::open_default().map_err(|error| error.to_string())?;
    let claims = store
        .query_claims(&KnowledgeNamespace::project(project_name), None)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|claim| {
            let observed = claim.observed_state(current_state);
            KnowledgeClaimInput {
                id: observed.id.to_string(),
                category: knowledge_category(observed.category),
                predicate: observed.predicate,
                confidence: knowledge_confidence(observed.confidence),
                validity: knowledge_validity(observed.validity),
                created_at: observed.created_at,
                verification_action: approved_verification_field(&observed.value, "action"),
                verification_verdict: approved_verification_field(&observed.value, "verdict"),
            }
        })
        .collect();
    Ok(ProjectKnowledgeInput { claims })
}

fn approved_verification_field(value: &serde_json::Value, field: &str) -> Option<String> {
    value
        .as_object()?
        .get(field)?
        .as_str()
        .filter(|value| !value.chars().any(char::is_control))
        .map(ToString::to_string)
}

fn safe_relative_path(value: &str) -> Result<String, String> {
    let path = std::path::Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("context package path is not project-relative".to_string());
    }
    Ok(value.replace('\\', "/"))
}

fn project_type(context: &astra_context::ProjectContext) -> Option<String> {
    let cargo = context
        .tooling
        .build_systems
        .iter()
        .any(|tool| tool.value.id == "cargo");
    let workspace = !context.workspace.kinds.is_empty();
    match (cargo, workspace) {
        (true, true) => Some("Rust Cargo workspace".to_string()),
        (true, false) => Some("Rust Cargo project".to_string()),
        _ => None,
    }
}

fn intelligence_confidence(value: astra_context::Confidence) -> IntelligenceConfidence {
    match value {
        astra_context::Confidence::Certain => IntelligenceConfidence::Certain,
        astra_context::Confidence::High => IntelligenceConfidence::High,
        astra_context::Confidence::Medium => IntelligenceConfidence::Medium,
        astra_context::Confidence::Low => IntelligenceConfidence::Low,
    }
}

fn knowledge_confidence(value: astra_knowledge::Confidence) -> IntelligenceConfidence {
    match value {
        astra_knowledge::Confidence::Certain => IntelligenceConfidence::Certain,
        astra_knowledge::Confidence::High => IntelligenceConfidence::High,
        astra_knowledge::Confidence::Medium => IntelligenceConfidence::Medium,
        astra_knowledge::Confidence::Low => IntelligenceConfidence::Low,
        astra_knowledge::Confidence::Unknown => IntelligenceConfidence::Unknown,
    }
}

fn knowledge_category(value: KnowledgeCategory) -> KnowledgeCategoryInput {
    match value {
        KnowledgeCategory::Fact => KnowledgeCategoryInput::Fact,
        KnowledgeCategory::Decision => KnowledgeCategoryInput::Decision,
        KnowledgeCategory::Verification => KnowledgeCategoryInput::Verification,
        KnowledgeCategory::Goal => KnowledgeCategoryInput::Goal,
    }
}

fn knowledge_validity(value: Validity) -> VerificationValidity {
    match value {
        Validity::Current => VerificationValidity::Current,
        Validity::Stale => VerificationValidity::Stale,
        Validity::Invalidated => VerificationValidity::Invalidated,
        Validity::Unknown => VerificationValidity::Unknown,
    }
}

fn repository_state(value: astra_context::RepositoryState) -> &'static str {
    match value {
        astra_context::RepositoryState::Git => "git",
        astra_context::RepositoryState::NotRepository => "not_repository",
        astra_context::RepositoryState::GitUnavailable => "git_unavailable",
        astra_context::RepositoryState::Partial => "partial",
    }
}

fn resolve_registered_project(config: &Config, name: &str) -> Result<PathBuf, ProjectError> {
    let path = workspace_path(config, name)
        .ok_or_else(|| ProjectError::UnknownProject(name.to_string()))?;
    let metadata = fs::metadata(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ProjectError::MissingPath(path.clone())
        } else {
            ProjectError::PathInspection(format!("{}: {error}", path.display()))
        }
    })?;
    if !metadata.is_dir() {
        return Err(ProjectError::NotDirectory(path));
    }
    path.canonicalize()
        .map_err(|error| ProjectError::PathInspection(format!("{}: {error}", path.display())))
}

fn absolutize_actions(
    mut actions: Vec<ProjectAction>,
    root: &std::path::Path,
) -> Vec<ProjectAction> {
    for action in &mut actions {
        if action.command.working_directory.is_relative() {
            action.command.working_directory =
                if action.command.working_directory == std::path::Path::new(".") {
                    root.to_path_buf()
                } else {
                    root.join(&action.command.working_directory)
                };
        }
    }
    actions
}

fn print_actions(project_name: &str, actions: &[ProjectAction]) {
    println!("Available actions for {project_name}");
    println!();
    if actions.is_empty() {
        println!("No supported actions detected.");
        return;
    }

    let action_width = actions
        .iter()
        .map(|action| action.id.as_str().len())
        .max()
        .unwrap_or(0)
        .max("ACTION".len());
    println!("{:<action_width$}  COMMAND", "ACTION");
    for action in actions {
        println!(
            "{:<action_width$}  {}",
            action.id.as_str(),
            display_command(&action.command.executable, &action.command.arguments)
        );
    }
}

fn print_dry_run(report: &DryRunReport) {
    let action = &report.plan.action;
    println!("Project: {}", report.plan.project.name);
    println!("Action: {}", action.id.as_str());
    println!(
        "Working directory: {}",
        action.command.working_directory.display()
    );
    println!("Executable: {}", shell_quote(&action.command.executable));
    println!("Arguments:");
    for argument in &action.command.arguments {
        println!("  - {}", shell_quote(argument));
    }
    match &report.plan.policy {
        PolicyDecision::Allowed => println!("Policy: allowed"),
        PolicyDecision::Rejected { reason } => println!("Policy: rejected ({reason})"),
    }
    println!();
    if report.plan.policy.is_allowed() {
        println!("Dry run complete. No process was started.");
    } else {
        println!("Dry run rejected. No process was started.");
    }
}

fn print_execution_plan(plan: &astra_execution::AuthorizedExecutionPlan) {
    let action = &plan.action;
    println!("Project: {}", plan.project.name);
    println!("Action: {}", action.id.as_str());
    println!(
        "Working directory: {}",
        action.command.working_directory.display()
    );
    println!("Executable: {}", shell_quote(&action.command.executable));
    println!("Arguments:");
    for argument in &action.command.arguments {
        println!("  - {}", shell_quote(argument));
    }
    println!("Policy: allowed");
    println!(
        "State fingerprint: {}",
        plan.source_state.combined_fingerprint
    );
    println!("Action fingerprint: {}", plan.action_fingerprint);
    println!("Plan fingerprint: {}", plan.plan_fingerprint);
}

fn print_execution_result(result: &ExecutionResult) {
    println!("Check result");
    println!(
        "  Process started: {}",
        if result.execution.process_started {
            "yes"
        } else {
            "no"
        }
    );
    println!(
        "  Exit code: {}",
        result
            .execution
            .exit_code
            .map_or_else(|| "none".to_string(), |code| code.to_string())
    );
    println!("  Duration: {} ms", result.execution.duration_ms);
    println!(
        "  Source state changed: {}",
        if result.state.changed { "yes" } else { "no" }
    );
    println!(
        "  Verdict: {}",
        match result.execution.verdict {
            VerificationVerdict::VerifiedCheck => "verified_check",
            VerificationVerdict::CommandFailed => "command_failed",
            VerificationVerdict::SourceStateChanged => "source_state_changed",
            VerificationVerdict::CommandFailedAndSourceStateChanged => {
                "command_failed_and_source_state_changed"
            }
            VerificationVerdict::Interrupted => "interrupted",
        }
    );
}

fn display_command(executable: &str, arguments: &[String]) -> String {
    std::iter::once(executable)
        .chain(arguments.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    let value = value
        .chars()
        .flat_map(|character| {
            if character.is_control() {
                character.escape_default().collect::<Vec<_>>()
            } else {
                vec![character]
            }
        })
        .collect::<String>();
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./:=@".contains(character))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}
