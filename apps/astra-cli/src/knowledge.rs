use astra_config::load_if_present;
use astra_execution::{ExecutionEngine, ExecutionResult, VerificationVerdict};
use astra_knowledge::{
    Confidence, Evidence, EvidenceKind, KnowledgeCategory, KnowledgeClaim, KnowledgeNamespace,
    KnowledgeStore, Validity, ValidityCondition, KNOWLEDGE_SCHEMA_VERSION,
};
use astra_workspaces::workspace_path;
use clap::{Args, Subcommand};
use serde::Serialize;

#[derive(Debug, Subcommand)]
pub(crate) enum KnowledgeCommands {
    /// List projects with persisted knowledge.
    List(KnowledgeOutputArgs),
    /// Show all persisted knowledge for a project.
    Show(ProjectKnowledgeArgs),
    /// Show factual knowledge for a project.
    Facts(ProjectKnowledgeArgs),
    /// Show execution verifications for a project.
    Verifications(ProjectKnowledgeArgs),
    /// Show recorded decisions for a project.
    Decisions(ProjectKnowledgeArgs),
}

#[derive(Debug, Args)]
pub(crate) struct KnowledgeOutputArgs {
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ProjectKnowledgeArgs {
    #[arg(value_name = "PROJECT")]
    project: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct KnowledgeListReport {
    schema_version: u32,
    projects: Vec<String>,
}

#[derive(Debug, Serialize)]
struct KnowledgeClaimsReport {
    schema_version: u32,
    project: String,
    claims: Vec<KnowledgeClaim>,
}

pub(crate) fn run(command: KnowledgeCommands) -> Result<(), String> {
    let store = KnowledgeStore::open_default().map_err(|error| error.to_string())?;
    match command {
        KnowledgeCommands::List(arguments) => list(&store, arguments.json),
        KnowledgeCommands::Show(arguments) => {
            show(&store, &arguments.project, None, arguments.json)
        }
        KnowledgeCommands::Facts(arguments) => show(
            &store,
            &arguments.project,
            Some(KnowledgeCategory::Fact),
            arguments.json,
        ),
        KnowledgeCommands::Verifications(arguments) => {
            verifications(&store, &arguments.project, arguments.json)
        }
        KnowledgeCommands::Decisions(arguments) => show(
            &store,
            &arguments.project,
            Some(KnowledgeCategory::Decision),
            arguments.json,
        ),
    }
}

fn list(store: &KnowledgeStore, json: bool) -> Result<(), String> {
    let projects = store.list_projects().map_err(|error| error.to_string())?;
    if json {
        let report = KnowledgeListReport {
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
            projects,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if projects.is_empty() {
        println!("No persisted project knowledge.");
        return Ok(());
    }
    println!("PROJECT");
    for project in projects {
        println!("{project}");
    }
    Ok(())
}

fn show(
    store: &KnowledgeStore,
    project: &str,
    category: Option<KnowledgeCategory>,
    json: bool,
) -> Result<(), String> {
    let claims = store
        .query_claims(&KnowledgeNamespace::project(project), category)
        .map_err(|error| error.to_string())?;
    render_claims(project, claims, json)
}

fn verifications(store: &KnowledgeStore, project: &str, json: bool) -> Result<(), String> {
    let claims = store
        .query_claims(
            &KnowledgeNamespace::project(project),
            Some(KnowledgeCategory::Verification),
        )
        .map_err(|error| error.to_string())?;
    let current_state = current_project_state(project);
    let claims = claims
        .into_iter()
        .map(|claim| claim.observed_state(current_state.as_deref()))
        .collect();
    render_claims(project, claims, json)
}

fn render_claims(project: &str, claims: Vec<KnowledgeClaim>, json: bool) -> Result<(), String> {
    if json {
        let report = KnowledgeClaimsReport {
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
            project: project.to_string(),
            claims,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    println!("Knowledge for {project}");
    if claims.is_empty() {
        println!("No knowledge claims found.");
        return Ok(());
    }
    for claim in claims {
        println!(
            "- [{}] {} = {} (confidence: {}, validity: {})",
            claim.category.singular(),
            claim.predicate,
            compact_value(&claim.value),
            claim.confidence.as_str(),
            claim.validity.as_str()
        );
        println!("  id: {}", claim.id);
        if !claim.evidence.is_empty() {
            println!(
                "  evidence: {}",
                claim
                    .evidence
                    .iter()
                    .map(|evidence| format!("{}:{}", evidence.kind.as_str(), evidence.identifier))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    Ok(())
}

fn compact_value(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<unavailable>".to_string())
}

fn current_project_state(project: &str) -> Option<String> {
    let config = load_if_present().ok().flatten()?;
    let root = workspace_path(&config, project)?.canonicalize().ok()?;
    let report = crate::context::analyze_without_processes(&root).ok()?;
    let actions = astra_actions::resolve_actions(&report.context.validation_commands);
    let action = astra_actions::select_action(&actions, astra_actions::ActionId::Check)?;
    let action = if action.command.working_directory.is_relative() {
        let mut action = action;
        action.command.working_directory =
            if action.command.working_directory == std::path::Path::new(".") {
                root.clone()
            } else {
                root.join(&action.command.working_directory)
            };
        action
    } else {
        action
    };
    let project_reference = astra_actions::ProjectReference {
        name: project.to_string(),
        root,
    };
    ExecutionEngine::new()
        .plan(&project_reference, &action)
        .ok()
        .map(|plan| plan.source_state.combined_fingerprint.to_string())
}

pub(crate) fn record_verification(
    project_name: &str,
    result: &ExecutionResult,
) -> Result<(), String> {
    let validity = if result.state.changed {
        Validity::Stale
    } else {
        Validity::Current
    };
    let verdict = match result.execution.verdict {
        VerificationVerdict::VerifiedCheck => "verified_check",
        VerificationVerdict::CommandFailed => "command_failed",
        VerificationVerdict::SourceStateChanged => "source_state_changed",
        VerificationVerdict::CommandFailedAndSourceStateChanged => {
            "command_failed_and_source_state_changed"
        }
        VerificationVerdict::Interrupted => "interrupted",
    };
    let state = result.state.after.combined_fingerprint.to_string();
    let action_fingerprint = result.action_fingerprint.to_string();
    let plan_fingerprint = result.plan_fingerprint.to_string();
    let claim = KnowledgeClaim::new(
        KnowledgeCategory::Verification,
        format!("project:{project_name}"),
        result.action.id.as_str().to_string(),
        serde_json::json!({
            "project": project_name,
            "project_root": result.project.root,
            "action": result.action.id.as_str(),
            "executable": result.action.command.executable,
            "arguments": result.action.command.arguments,
            "verdict": verdict,
            "exit_code": result.execution.exit_code,
            "process_started": result.execution.process_started,
            "interrupted": result.execution.interrupted,
            "state_fingerprint": state,
            "action_fingerprint": action_fingerprint,
            "plan_fingerprint": plan_fingerprint,
        }),
        vec![Evidence::new(
            EvidenceKind::ExecutionResult,
            format!("execution:{plan_fingerprint}"),
        )
        .with_locator(format!("project:{project_name}"))
        .with_fingerprints(vec![state.clone(), action_fingerprint, plan_fingerprint])],
        Confidence::Certain,
        validity,
    )
    .map(|claim| {
        claim.with_validity_conditions(vec![ValidityCondition::state_bound(
            &state,
            result.action_fingerprint.to_string(),
            result.plan_fingerprint.to_string(),
        )])
    })
    .map_err(|error| error.to_string())?;
    KnowledgeStore::open_default()
        .map_err(|error| error.to_string())?
        .add_claim(&KnowledgeNamespace::project(project_name), &claim)
        .map_err(|error| error.to_string())?;
    Ok(())
}
