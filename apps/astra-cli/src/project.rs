use crate::{context, ProjectCommands, ProjectCommandsArgs, ProjectInspectArgs};
use astra_actions::{resolve_actions, ProjectAction, ProjectActionReport, ProjectReference};
use astra_config::{load_if_present, Config};
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
    Serialization(String),
    Output(String),
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
            Self::Serialization(message) => {
                write!(formatter, "could not serialize project actions: {message}")
            }
            Self::Output(message) => formatter.write_str(message),
        }
    }
}

pub(crate) fn run(command: ProjectCommands) -> Result<(), ProjectError> {
    match command {
        ProjectCommands::List => list(),
        ProjectCommands::Inspect(arguments) => inspect(arguments),
        ProjectCommands::Commands(arguments) => commands(arguments),
        ProjectCommands::Create { kind, name } => {
            crate::create_project(&kind, &name).map_err(ProjectError::Output)
        }
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
