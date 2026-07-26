use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::{fs, path::Path, process::Command};

fn write_config(home: &Path, workspace: &Path, layout_name: &str, terminal: &str) {
    let config_dir = home.join(".config/astra");
    fs::create_dir_all(&config_dir).expect("config directory");
    fs::write(
        config_dir.join("config.toml"),
        format!(
            r#"[workspace]
root = "/tmp/workspaces"

[editor]
command = "/Applications/Editor With Spaces.app/editor"

[ai]
provider = "ollama"

[cyber]
labs = "/tmp/cyber"

[terminal]
command = "{terminal}"

[workspaces]
project = "{}"

[workspace_layouts.{layout_name}]
editor = false
ollama = false

[[workspace_layouts.{layout_name}.tabs]]
name = "development"
command = ["cargo", "watch", "-x", "check all"]

[[workspace_layouts.{layout_name}.tabs.panes]]
target = 0
direction = "right"
percent = 45
command = ["git", "status", "--short"]
"#,
            workspace.display()
        ),
    )
    .expect("configuration");
}

fn astra(home: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut command = Command::cargo_bin("astra").expect("astra binary");
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .args(args)
        .assert()
}

#[test]
fn layout_is_selected_by_layout_name_only() {
    let home = tempfile::tempdir().expect("home");
    let workspace = tempfile::tempdir().expect("workspace");
    write_config(
        home.path(),
        workspace.path(),
        "rust-development",
        "missing wezterm",
    );

    astra(home.path(), &["workspace", "layout", "rust-development"])
        .success()
        .stdout(
            predicate::str::contains("Layout: rust-development")
                .and(predicate::str::contains("Pane 1"))
                .and(predicate::str::contains(
                    r#"["cargo","watch","-x","check all"]"#,
                )),
        );
}

#[test]
fn dry_run_supports_different_workspace_and_layout_names_without_wezterm() {
    let home = tempfile::tempdir().expect("home");
    let workspace = tempfile::tempdir().expect("workspace");
    write_config(
        home.path(),
        workspace.path(),
        "rust-development",
        "/Applications/WezTerm Dev.app/Contents/MacOS/wezterm",
    );

    astra(
        home.path(),
        &[
            "workspace",
            "launch",
            "project",
            "--layout",
            "rust-development",
            "--dry-run",
        ],
    )
    .success()
    .stdout(
        predicate::str::contains(r#"workspace "project" with layout "rust-development""#)
            .and(predicate::str::contains(
                r#"exec="/Applications/WezTerm Dev.app/Contents/MacOS/wezterm""#,
            ))
            .and(predicate::str::contains(r#""check all""#)),
    );
}

#[test]
fn omitted_layout_uses_same_name_default() {
    let home = tempfile::tempdir().expect("home");
    let workspace = tempfile::tempdir().expect("workspace");
    write_config(home.path(), workspace.path(), "project", "missing wezterm");

    astra(
        home.path(),
        &["workspace", "launch", "project", "--dry-run"],
    )
    .success()
    .stdout(predicate::str::contains(r#"with layout "project""#));
}

#[test]
fn omitted_layout_explains_how_to_select_another_layout() {
    let home = tempfile::tempdir().expect("home");
    let workspace = tempfile::tempdir().expect("workspace");
    write_config(
        home.path(),
        workspace.path(),
        "rust-development",
        "missing wezterm",
    );

    astra(
        home.path(),
        &["workspace", "launch", "project", "--dry-run"],
    )
    .failure()
    .stderr(
        predicate::str::contains("workspace layout 'project' was not found")
            .and(predicate::str::contains("--layout <layout-name>")),
    );
}
