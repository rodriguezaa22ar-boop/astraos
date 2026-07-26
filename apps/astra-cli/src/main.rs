use astra_config::{config_path, load, save, Config};
use astra_core::VERSION;
use astra_dashboard::run_dashboard;
use astra_projects::valid_project_name;
use astra_system::{command_exists, command_output};
use astra_workspaces::{
    add_workspace as add_workspace_entry, astra_root, list_workspaces as list_workspace_entries,
    remove_workspace as remove_workspace_entry, workspace_path,
};
use clap::{Parser, Subcommand};
use std::{
    env, fs,
    path::Path,
    process::{Command, ExitCode, Stdio},
};
use tracing::{error, info};

#[derive(Debug, Parser)]
#[command(name = "astra", version = VERSION, about = "AstraOS command center")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Dashboard {
        #[arg(long)]
        interactive: bool,
    },
    Doctor,
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommands,
    },
    Project {
        kind: String,
        name: String,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Debug, Subcommand)]
enum WorkspaceCommands {
    List,
    Add {
        name: String,
        path: String,
        #[arg(long)]
        force: bool,
    },
    Remove {
        name: String,
    },
    Open {
        name: String,
        #[arg(long)]
        create: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommands {
    Path,
    Show,
    Init {
        #[arg(long)]
        force: bool,
    },
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(tracing::Level::WARN)
        .compact()
        .init();

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            error!("{message}");
            eprintln!("astra: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();

    match cli
        .command
        .unwrap_or(Commands::Dashboard { interactive: false })
    {
        Commands::Dashboard { interactive } => {
            if interactive {
                let config = load().map_err(|error| error.to_string())?;
                run_dashboard(config).map_err(|error| error.to_string())
            } else {
                dashboard()
            }
        }
        Commands::Doctor => doctor(),
        Commands::Workspace { command } => workspace_command(command),
        Commands::Project { kind, name } => create_project(&kind, &name),
        Commands::Config { command } => config_command(command),
    }
}

fn config_command(command: ConfigCommands) -> Result<(), String> {
    match command {
        ConfigCommands::Path => {
            println!("{}", config_path().display());
            Ok(())
        }
        ConfigCommands::Show => {
            let config = load().map_err(|error| error.to_string())?;
            let rendered = toml::to_string_pretty(&config).map_err(|error| error.to_string())?;
            print!("{rendered}");
            Ok(())
        }
        ConfigCommands::Init { force } => {
            let path = config_path();

            if path.exists() && !force {
                return Err(format!(
                    "configuration already exists at {}; use --force to overwrite it",
                    path.display()
                ));
            }

            let config = Config::default();
            save(&config).map_err(|error| error.to_string())?;
            info!("configuration initialized");
            println!("✓ Created {}", path.display());
            Ok(())
        }
    }
}

fn workspace_command(command: WorkspaceCommands) -> Result<(), String> {
    match command {
        WorkspaceCommands::List => list_workspaces(),
        WorkspaceCommands::Add { name, path, force } => add_workspace(&name, &path, force),
        WorkspaceCommands::Remove { name } => remove_workspace(&name),
        WorkspaceCommands::Open { name, create } => open_workspace(&name, create),
    }
}

fn list_workspaces() -> Result<(), String> {
    let config = load().map_err(|error| error.to_string())?;

    for (name, path) in list_workspace_entries(&config) {
        println!("{name}\t{path}");
    }

    Ok(())
}

fn add_workspace(name: &str, path: &str, force: bool) -> Result<(), String> {
    let mut config = load().map_err(|error| error.to_string())?;
    add_workspace_entry(&mut config, name, path, force).map_err(|error| error.to_string())?;
    save(&config).map_err(|error| error.to_string())?;

    println!("✓ Added workspace {name}");
    Ok(())
}

fn remove_workspace(name: &str) -> Result<(), String> {
    let mut config = load().map_err(|error| error.to_string())?;
    remove_workspace_entry(&mut config, name).map_err(|error| error.to_string())?;
    save(&config).map_err(|error| error.to_string())?;

    println!("✓ Removed workspace {name}");
    Ok(())
}

fn dashboard() -> Result<(), String> {
    let config = load().map_err(|error| error.to_string())?;

    println!("════════════════════════════════════════════════════");
    println!("              ASTRA COMMAND CENTER");
    println!("════════════════════════════════════════════════════");

    let user = env::var("USER").unwrap_or_else(|_| "unknown".to_string());
    let hostname = command_output("hostname", &[]).unwrap_or_else(|| "unknown".to_string());
    let macos =
        command_output("sw_vers", &["-productVersion"]).unwrap_or_else(|| "unknown".to_string());

    println!("Version:   {VERSION}");
    println!("Host:      {hostname}");
    println!("User:      {user}");
    println!("macOS:     {macos}");
    println!("Workspace: {}", config.workspace.root);

    println!("\nSystem");
    for tool in [
        "brew", "git", "gh", "node", "python3", "docker", "codex", "ollama",
    ] {
        let marker = if command_exists(tool) { "✓" } else { "!" };
        println!("{marker} {tool}");
    }

    println!("\nProjects");
    for (label, key) in [
        ("Astraeus Omnia", "omnia"),
        ("Omnia API Foundry", "api"),
        ("Games", "games"),
        ("Cybersecurity", "cyber"),
        ("AI Lab", "ai"),
    ] {
        let exists = workspace_path(&config, key)
            .map(|path| path.exists())
            .unwrap_or(false);
        let marker = if exists { "✓" } else { "!" };
        println!("{marker} {label}");
    }

    Ok(())
}

fn doctor() -> Result<(), String> {
    println!("AstraOS Doctor\n");

    let required = [
        "brew", "git", "gh", "node", "npm", "pnpm", "python3", "uv", "jq", "yq", "rg", "fd", "fzf",
    ];

    let mut failures = 0;

    for tool in required {
        if command_exists(tool) {
            println!("✓ {tool}");
        } else {
            println!("✗ {tool}");
            failures += 1;
        }
    }

    println!("\nAuthentication");
    if command_success("gh", &["auth", "status"]) {
        println!("✓ GitHub authenticated");
    } else {
        println!("! Run: gh auth login");
    }

    println!("\nSystem Security");
    run_passthrough("csrutil", &["status"]);
    run_passthrough("spctl", &["--status"]);
    run_passthrough("fdesetup", &["status"]);

    println!("\nStorage");
    run_passthrough("df", &["-h", "/"]);

    if failures == 0 {
        println!("\n✓ AstraOS is healthy");
        Ok(())
    } else {
        Err(format!("{failures} required tool(s) missing"))
    }
}

fn open_workspace(name: &str, create: bool) -> Result<(), String> {
    let config = load().map_err(|error| error.to_string())?;
    let path = workspace_path(&config, name).ok_or_else(|| format!("unknown workspace: {name}"))?;

    if !path.exists() {
        if !create {
            return Err(format!(
                "workspace directory does not exist: {} (use --create to create it)",
                path.display()
            ));
        }

        fs::create_dir_all(&path)
            .map_err(|error| format!("could not create workspace {}: {error}", path.display()))?;
    }

    if command_exists(&config.editor.command) {
        Command::new(&config.editor.command)
            .arg(&path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("failed to open editor: {error}"))?;
    }

    match name {
        "ai" => start_ollama(),
        "cyber" => {
            open_application("Wireshark");
            open_application("Burp Suite");
        }
        _ => {}
    }

    println!("✓ Opened {name} workspace");
    println!("{}", path.display());
    Ok(())
}

fn create_project(kind: &str, name: &str) -> Result<(), String> {
    let config = load().map_err(|error| error.to_string())?;
    if !valid_project_name(name) {
        return Err(
            "project name may contain only letters, numbers, dots, dashes, and underscores"
                .to_string(),
        );
    }

    let path = astra_root(&config).join("projects").join(name);

    if path.exists() {
        return Err(format!("project already exists: {}", path.display()));
    }

    fs::create_dir_all(&path)
        .map_err(|error| format!("could not create project directory: {error}"))?;

    run_in_directory(&path, "git", &["init", "-b", "main"])?;

    let result = match kind {
        "node" => create_node_project(&path),
        "python" => create_python_project(&path),
        "static" => create_static_project(&path),
        unsupported => Err(format!("unsupported project type: {unsupported}")),
    };

    if let Err(error) = result {
        let _ = fs::remove_dir_all(&path);
        return Err(error);
    }

    fs::write(
        path.join("README.md"),
        format!("# {name}\n\nCreated with AstraOS.\n"),
    )
    .map_err(|error| format!("could not write README.md: {error}"))?;

    let _ = run_in_directory(&path, "git", &["add", "."]);
    let _ = run_in_directory(&path, "git", &["commit", "-m", "chore: initialize project"]);

    println!("✓ Created {kind} project: {}", path.display());
    Ok(())
}

fn create_node_project(path: &Path) -> Result<(), String> {
    run_in_directory(path, "npm", &["init", "-y"])?;
    run_in_directory(
        path,
        "npm",
        &[
            "install",
            "-D",
            "typescript",
            "tsx",
            "eslint",
            "prettier",
            "vitest",
        ],
    )?;
    run_in_directory(path, "npx", &["tsc", "--init"])?;

    fs::create_dir_all(path.join("src")).map_err(|error| error.to_string())?;
    fs::create_dir_all(path.join("test")).map_err(|error| error.to_string())?;
    fs::write(
        path.join("src/index.ts"),
        "export function main(): void {\n  console.log(\"Astra project ready.\");\n}\n\nmain();\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        path.join(".gitignore"),
        "node_modules/\ndist/\n.env\n.DS_Store\ncoverage/\n",
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn create_python_project(path: &Path) -> Result<(), String> {
    run_in_directory(path, "uv", &["init"])?;
    fs::create_dir_all(path.join("tests")).map_err(|error| error.to_string())?;
    fs::write(
        path.join(".gitignore"),
        ".venv/\n__pycache__/\n.pytest_cache/\n.env\n.DS_Store\n",
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn create_static_project(path: &Path) -> Result<(), String> {
    fs::write(
        path.join("index.html"),
        "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n  <title>Astra Project</title>\n</head>\n<body>\n  <main><h1>Astra Project Ready</h1></main>\n</body>\n</html>\n",
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn command_success(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn run_passthrough(command: &str, args: &[&str]) {
    let _ = Command::new(command).args(args).status();
}

fn run_in_directory(directory: &Path, command: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(command)
        .current_dir(directory)
        .args(args)
        .status()
        .map_err(|error| format!("failed to run {command}: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("{command} exited with status {status}"))
    }
}

fn start_ollama() {
    if command_exists("brew") {
        let _ = Command::new("brew")
            .args(["services", "start", "ollama"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn open_application(name: &str) {
    let _ = Command::new("open")
        .args(["-a", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
