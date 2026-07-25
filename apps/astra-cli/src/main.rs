use std::{
    env, fs,
    path::Path,
    process::{Command, ExitCode, Stdio},
};

use astra_core::VERSION;
use astra_projects::valid_project_name;
use astra_system::{command_exists, command_output};
use astra_workspaces::{astra_root, workspace_path};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("astra: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    let command = arguments.next().unwrap_or_else(|| "dashboard".to_string());

    match command.as_str() {
        "dashboard" => dashboard(),

        "doctor" => doctor(),

        "workspace" | "open" => {
            let name = arguments.next().ok_or("usage: astra workspace <name>")?;

            open_workspace(&name)
        }

        "project" => {
            let project_type = arguments
                .next()
                .ok_or("usage: astra project <node|python|static> <name>")?;

            let name = arguments
                .next()
                .ok_or("usage: astra project <node|python|static> <name>")?;

            create_project(&project_type, &name)
        }

        "version" | "--version" | "-V" => {
            println!("astra {VERSION}");
            Ok(())
        }

        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }

        unknown => Err(format!("unknown command: {unknown}\n\nRun `astra help`.")),
    }
}

fn dashboard() -> Result<(), String> {
    println!("════════════════════════════════════════════════════");
    println!("              ASTRA COMMAND CENTER");
    println!("════════════════════════════════════════════════════");

    let user = env::var("USER").unwrap_or_else(|_| "unknown".to_string());

    let hostname = command_output("hostname", &[]).unwrap_or_else(|| "unknown".to_string());

    let macos_version =
        command_output("sw_vers", &["-productVersion"]).unwrap_or_else(|| "unknown".to_string());

    println!("Version:   {VERSION}");
    println!("Host:      {hostname}");
    println!("User:      {user}");
    println!("macOS:     {macos_version}");
    println!("Workspace: {}", astra_root().display());

    println!("\nSystem");

    for tool in [
        "brew", "git", "gh", "node", "python3", "docker", "codex", "ollama",
    ] {
        print_tool_status(tool);
    }

    println!("\nProjects");

    for (label, workspace_name) in [
        ("Astraeus Omnia", "omnia"),
        ("Omnia API Foundry", "api"),
        ("Games", "games"),
        ("Cybersecurity", "cyber"),
        ("AI Lab", "ai"),
    ] {
        let exists = workspace_path(workspace_name)
            .map(|path| path.exists())
            .unwrap_or(false);

        let symbol = if exists { "✓" } else { "!" };

        println!("{symbol} {label}");
    }

    Ok(())
}

fn doctor() -> Result<(), String> {
    println!("AstraOS Doctor\n");

    let required_tools = [
        "brew", "git", "gh", "node", "npm", "pnpm", "python3", "uv", "jq", "yq", "rg", "fd", "fzf",
    ];

    let mut failures = 0;

    for tool in required_tools {
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

fn open_workspace(name: &str) -> Result<(), String> {
    let path = workspace_path(name).ok_or_else(|| format!("unknown workspace: {name}"))?;

    fs::create_dir_all(&path)
        .map_err(|error| format!("could not create workspace {}: {error}", path.display()))?;

    if command_exists("code") {
        Command::new("code")
            .arg(&path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("failed to open VS Code: {error}"))?;
    }

    match name {
        "ai" => {
            start_ollama();
        }

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

fn create_project(project_type: &str, name: &str) -> Result<(), String> {
    if !valid_project_name(name) {
        return Err(
            "project name may contain only letters, numbers, dots, dashes, and underscores"
                .to_string(),
        );
    }

    let project_path = astra_root().join("projects").join(name);

    if project_path.exists() {
        return Err(format!(
            "project already exists: {}",
            project_path.display()
        ));
    }

    fs::create_dir_all(&project_path)
        .map_err(|error| format!("could not create project directory: {error}"))?;

    run_in_directory(&project_path, "git", &["init", "-b", "main"])?;

    let result = match project_type {
        "node" => create_node_project(&project_path),
        "python" => create_python_project(&project_path),
        "static" => create_static_project(&project_path),
        unsupported => Err(format!("unsupported project type: {unsupported}")),
    };

    if let Err(error) = result {
        let _ = fs::remove_dir_all(&project_path);
        return Err(error);
    }

    fs::write(
        project_path.join("README.md"),
        format!("# {name}\n\nCreated with AstraOS.\n"),
    )
    .map_err(|error| format!("could not write README.md: {error}"))?;

    let _ = run_in_directory(&project_path, "git", &["add", "."]);

    let _ = run_in_directory(
        &project_path,
        "git",
        &["commit", "-m", "chore: initialize project"],
    );

    if command_exists("code") {
        let _ = Command::new("code").arg(&project_path).spawn();
    }

    println!(
        "✓ Created {project_type} project: {}",
        project_path.display()
    );

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
        concat!(
            "export function main(): void {\n",
            "  console.log(\"Astra project ready.\");\n",
            "}\n\n",
            "main();\n"
        ),
    )
    .map_err(|error| error.to_string())?;

    fs::write(
        path.join(".gitignore"),
        concat!(
            "node_modules/\n",
            "dist/\n",
            ".env\n",
            ".DS_Store\n",
            "coverage/\n"
        ),
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn create_python_project(path: &Path) -> Result<(), String> {
    run_in_directory(path, "uv", &["init"])?;

    fs::create_dir_all(path.join("tests")).map_err(|error| error.to_string())?;

    fs::write(
        path.join(".gitignore"),
        concat!(
            ".venv/\n",
            "__pycache__/\n",
            ".pytest_cache/\n",
            ".env\n",
            ".DS_Store\n"
        ),
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn create_static_project(path: &Path) -> Result<(), String> {
    fs::write(
        path.join("index.html"),
        concat!(
            "<!doctype html>\n",
            "<html lang=\"en\">\n",
            "<head>\n",
            "  <meta charset=\"utf-8\">\n",
            "  <meta name=\"viewport\" ",
            "content=\"width=device-width,initial-scale=1\">\n",
            "  <title>Astra Project</title>\n",
            "</head>\n",
            "<body>\n",
            "  <main>\n",
            "    <h1>Astra Project Ready</h1>\n",
            "  </main>\n",
            "</body>\n",
            "</html>\n"
        ),
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn start_ollama() {
    if !command_exists("brew") {
        return;
    }

    let _ = Command::new("brew")
        .args(["services", "start", "ollama"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn open_application(name: &str) {
    let _ = Command::new("open")
        .args(["-a", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn command_success(command: &str, arguments: &[&str]) -> bool {
    Command::new(command)
        .args(arguments)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn run_passthrough(command: &str, arguments: &[&str]) {
    let _ = Command::new(command).args(arguments).status();
}

fn run_in_directory(directory: &Path, command: &str, arguments: &[&str]) -> Result<(), String> {
    let status = Command::new(command)
        .current_dir(directory)
        .args(arguments)
        .status()
        .map_err(|error| format!("failed to run {command}: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("{command} exited with status {status}"))
    }
}

fn print_tool_status(tool: &str) {
    let symbol = if command_exists(tool) { "✓" } else { "!" };

    println!("{symbol} {tool}");
}

fn print_help() {
    println!(
        "AstraOS {VERSION}

Usage:
  astra dashboard
  astra doctor
  astra workspace <name>
  astra project <node|python|static> <name>
  astra version
  astra help

Workspaces:
  omnia
  api
  games
  cyber
  ai
  learning
  projects"
    );
}
