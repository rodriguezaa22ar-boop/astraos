use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("astra: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "dashboard".to_string());

    match command.as_str() {
        "dashboard" => dashboard(),
        "doctor" => doctor(),
        "workspace" | "open" => {
            let name = args.next().ok_or("usage: astra workspace <name>")?;
            workspace(&name)
        }
        "project" => {
            let kind = args
                .next()
                .ok_or("usage: astra project <node|python|static> <name>")?;
            let name = args
                .next()
                .ok_or("usage: astra project <node|python|static> <name>")?;
            create_project(&kind, &name)
        }
        "version" | "--version" | "-V" => {
            println!("astra {VERSION}");
            Ok(())
        }
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown command: {other}\n\nRun `astra help`.")),
    }
}

fn astra_root() -> PathBuf {
    env::var_os("ASTRA_ROOT")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join("Developer")))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn workspace_path(name: &str) -> Option<PathBuf> {
    let root = astra_root();
    match name {
        "omnia" => Some(root.join("astraeus-omnia")),
        "api" => Some(root.join("omnia-api-foundry")),
        "games" => Some(root.join("games")),
        "cyber" => Some(root.join("cybersecurity")),
        "ai" => Some(root.join("ai")),
        "learning" => Some(root.join("learning")),
        "projects" => Some(root.join("projects")),
        _ => None,
    }
}

fn dashboard() -> Result<(), String> {
    println!("════════════════════════════════════════════════════");
    println!("              ASTRA COMMAND CENTER");
    println!("════════════════════════════════════════════════════");

    let user = env::var("USER").unwrap_or_else(|_| "unknown".into());
    let host = output("hostname", &[]).unwrap_or_else(|| "unknown".into());
    let macos = output("sw_vers", &["-productVersion"]).unwrap_or_else(|| "unknown".into());

    println!("Version:   {VERSION}");
    println!("Host:      {host}");
    println!("User:      {user}");
    println!("macOS:     {macos}");
    println!("Workspace: {}", astra_root().display());

    println!("\nSystem");
    for tool in [
        "brew", "git", "gh", "node", "python3", "docker", "codex", "ollama",
    ] {
        print_tool_status(tool);
    }

    println!("\nProjects");
    for (label, key) in [
        ("Astraeus Omnia", "omnia"),
        ("Omnia API Foundry", "api"),
        ("Games", "games"),
        ("Cybersecurity", "cyber"),
        ("AI Lab", "ai"),
    ] {
        let exists = workspace_path(key).map(|p| p.exists()).unwrap_or(false);
        println!("{} {label}", if exists { "✓" } else { "!" });
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
    if success("gh", &["auth", "status"]) {
        println!("✓ GitHub authenticated");
    } else {
        println!("! Run: gh auth login");
    }

    println!("\nSystem Security");
    passthrough("csrutil", &["status"]);
    passthrough("spctl", &["--status"]);
    passthrough("fdesetup", &["status"]);

    println!("\nStorage");
    passthrough("df", &["-h", "/"]);

    if failures == 0 {
        println!("\n✓ AstraOS is healthy");
        Ok(())
    } else {
        Err(format!("{failures} required tool(s) missing"))
    }
}

fn workspace(name: &str) -> Result<(), String> {
    let path = workspace_path(name).ok_or_else(|| format!("unknown workspace: {name}"))?;

    fs::create_dir_all(&path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;

    if command_exists("code") {
        Command::new("code")
            .arg(&path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to open VS Code: {e}"))?;
    }

    match name {
        "ai" if command_exists("brew") => {
            let _ = Command::new("brew")
                .args(["services", "start", "ollama"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        "cyber" => {
            open_app("Wireshark");
            open_app("Burp Suite");
        }
        _ => {}
    }

    println!("✓ Opened {name} workspace");
    println!("{}", path.display());
    Ok(())
}

fn create_project(kind: &str, name: &str) -> Result<(), String> {
    validate_project_name(name)?;

    let path = astra_root().join("projects").join(name);
    if path.exists() {
        return Err(format!("project already exists: {}", path.display()));
    }

    fs::create_dir_all(&path).map_err(|e| format!("cannot create project directory: {e}"))?;

    run_in(&path, "git", &["init", "-b", "main"])?;

    match kind {
        "node" => create_node_project(&path)?,
        "python" => create_python_project(&path)?,
        "static" => create_static_project(&path)?,
        _ => {
            let _ = fs::remove_dir_all(&path);
            return Err(format!("unsupported project type: {kind}"));
        }
    }

    fs::write(
        path.join("README.md"),
        format!("# {name}\n\nCreated with AstraOS.\n"),
    )
    .map_err(|e| e.to_string())?;

    let _ = run_in(&path, "git", &["add", "."]);
    let _ = run_in(&path, "git", &["commit", "-m", "chore: initialize project"]);

    if command_exists("code") {
        let _ = Command::new("code").arg(&path).spawn();
    }

    println!("✓ Created {kind} project: {}", path.display());
    Ok(())
}

fn create_node_project(path: &Path) -> Result<(), String> {
    run_in(path, "npm", &["init", "-y"])?;
    run_in(
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
    run_in(path, "npx", &["tsc", "--init"])?;

    fs::create_dir_all(path.join("src")).map_err(|e| e.to_string())?;
    fs::create_dir_all(path.join("test")).map_err(|e| e.to_string())?;
    fs::write(
        path.join("src/index.ts"),
        "export function main(): void {\n  console.log(\"Astra project ready.\");\n}\n\nmain();\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        path.join(".gitignore"),
        "node_modules/\ndist/\n.env\n.DS_Store\ncoverage/\n",
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn create_python_project(path: &Path) -> Result<(), String> {
    run_in(path, "uv", &["init"])?;
    fs::create_dir_all(path.join("tests")).map_err(|e| e.to_string())?;
    fs::write(
        path.join(".gitignore"),
        ".venv/\n__pycache__/\n.pytest_cache/\n.env\n.DS_Store\n",
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn create_static_project(path: &Path) -> Result<(), String> {
    fs::write(
        path.join("index.html"),
        "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n  <title>Astra Project</title>\n</head>\n<body>\n  <main><h1>Astra Project Ready</h1></main>\n</body>\n</html>\n",
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn validate_project_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));

    if valid {
        Ok(())
    } else {
        Err("project name may contain only letters, numbers, dots, dashes, and underscores".into())
    }
}

fn command_exists(command: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {command} >/dev/null 2>&1")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn print_tool_status(tool: &str) {
    println!("{} {tool}", if command_exists(tool) { "✓" } else { "!" });
}

fn output(command: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(command).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn success(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn passthrough(command: &str, args: &[&str]) {
    let _ = Command::new(command).args(args).status();
}

fn run_in(dir: &Path, command: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(command)
        .current_dir(dir)
        .args(args)
        .status()
        .map_err(|e| format!("failed to run {command}: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("{command} exited with status {status}"))
    }
}

fn open_app(name: &str) {
    let _ = Command::new("open")
        .args(["-a", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
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
  omnia  api  games  cyber  ai  learning  projects"
    );
}
