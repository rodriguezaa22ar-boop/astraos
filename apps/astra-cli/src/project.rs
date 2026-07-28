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
    authority_targets, render_resolved_text, render_text as render_intelligence_text, ActionInput,
    DeterministicProjectIntelligenceAnalyzer, ExecutionCapabilityInput, IntelligenceConfidence,
    IntelligenceEvidenceRef, KnowledgeCategoryInput, KnowledgeClaimInput,
    OperatorAuthorityResolver, ProjectContextInput, ProjectIdentityInput, ProjectIntelligence,
    ProjectIntelligenceAnalyzer, ProjectIntelligenceInput, ProjectKnowledgeInput, RepositoryInput,
    ResolutionStatus, VerificationValidity, WorkspacePackageInput,
};
use astra_knowledge::{
    AcceptancePayload, AnnotationPayload, AnnotationScope, CorrectionPayload, DisputePayload,
    KnowledgeCategory, KnowledgeNamespace, KnowledgeStore, NewOperatorResponse, OperatorConfidence,
    OperatorHistoryOperation, OperatorIdentity, OperatorIntent, OperatorResponse,
    OperatorResponseHistoryEntry, OperatorResponseId, OperatorResponsePayload,
    OperatorTargetBinding, RejectionPayload, Validity, OPERATOR_RESPONSE_SCHEMA_VERSION,
};
use astra_workspaces::{list_workspaces, workspace_path};
use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;
use std::{fmt, fs, path::PathBuf};

#[derive(Debug, Subcommand)]
pub(crate) enum ProjectResponsesCommands {
    /// List current operator-response records.
    List(ProjectResponsesListArgs),
    /// Show one operator response.
    Show(ProjectResponseShowArgs),
    /// Show committed response transaction history.
    History(ProjectResponsesListArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProjectInsightCommands {
    /// Accept one derived insight immediately.
    Accept(ProjectInsightAcceptArgs),
    /// Create a rejection draft.
    Reject(ProjectInsightRejectArgs),
    /// Create a correction draft.
    Correct(ProjectInsightCorrectArgs),
    /// Create a dispute draft.
    Dispute(ProjectInsightDisputeArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProjectResponseCommands {
    /// Edit the typed payload of a draft response.
    Edit(ProjectResponseEditArgs),
    /// Preview a response and its current target binding.
    Preview(ProjectResponseShowArgs),
    /// Delete a draft response while preserving transaction history.
    Delete(ProjectResponseMutationArgs),
    /// Activate a draft after revalidating its target.
    Activate(ProjectResponseActivateArgs),
    /// Retire an active response.
    Retire(ProjectResponseMutationArgs),
    /// Withdraw an active response.
    Withdraw(ProjectResponseMutationArgs),
    /// Reaffirm a response against the current target.
    Reaffirm(ProjectResponseMutationArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ProjectResponsesListArgs {
    #[arg(value_name = "PROJECT")]
    project: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ProjectResponseShowArgs {
    #[arg(value_name = "PROJECT")]
    project: String,
    #[arg(value_name = "RESPONSE")]
    response: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ProjectResponseMutationArgs {
    #[arg(value_name = "PROJECT")]
    project: String,
    #[arg(value_name = "RESPONSE")]
    response: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ProjectResponseActivateArgs {
    #[arg(value_name = "PROJECT")]
    project: String,
    #[arg(value_name = "RESPONSE")]
    response: String,
    #[arg(long, value_name = "RESPONSE_ID")]
    supersedes: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ProjectAnnotateArgs {
    #[arg(value_name = "PROJECT")]
    project: String,
    #[arg(value_name = "TARGET")]
    target: String,
    #[arg(long, value_name = "TEXT")]
    statement: String,
    #[arg(long, value_enum)]
    intent: OperatorIntentArg,
    #[arg(long)]
    state_bound: bool,
    #[arg(long, value_enum)]
    confidence: Option<OperatorConfidenceArg>,
    #[command(flatten)]
    operator: OperatorArgs,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ProjectInsightAcceptArgs {
    #[arg(value_name = "PROJECT")]
    project: String,
    #[arg(value_name = "INSIGHT")]
    insight: String,
    #[arg(long, value_name = "TEXT")]
    reason: Option<String>,
    #[arg(long, value_enum)]
    confidence: Option<OperatorConfidenceArg>,
    #[arg(long, value_name = "RESPONSE_ID")]
    supersedes: Option<String>,
    #[command(flatten)]
    operator: OperatorArgs,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ProjectInsightRejectArgs {
    #[arg(value_name = "PROJECT")]
    project: String,
    #[arg(value_name = "INSIGHT")]
    insight: String,
    #[arg(long, value_name = "TEXT")]
    reason: String,
    #[arg(long, value_enum)]
    intent: Option<OperatorIntentArg>,
    #[arg(long, value_enum)]
    confidence: Option<OperatorConfidenceArg>,
    #[arg(long, value_name = "RESPONSE_ID")]
    supersedes: Option<String>,
    #[command(flatten)]
    operator: OperatorArgs,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ProjectInsightCorrectArgs {
    #[arg(value_name = "PROJECT")]
    project: String,
    #[arg(value_name = "INSIGHT")]
    insight: String,
    #[arg(long, value_name = "TEXT")]
    statement: String,
    #[arg(long, value_name = "TEXT")]
    reason: Option<String>,
    #[arg(long, value_enum)]
    intent: OperatorIntentArg,
    #[arg(long, value_enum)]
    confidence: Option<OperatorConfidenceArg>,
    #[arg(long, value_name = "RESPONSE_ID")]
    supersedes: Option<String>,
    #[command(flatten)]
    operator: OperatorArgs,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ProjectInsightDisputeArgs {
    #[arg(value_name = "PROJECT")]
    project: String,
    #[arg(value_name = "INSIGHT")]
    insight: String,
    #[arg(long, value_name = "TEXT")]
    reason: String,
    #[arg(long, value_enum)]
    intent: Option<OperatorIntentArg>,
    #[arg(long, value_enum)]
    confidence: Option<OperatorConfidenceArg>,
    #[arg(long, value_name = "RESPONSE_ID")]
    supersedes: Option<String>,
    #[command(flatten)]
    operator: OperatorArgs,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ProjectResponseEditArgs {
    #[arg(value_name = "PROJECT")]
    project: String,
    #[arg(value_name = "RESPONSE")]
    response: String,
    #[arg(long, value_name = "TEXT")]
    statement: Option<String>,
    #[arg(long, value_name = "TEXT")]
    reason: Option<String>,
    #[arg(long, value_enum)]
    intent: Option<OperatorIntentArg>,
    #[arg(long, value_enum)]
    confidence: Option<OperatorConfidenceArg>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct OperatorArgs {
    /// Stable key for a named local operator.
    #[arg(long, value_name = "KEY")]
    operator: Option<String>,
    /// Display name stored with this historical response.
    #[arg(long, value_name = "NAME", requires = "operator")]
    operator_name: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OperatorConfidenceArg {
    Certain,
    High,
    Medium,
    Tentative,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OperatorIntentArg {
    Architecture,
    Decision,
    Preference,
    TemporaryConstraint,
    Experiment,
    Context,
}

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
    Authority(String),
    TargetNotFound(String),
    TargetAmbiguous(String),
    UnresolvedAuthority,
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
            Self::Knowledge(message) => write!(formatter, "knowledge operation failed: {message}"),
            Self::Authority(message) => write!(formatter, "operator authority failed: {message}"),
            Self::TargetNotFound(selector) => {
                write!(formatter, "intelligence target not found: {selector}")
            }
            Self::TargetAmbiguous(selector) => {
                write!(formatter, "intelligence target is ambiguous: {selector}")
            }
            Self::UnresolvedAuthority => {
                formatter.write_str("project understanding has unresolved operator authority")
            }
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
        ProjectCommands::Responses { command } => responses(command),
        ProjectCommands::Insight { command } => insight(command),
        ProjectCommands::Annotate(arguments) => annotate(arguments),
        ProjectCommands::Response { command } => response(command),
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
    let intelligence = build_base_intelligence(&arguments.name)?;
    if arguments.base {
        return render_base_intelligence(&intelligence, arguments.json);
    }
    let responses = operator_store()?
        .list_operator_responses(&arguments.name)
        .map_err(|error| ProjectError::Authority(error.to_string()))?;
    let resolved = OperatorAuthorityResolver
        .resolve(&intelligence, &responses, arguments.explain)
        .map_err(|error| ProjectError::Intelligence(error.to_string()))?;
    if arguments.require_resolved && resolved.resolution_status == ResolutionStatus::Unresolved {
        return Err(ProjectError::UnresolvedAuthority);
    }
    if arguments.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&resolved)
                .map_err(|error| ProjectError::Serialization(error.to_string()))?
        );
    } else {
        print!("{}", render_resolved_text(&resolved, arguments.explain));
    }
    Ok(())
}

fn build_base_intelligence(project_name: &str) -> Result<ProjectIntelligence, ProjectError> {
    let config = load_project_config()?;
    let root = resolve_registered_project(&config, project_name)?;
    // Understanding is intentionally based on the no-process context mode. A
    // separate bounded Git-state capture is used only to project the validity
    // of an already persisted verification; no project action is launched.
    let report = context::analyze_without_processes(&root).map_err(ProjectError::Context)?;
    let actions = absolutize_actions(resolve_actions(&report.context.validation_commands), &root);
    let project = ProjectReference {
        name: project_name.to_string(),
        root: root.clone(),
    };
    let current_state = current_state_fingerprint(&project, &actions);
    let input = intelligence_input(
        project_name,
        &report.context,
        &project,
        actions,
        current_state,
    )
    .map_err(ProjectError::Intelligence)?;
    let intelligence = DeterministicProjectIntelligenceAnalyzer
        .analyze(&input)
        .map_err(|error| ProjectError::Intelligence(error.to_string()))?;
    Ok(intelligence)
}

fn render_base_intelligence(
    intelligence: &ProjectIntelligence,
    json: bool,
) -> Result<(), ProjectError> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(intelligence)
                .map_err(|error| ProjectError::Serialization(error.to_string()))?
        );
    } else {
        print!("{}", render_intelligence_text(intelligence));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct OperatorResponsesReport {
    schema_version: u32,
    project: String,
    responses: Vec<OperatorResponse>,
}

#[derive(Debug, Serialize)]
struct OperatorResponseReport {
    schema_version: u32,
    project: String,
    response: OperatorResponse,
}

#[derive(Debug, Serialize)]
struct OperatorHistoryReport {
    schema_version: u32,
    project: String,
    history: Vec<OperatorResponseHistoryEntry>,
}

#[derive(Debug, Serialize)]
struct OperatorPreviewReport {
    schema_version: u32,
    project: String,
    response: OperatorResponse,
    target_matches: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_target: Option<OperatorTargetBinding>,
}

fn responses(command: ProjectResponsesCommands) -> Result<(), ProjectError> {
    match command {
        ProjectResponsesCommands::List(arguments) => {
            let responses = projected_operator_responses(&arguments.project)?;
            if arguments.json {
                print_json(&OperatorResponsesReport {
                    schema_version: OPERATOR_RESPONSE_SCHEMA_VERSION,
                    project: arguments.project,
                    responses,
                })
            } else {
                print_response_list(&arguments.project, &responses);
                Ok(())
            }
        }
        ProjectResponsesCommands::Show(arguments) => {
            let id = response_id(&arguments.response)?;
            let response = load_projected_operator_response(&arguments.project, &id)?;
            render_response(&arguments.project, response, arguments.json)
        }
        ProjectResponsesCommands::History(arguments) => {
            ensure_registered_project(&arguments.project)?;
            let history = operator_store()?
                .operator_response_history(&arguments.project)
                .map_err(authority_error)?;
            if arguments.json {
                print_json(&OperatorHistoryReport {
                    schema_version: OPERATOR_RESPONSE_SCHEMA_VERSION,
                    project: arguments.project,
                    history,
                })
            } else {
                print_response_history(&arguments.project, &history);
                Ok(())
            }
        }
    }
}

fn insight(command: ProjectInsightCommands) -> Result<(), ProjectError> {
    match command {
        ProjectInsightCommands::Accept(arguments) => {
            let base = build_base_intelligence(&arguments.project)?;
            let target = resolve_authority_target(&base, &arguments.insight)?;
            let payload = OperatorResponsePayload::Acceptance(AcceptancePayload {
                reason: arguments.reason,
                confidence: arguments.confidence.map(operator_confidence),
            });
            create_operator_response(
                &arguments.project,
                target,
                operator_identity(arguments.operator)?,
                payload,
                optional_response_id(arguments.supersedes.as_deref())?,
                arguments.json,
            )
        }
        ProjectInsightCommands::Reject(arguments) => {
            let base = build_base_intelligence(&arguments.project)?;
            let target = resolve_authority_target(&base, &arguments.insight)?;
            let payload = OperatorResponsePayload::Rejection(RejectionPayload {
                reason: arguments.reason,
                intent: arguments.intent.map(operator_intent),
                confidence: arguments.confidence.map(operator_confidence),
            });
            create_operator_response(
                &arguments.project,
                target,
                operator_identity(arguments.operator)?,
                payload,
                optional_response_id(arguments.supersedes.as_deref())?,
                arguments.json,
            )
        }
        ProjectInsightCommands::Correct(arguments) => {
            let base = build_base_intelligence(&arguments.project)?;
            let target = resolve_authority_target(&base, &arguments.insight)?;
            let payload = OperatorResponsePayload::Correction(CorrectionPayload {
                replacement_statement: arguments.statement,
                reason: arguments.reason,
                intent: operator_intent(arguments.intent),
                confidence: arguments.confidence.map(operator_confidence),
            });
            create_operator_response(
                &arguments.project,
                target,
                operator_identity(arguments.operator)?,
                payload,
                optional_response_id(arguments.supersedes.as_deref())?,
                arguments.json,
            )
        }
        ProjectInsightCommands::Dispute(arguments) => {
            let base = build_base_intelligence(&arguments.project)?;
            let target = resolve_authority_target(&base, &arguments.insight)?;
            let payload = OperatorResponsePayload::Dispute(DisputePayload {
                reason: arguments.reason,
                intent: arguments.intent.map(operator_intent),
                confidence: arguments.confidence.map(operator_confidence),
            });
            create_operator_response(
                &arguments.project,
                target,
                operator_identity(arguments.operator)?,
                payload,
                optional_response_id(arguments.supersedes.as_deref())?,
                arguments.json,
            )
        }
    }
}

fn annotate(arguments: ProjectAnnotateArgs) -> Result<(), ProjectError> {
    let base = build_base_intelligence(&arguments.project)?;
    let target = resolve_authority_target(&base, &arguments.target)?;
    let payload = OperatorResponsePayload::Annotation(AnnotationPayload {
        statement: arguments.statement,
        intent: operator_intent(arguments.intent),
        scope: if arguments.state_bound {
            AnnotationScope::StateBound
        } else {
            AnnotationScope::Persistent
        },
        confidence: arguments.confidence.map(operator_confidence),
    });
    create_operator_response(
        &arguments.project,
        target,
        operator_identity(arguments.operator)?,
        payload,
        None,
        arguments.json,
    )
}

fn response(command: ProjectResponseCommands) -> Result<(), ProjectError> {
    match command {
        ProjectResponseCommands::Edit(arguments) => edit_response(arguments),
        ProjectResponseCommands::Preview(arguments) => preview_response(arguments),
        ProjectResponseCommands::Delete(arguments) => {
            ensure_registered_project(&arguments.project)?;
            let id = response_id(&arguments.response)?;
            operator_store()?
                .delete_operator_response_draft(&arguments.project, &id)
                .map_err(authority_error)?;
            if arguments.json {
                print_json(&serde_json::json!({
                    "schema_version": OPERATOR_RESPONSE_SCHEMA_VERSION,
                    "project": arguments.project,
                    "deleted_response": id,
                }))
            } else {
                println!("Deleted draft response {id}.");
                Ok(())
            }
        }
        ProjectResponseCommands::Activate(arguments) => {
            let base = build_base_intelligence(&arguments.project)?;
            let id = response_id(&arguments.response)?;
            let stored = load_operator_response(&arguments.project, &id)?;
            let target = resolve_bound_target(&base, &stored.target)?;
            let supersedes =
                optional_response_id(arguments.supersedes.as_deref())?.or(stored.supersedes);
            let activated = operator_store()?
                .activate_operator_response(&arguments.project, &id, &target, supersedes)
                .map_err(authority_error)?;
            render_response(&arguments.project, activated, arguments.json)
        }
        ProjectResponseCommands::Retire(arguments) => {
            ensure_registered_project(&arguments.project)?;
            let id = response_id(&arguments.response)?;
            let retired = operator_store()?
                .retire_operator_response(&arguments.project, &id)
                .map_err(authority_error)?;
            render_response(&arguments.project, retired, arguments.json)
        }
        ProjectResponseCommands::Withdraw(arguments) => {
            ensure_registered_project(&arguments.project)?;
            let id = response_id(&arguments.response)?;
            let withdrawn = operator_store()?
                .withdraw_operator_response(&arguments.project, &id)
                .map_err(authority_error)?;
            render_response(&arguments.project, withdrawn, arguments.json)
        }
        ProjectResponseCommands::Reaffirm(arguments) => {
            let base = build_base_intelligence(&arguments.project)?;
            let id = response_id(&arguments.response)?;
            let stored = load_operator_response(&arguments.project, &id)?;
            let target = resolve_bound_target(&base, &stored.target)?;
            let reaffirmed = operator_store()?
                .reaffirm_operator_response(&arguments.project, &id, target)
                .map_err(authority_error)?;
            render_response(&arguments.project, reaffirmed, arguments.json)
        }
    }
}

fn edit_response(arguments: ProjectResponseEditArgs) -> Result<(), ProjectError> {
    ensure_registered_project(&arguments.project)?;
    let id = response_id(&arguments.response)?;
    let existing = load_operator_response(&arguments.project, &id)?;
    let confidence = arguments.confidence.map(operator_confidence);
    let intent = arguments.intent.map(operator_intent);
    let payload = match existing.payload {
        OperatorResponsePayload::Rejection(mut payload) => {
            if let Some(reason) = arguments.reason {
                payload.reason = reason;
            }
            if let Some(intent) = intent {
                payload.intent = Some(intent);
            }
            if let Some(confidence) = confidence {
                payload.confidence = Some(confidence);
            }
            OperatorResponsePayload::Rejection(payload)
        }
        OperatorResponsePayload::Correction(mut payload) => {
            if let Some(statement) = arguments.statement {
                payload.replacement_statement = statement;
            }
            if let Some(reason) = arguments.reason {
                payload.reason = Some(reason);
            }
            if let Some(intent) = intent {
                payload.intent = intent;
            }
            if let Some(confidence) = confidence {
                payload.confidence = Some(confidence);
            }
            OperatorResponsePayload::Correction(payload)
        }
        OperatorResponsePayload::Dispute(mut payload) => {
            if let Some(reason) = arguments.reason {
                payload.reason = reason;
            }
            if let Some(intent) = intent {
                payload.intent = Some(intent);
            }
            if let Some(confidence) = confidence {
                payload.confidence = Some(confidence);
            }
            OperatorResponsePayload::Dispute(payload)
        }
        OperatorResponsePayload::Annotation(_) | OperatorResponsePayload::Acceptance(_) => {
            return Err(ProjectError::Authority(
                "only draft-first rejection, correction, and dispute responses are editable"
                    .to_string(),
            ));
        }
    };
    let edited = operator_store()?
        .edit_operator_response_draft(&arguments.project, &id, payload)
        .map_err(authority_error)?;
    render_response(&arguments.project, edited, arguments.json)
}

fn preview_response(arguments: ProjectResponseShowArgs) -> Result<(), ProjectError> {
    let base = build_base_intelligence(&arguments.project)?;
    let id = response_id(&arguments.response)?;
    let response = load_projected_operator_response(&arguments.project, &id)?;
    let current_target = resolve_bound_target(&base, &response.target).ok();
    let target_matches = current_target
        .as_ref()
        .is_some_and(|target| response.target.exact_match(target));
    let preview = OperatorPreviewReport {
        schema_version: OPERATOR_RESPONSE_SCHEMA_VERSION,
        project: arguments.project,
        response,
        target_matches,
        current_target,
    };
    if arguments.json {
        print_json(&preview)
    } else {
        println!("Response preview");
        println!("  ID: {}", preview.response.id);
        println!("  Type: {}", preview.response.payload.kind());
        println!("  Lifecycle: {}", preview.response.lifecycle.as_str());
        println!("  Target: {}", preview.response.target.target_id);
        println!(
            "  Target fingerprint: {}",
            preview.response.target.evidence_fingerprint
        );
        println!(
            "  Current target matches: {}",
            if preview.target_matches { "yes" } else { "no" }
        );
        Ok(())
    }
}

fn create_operator_response(
    project: &str,
    target: OperatorTargetBinding,
    operator: OperatorIdentity,
    payload: OperatorResponsePayload,
    supersedes: Option<OperatorResponseId>,
    json: bool,
) -> Result<(), ProjectError> {
    let mut request = NewOperatorResponse::new(project, target.clone(), operator, payload);
    if let Some(supersedes) = supersedes {
        request = request.with_supersedes(supersedes);
    }
    let response = operator_store()?
        .create_operator_response(request)
        .map_err(authority_error)?;
    if !json {
        println!("Target: {}", target.target_id);
        println!("Target fingerprint: {}", target.evidence_fingerprint);
    }
    render_response(project, response, json)
}

fn resolve_authority_target(
    base: &ProjectIntelligence,
    selector: &str,
) -> Result<OperatorTargetBinding, ProjectError> {
    let targets =
        authority_targets(base).map_err(|error| ProjectError::Intelligence(error.to_string()))?;
    let exact = targets
        .iter()
        .filter(|target| target.target_id == selector)
        .cloned()
        .collect::<Vec<_>>();
    if let [target] = exact.as_slice() {
        return Ok(target.clone());
    }
    let friendly = targets
        .into_iter()
        .filter(|target| target.rule_id.as_deref() == Some(selector))
        .collect::<Vec<_>>();
    match friendly.as_slice() {
        [target] => Ok(target.clone()),
        [] => Err(ProjectError::TargetNotFound(selector.to_string())),
        _ => Err(ProjectError::TargetAmbiguous(selector.to_string())),
    }
}

fn resolve_bound_target(
    base: &ProjectIntelligence,
    binding: &OperatorTargetBinding,
) -> Result<OperatorTargetBinding, ProjectError> {
    if let Some(rule_id) = &binding.rule_id {
        resolve_authority_target(base, rule_id)
    } else {
        resolve_authority_target(base, &binding.target_id)
    }
}

fn operator_store() -> Result<KnowledgeStore, ProjectError> {
    KnowledgeStore::open_default().map_err(|error| ProjectError::Authority(error.to_string()))
}

fn ensure_registered_project(project: &str) -> Result<(), ProjectError> {
    let config = load_project_config()?;
    resolve_registered_project(&config, project).map(|_| ())
}

fn load_operator_response(
    project: &str,
    id: &OperatorResponseId,
) -> Result<OperatorResponse, ProjectError> {
    operator_store()?
        .get_operator_response(project, id)
        .map_err(authority_error)?
        .ok_or_else(|| ProjectError::Authority(format!("operator response not found: {id}")))
}

fn projected_operator_responses(project: &str) -> Result<Vec<OperatorResponse>, ProjectError> {
    let base = build_base_intelligence(project)?;
    let mut responses = operator_store()?
        .list_operator_responses(project)
        .map_err(authority_error)?;
    let resolved = OperatorAuthorityResolver
        .resolve(&base, &responses, true)
        .map_err(|error| ProjectError::Intelligence(error.to_string()))?;
    let lifecycle = resolved
        .explanations
        .into_iter()
        .map(|explanation| (explanation.response_id, explanation.lifecycle))
        .collect::<std::collections::BTreeMap<_, _>>();
    for response in &mut responses {
        if let Some(projected) = lifecycle.get(&response.id) {
            response.lifecycle = *projected;
        }
    }
    Ok(responses)
}

fn load_projected_operator_response(
    project: &str,
    id: &OperatorResponseId,
) -> Result<OperatorResponse, ProjectError> {
    projected_operator_responses(project)?
        .into_iter()
        .find(|response| response.id == *id)
        .ok_or_else(|| ProjectError::Authority(format!("operator response not found: {id}")))
}

fn response_id(value: &str) -> Result<OperatorResponseId, ProjectError> {
    OperatorResponseId::parse(value.to_string()).map_err(authority_error)
}

fn optional_response_id(value: Option<&str>) -> Result<Option<OperatorResponseId>, ProjectError> {
    value.map(response_id).transpose()
}

fn operator_identity(arguments: OperatorArgs) -> Result<OperatorIdentity, ProjectError> {
    match arguments.operator {
        Some(stable_key) => {
            let display_name = arguments
                .operator_name
                .unwrap_or_else(|| stable_key.clone());
            OperatorIdentity::named(stable_key, display_name).map_err(authority_error)
        }
        None => OperatorIdentity::local("Local operator").map_err(authority_error),
    }
}

fn operator_confidence(value: OperatorConfidenceArg) -> OperatorConfidence {
    match value {
        OperatorConfidenceArg::Certain => OperatorConfidence::Certain,
        OperatorConfidenceArg::High => OperatorConfidence::High,
        OperatorConfidenceArg::Medium => OperatorConfidence::Medium,
        OperatorConfidenceArg::Tentative => OperatorConfidence::Tentative,
    }
}

fn operator_intent(value: OperatorIntentArg) -> OperatorIntent {
    match value {
        OperatorIntentArg::Architecture => OperatorIntent::Architecture,
        OperatorIntentArg::Decision => OperatorIntent::Decision,
        OperatorIntentArg::Preference => OperatorIntent::Preference,
        OperatorIntentArg::TemporaryConstraint => OperatorIntent::TemporaryConstraint,
        OperatorIntentArg::Experiment => OperatorIntent::Experiment,
        OperatorIntentArg::Context => OperatorIntent::Context,
    }
}

fn authority_error(error: astra_knowledge::KnowledgeError) -> ProjectError {
    ProjectError::Authority(error.to_string())
}

fn render_response(
    project: &str,
    response: OperatorResponse,
    json: bool,
) -> Result<(), ProjectError> {
    if json {
        return print_json(&OperatorResponseReport {
            schema_version: OPERATOR_RESPONSE_SCHEMA_VERSION,
            project: project.to_string(),
            response,
        });
    }
    println!("Operator response");
    println!("  ID: {}", response.id);
    println!("  Type: {}", response.payload.kind());
    println!("  Lifecycle: {}", response.lifecycle.as_str());
    println!("  Target: {}", response.target.target_id);
    println!(
        "  Target fingerprint: {}",
        response.target.evidence_fingerprint
    );
    Ok(())
}

fn print_response_list(project: &str, responses: &[OperatorResponse]) {
    println!("Operator responses for {project}");
    if responses.is_empty() {
        println!("No operator responses.");
        return;
    }
    println!("RESPONSE               TYPE        LIFECYCLE");
    for response in responses {
        println!(
            "{:<22} {:<11} {}",
            response.id,
            response.payload.kind(),
            response.lifecycle.as_str()
        );
    }
}

fn print_response_history(project: &str, history: &[OperatorResponseHistoryEntry]) {
    println!("Operator response history for {project}");
    if history.is_empty() {
        println!("No committed operator-response transactions.");
        return;
    }
    println!("TRANSACTION                 OPERATION     RESPONSE");
    for entry in history {
        println!(
            "{:<27} {:<13} {}",
            entry.transaction_id,
            history_operation(entry.operation),
            entry
                .response
                .as_ref()
                .map_or("-", |response| response.id.as_str())
        );
    }
}

fn history_operation(operation: OperatorHistoryOperation) -> &'static str {
    match operation {
        OperatorHistoryOperation::Create => "create",
        OperatorHistoryOperation::EditDraft => "edit_draft",
        OperatorHistoryOperation::Activate => "activate",
        OperatorHistoryOperation::DeleteDraft => "delete_draft",
        OperatorHistoryOperation::Retire => "retire",
        OperatorHistoryOperation::Withdraw => "withdraw",
        OperatorHistoryOperation::Reaffirm => "reaffirm",
    }
}

fn print_json(value: &impl Serialize) -> Result<(), ProjectError> {
    println!(
        "{}",
        serde_json::to_string_pretty(value)
            .map_err(|error| ProjectError::Serialization(error.to_string()))?
    );
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
