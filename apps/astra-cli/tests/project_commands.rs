use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::{fs, path::Path};
use tempfile::{tempdir, TempDir};

fn project() -> TempDir {
    let directory = tempdir().expect("project directory");
    write_project_at(directory.path());
    directory
}

fn write_project_at(directory: &Path) {
    fs::create_dir_all(directory.join("src")).expect("source directory");
    fs::write(
        directory.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\n",
    )
    .expect("workspace manifest");
    fs::write(directory.join("src/lib.rs"), "pub fn ready() {}\n").expect("source");
}

fn project_without_commands() -> TempDir {
    let directory = tempdir().expect("project directory");
    fs::write(directory.path().join("README.md"), "# No actions\n").expect("readme");
    directory
}

fn write_config(home: &Path, entries: &[(&str, &Path)]) {
    let config_dir = home.join("astra-config");
    fs::create_dir_all(&config_dir).expect("config directory");
    let mut config = String::from(
        "[workspace]\nroot = \"/tmp/workspaces\"\n\n[editor]\ncommand = \"true\"\n\n[ai]\nprovider = \"ollama\"\n\n[cyber]\nlabs = \"/tmp/cyber\"\n\n[workspaces]\n",
    );
    for (name, path) in entries {
        config.push_str(&format!("{name} = {:?}\n", path.to_string_lossy()));
    }
    fs::write(config_dir.join("config.toml"), config).expect("configuration");
}

fn astra(home: &Path, current_dir: &Path) -> Command {
    let mut command = Command::cargo_bin("astra").expect("binary");
    command
        .env("HOME", home)
        .env("ASTRA_CONFIG_DIR", home.join("astra-config"))
        .env("PATH", "")
        .current_dir(current_dir);
    command
}

#[test]
fn project_help_lists_the_explicit_subcommands() {
    astra(tempdir().expect("home").path(), Path::new("."))
        .args(["project", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("inspect"))
        .stdout(predicate::str::contains("commands"))
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("create"));
}

#[test]
fn project_run_help_requires_and_describes_dry_run() {
    astra(
        tempdir().expect("home").path(),
        tempdir().expect("current directory").path(),
    )
    .args(["project", "run", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("<NAME>"))
    .stdout(predicate::str::contains("<ACTION>"))
    .stdout(predicate::str::contains("--dry-run"))
    .stdout(predicate::str::contains("without starting a process"));
}

#[test]
fn legacy_ambiguous_project_positionals_are_rejected_in_favor_of_create() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");

    astra(home.path(), current.path())
        .args(["project", "node", "demo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
fn project_list_is_deterministic_and_aligned() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");
    write_config(
        home.path(),
        &[
            ("beta", Path::new("/tmp/beta")),
            ("alpha", Path::new("/tmp/alpha")),
        ],
    );

    let first = astra(home.path(), current.path())
        .args(["project", "list"])
        .output()
        .expect("first list output");
    let second = astra(home.path(), current.path())
        .args(["project", "list"])
        .output()
        .expect("second list output");

    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(
        String::from_utf8(first.stdout).expect("list output is UTF-8"),
        "PROJECT  PATH\nalpha    /tmp/alpha\nbeta     /tmp/beta\n"
    );
}

#[test]
fn project_list_without_config_does_not_create_a_config_file() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");
    let config_path = home.path().join("astra-config/config.toml");

    astra(home.path(), current.path())
        .args(["project", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("PROJECT"));

    assert!(!config_path.exists());
}

#[test]
fn project_inspect_resolves_a_registered_project_and_supports_json() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");
    let project = project();
    write_config(home.path(), &[("demo", project.path())]);

    astra(home.path(), current.path())
        .args(["project", "inspect", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Project: "))
        .stdout(predicate::str::contains("Packages:"));

    astra(home.path(), current.path())
        .args(["project", "inspect", "demo", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"schema_version\": 1"))
        .stdout(predicate::str::contains("\"demo\"").not());
}

#[test]
fn project_commands_discovers_build_check_and_test_without_execution() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");
    let project = project();
    write_config(home.path(), &[("demo", project.path())]);

    astra(home.path(), current.path())
        .args(["project", "commands", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Available actions for demo"))
        .stdout(predicate::str::contains("build   cargo build --workspace"))
        .stdout(predicate::str::contains("check   cargo check --workspace"))
        .stdout(predicate::str::contains("test    cargo test --workspace"));
}

#[test]
fn project_commands_json_is_versioned_and_structured() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");
    let project = project();
    write_config(home.path(), &[("demo", project.path())]);

    let output = astra(home.path(), current.path())
        .args(["project", "commands", "demo", "--json"])
        .output()
        .expect("command output");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("action JSON");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["project"]["name"], "demo");
    assert_eq!(
        json["project"]["root"],
        project
            .path()
            .canonicalize()
            .expect("canonical project")
            .to_string_lossy()
            .as_ref()
    );
    let actions = json["actions"].as_array().expect("actions array");
    assert_eq!(actions.len(), 3);
    assert_eq!(actions[0]["id"], "build");
    assert_eq!(actions[1]["id"], "check");
    assert_eq!(actions[2]["id"], "test");
    assert_eq!(actions[0]["executable"], "cargo");
    assert_eq!(
        actions[0]["arguments"],
        serde_json::json!(["build", "--workspace"])
    );
    assert_eq!(actions[0]["working_directory"], json["project"]["root"]);
    assert_eq!(actions[0]["source"], "context_engine");
    assert_eq!(actions[0]["confidence"], "high");
}

#[test]
fn unknown_and_missing_projects_fail_concisely() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");
    write_config(
        home.path(),
        &[("missing", &current.path().join("does-not-exist"))],
    );

    astra(home.path(), current.path())
        .args(["project", "commands", "nonexistent"])
        .assert()
        .failure()
        .stderr("astra: unknown project: nonexistent\n");
    astra(home.path(), current.path())
        .args(["project", "commands", "missing"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("project path does not exist"))
        .stderr(predicate::str::contains("ERROR").not());
}

#[test]
fn project_without_supported_actions_succeeds_with_a_clear_message() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");
    let project = project_without_commands();
    write_config(home.path(), &[("empty", project.path())]);

    astra(home.path(), current.path())
        .args(["project", "commands", "empty"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No supported actions detected."));
}

#[test]
fn project_commands_never_executes_detected_commands() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");
    let project = project();
    write_config(home.path(), &[("demo", project.path())]);

    // PATH is empty, so an accidental cargo invocation would fail the command.
    astra(home.path(), current.path())
        .args(["project", "commands", "demo"])
        .assert()
        .success();
}

#[test]
fn project_run_dry_run_supports_build_check_and_test_without_execution() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");
    let project = project();
    write_config(home.path(), &[("demo", project.path())]);

    for action in ["build", "check", "test"] {
        let output = astra(home.path(), current.path())
            .args(["project", "run", "demo", action, "--dry-run"])
            .output()
            .expect("dry-run output");
        assert!(output.status.success(), "action {action} failed");
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 output");
        assert!(stdout.contains(&format!("Action: {action}")));
        assert!(stdout.contains("Policy: allowed"));
        assert!(stdout.contains("Dry run complete. No process was started."));
    }
}

#[test]
fn project_run_json_is_versioned_and_never_starts_a_process() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");
    let project = project();
    write_config(home.path(), &[("demo", project.path())]);

    let output = astra(home.path(), current.path())
        .args(["project", "run", "demo", "check", "--dry-run", "--json"])
        .output()
        .expect("dry-run JSON");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("dry-run JSON");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["project"]["name"], "demo");
    assert_eq!(json["action"]["id"], "check");
    assert_eq!(
        json["action"]["arguments"],
        serde_json::json!(["check", "--workspace"])
    );
    assert_eq!(json["policy"]["decision"], "allowed");
    assert_eq!(json["execution"]["mode"], "dry_run");
    assert_eq!(json["execution"]["process_started"], false);
}

#[test]
fn project_run_requires_explicit_dry_run() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");

    astra(home.path(), current.path())
        .args(["project", "run", "demo", "check"])
        .assert()
        .failure()
        .stderr("astra: real action execution is not available; use --dry-run\n");
}

#[test]
fn project_run_reports_unknown_unsupported_and_unavailable_actions() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");
    let project = project();
    let empty = project_without_commands();
    let missing = current.path().join("missing");
    write_config(
        home.path(),
        &[
            ("demo", project.path()),
            ("empty", empty.path()),
            ("missing", &missing),
        ],
    );

    astra(home.path(), current.path())
        .args(["project", "run", "nonexistent", "check", "--dry-run"])
        .assert()
        .failure()
        .stderr("astra: unknown project: nonexistent\n");
    astra(home.path(), current.path())
        .args(["project", "run", "demo", "deploy", "--dry-run"])
        .assert()
        .failure()
        .stderr("astra: unsupported action: deploy\n");
    astra(home.path(), current.path())
        .args(["project", "run", "empty", "check", "--dry-run"])
        .assert()
        .failure()
        .stderr("astra: project does not expose action: check for empty\n");
    astra(home.path(), current.path())
        .args(["project", "run", "missing", "check", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("project path does not exist"));
}

#[test]
fn project_run_does_not_create_config_or_modify_project_files() {
    let home = tempdir().expect("home");
    let current = tempdir().expect("current directory");
    let project_root = home.path().join("Developer/projects/astraos");
    write_project_at(&project_root);
    let manifest_before = fs::read(project_root.join("Cargo.toml")).expect("manifest");
    let source_before = fs::read(project_root.join("src/lib.rs")).expect("source");
    let config_path = home.path().join("astra-config/config.toml");

    astra(home.path(), current.path())
        .args(["project", "run", "astraos", "check", "--dry-run"])
        .assert()
        .success();

    assert!(!config_path.exists());
    assert_eq!(
        fs::read(project_root.join("Cargo.toml")).expect("manifest"),
        manifest_before
    );
    assert_eq!(
        fs::read(project_root.join("src/lib.rs")).expect("source"),
        source_before
    );
}
