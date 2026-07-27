use assert_cmd::Command;
use predicates::prelude::*;
use std::{fs, path::Path};

fn project() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("project directory");
    fs::create_dir(directory.path().join("src")).expect("source directory");
    fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname = \"context-fixture\"\nversion = \"0.1.0\"\n",
    )
    .expect("manifest");
    fs::write(
        directory.path().join("src/main.rs"),
        "fn main() { println!(\"hello\"); }\n",
    )
    .expect("source");
    directory
}

fn astra(home: &Path) -> Command {
    let mut command = Command::cargo_bin("astra").expect("binary");
    command
        .env("HOME", home)
        .env("ASTRA_CONFIG_DIR", home.join("astra-config"))
        .env("PATH", "");
    command
}

#[test]
fn context_text_inspects_a_path_without_loading_user_configuration() {
    let project = project();
    let home = tempfile::tempdir().expect("home");
    astra(home.path())
        .args(["context", project.path().to_str().expect("UTF-8 path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Packages:"))
        .stdout(predicate::str::contains("context-fixture [rust]"))
        .stdout(predicate::str::contains("rust"));
    assert!(!home.path().join("astra-config/config.toml").exists());
}

#[test]
fn context_path_defaults_to_the_current_directory() {
    let project = project();
    let home = tempfile::tempdir().expect("home");
    astra(home.path())
        .current_dir(project.path())
        .arg("context")
        .assert()
        .success()
        .stdout(predicate::str::contains("context-fixture [rust]"));
}

#[test]
fn context_json_is_deterministic_and_versioned() {
    let project = project();
    let home = tempfile::tempdir().expect("home");
    let path = project.path().to_str().expect("UTF-8 path");
    let first = astra(home.path())
        .args(["context", path, "--json"])
        .output()
        .expect("first output");
    let second = astra(home.path())
        .args(["context", path, "--json"])
        .output()
        .expect("second output");
    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);
    let output = String::from_utf8(first.stdout).expect("UTF-8 output");
    assert!(output.contains("\"schema_version\": 1"));
    assert!(!output.contains("\"duration\""));
}

#[test]
fn context_tree_renders_the_semantic_project_tree() {
    let project = project();
    let home = tempfile::tempdir().expect("home");
    astra(home.path())
        .args([
            "context",
            "tree",
            project.path().to_str().expect("UTF-8 path"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("├── packages"))
        .stdout(predicate::str::contains("└── entry points"));
}

#[test]
fn context_reports_a_missing_root_without_panicking() {
    let home = tempfile::tempdir().expect("home");
    let missing = home.path().join("missing");
    astra(home.path())
        .args(["context", missing.to_str().expect("UTF-8 path")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("project root does not exist"));
}

#[test]
fn every_context_view_ignores_and_preserves_malformed_astra_configuration() {
    let project = project();
    let home = tempfile::tempdir().expect("home");
    let config_dir = home.path().join("astra-config");
    fs::create_dir(&config_dir).expect("config directory");
    let config = config_dir.join("config.toml");
    let sentinel = b"this is intentionally not valid = [toml";
    fs::write(&config, sentinel).expect("malformed config");
    let project_path = project.path().to_str().expect("UTF-8 path");

    for arguments in [
        vec!["context", project_path],
        vec!["context", project_path, "--json"],
        vec!["context", "tree", project_path],
    ] {
        astra(home.path()).args(arguments).assert().success();
    }
    assert_eq!(fs::read(config).expect("config contents"), sentinel);
}
