use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::path::Path;
use std::{fs, path::PathBuf, process::Command};
use tempfile::TempDir;

fn create_temp_home() -> TempDir {
    tempfile::tempdir().expect("temp dir")
}

fn run_workspace_command(
    args: &[&str],
    home_dir: &PathBuf,
    current_dir: &PathBuf,
) -> assert_cmd::assert::Assert {
    let mut command = Command::cargo_bin("astra").expect("binary");
    command.env("HOME", home_dir);
    command.env("XDG_CONFIG_HOME", home_dir.join(".config"));
    command.current_dir(current_dir);
    command.arg("workspace");
    command.args(args);
    command.assert()
}

fn write_config(home_dir: &Path, contents: &str) {
    let config_dir = home_dir.join(".config/astra");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("config.toml"), contents).unwrap();
}

#[test]
fn workspace_list_prints_registry_entries_in_deterministic_order() {
    let temp_home = create_temp_home();
    let home = temp_home.path().to_path_buf();
    let temp_dir = tempfile::tempdir().unwrap();
    let current_dir = temp_dir.path().to_path_buf();
    write_config(
        &home,
        r#"[workspace]
root = "/tmp/workspaces"

[editor]
command = "true"

[ai]
provider = "ollama"

[cyber]
labs = "/tmp/cyber"

[workspaces]
beta = "/tmp/beta"
alpha = "/tmp/alpha"
"#,
    );

    let assert = run_workspace_command(&["list"], &home, &current_dir);
    assert
        .success()
        .stdout(predicate::str::contains("alpha").and(predicate::str::contains("beta")));
}

#[test]
fn workspace_add_requires_valid_name() {
    let temp_home = create_temp_home();
    let home = temp_home.path().to_path_buf();
    let temp_dir = tempfile::tempdir().unwrap();
    let current_dir = temp_dir.path().to_path_buf();

    let assert = run_workspace_command(&["add", "bad name", "/tmp/example"], &home, &current_dir);
    assert
        .failure()
        .stderr(predicate::str::contains("invalid workspace name"));
}

#[test]
fn workspace_add_refuses_duplicate_names_without_force() {
    let temp_home = create_temp_home();
    let home = temp_home.path().to_path_buf();
    let temp_dir = tempfile::tempdir().unwrap();
    let current_dir = temp_dir.path().to_path_buf();
    write_config(
        &home,
        r#"[workspace]
root = "/tmp/workspaces"

[editor]
command = "true"

[ai]
provider = "ollama"

[cyber]
labs = "/tmp/cyber"

[workspaces]
alpha = "/tmp/old"
"#,
    );

    let assert = run_workspace_command(&["add", "alpha", "/tmp/new"], &home, &current_dir);
    assert
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn workspace_add_replaces_existing_entry_when_force_is_used() {
    let temp_home = create_temp_home();
    let home = temp_home.path().to_path_buf();
    let temp_dir = tempfile::tempdir().unwrap();
    let current_dir = temp_dir.path().to_path_buf();
    write_config(
        &home,
        r#"[workspace]
root = "/tmp/workspaces"

[editor]
command = "true"

[ai]
provider = "ollama"

[cyber]
labs = "/tmp/cyber"

[workspaces]
alpha = "/tmp/old"
"#,
    );

    let assert = run_workspace_command(
        &["add", "alpha", "/tmp/new", "--force"],
        &home,
        &current_dir,
    );
    assert.success();

    let config_path = home.join(".config/astra/config.toml");
    let contents = fs::read_to_string(&config_path).unwrap();
    assert!(contents.contains("alpha = \"/tmp/new\""));
}

#[test]
fn workspace_add_normalizes_relative_and_tilde_paths() {
    let temp_home = create_temp_home();
    let home = temp_home.path().to_path_buf();
    let temp_dir = tempfile::tempdir().unwrap();
    let current_dir = temp_dir.path().to_path_buf();
    write_config(
        &home,
        r#"[workspace]
root = "/tmp/workspaces"

[editor]
command = "true"

[ai]
provider = "ollama"

[cyber]
labs = "/tmp/cyber"
"#,
    );

    let assert = run_workspace_command(
        &["add", "relative", "./nested/workspace"],
        &home,
        &current_dir,
    );
    assert.success();

    let config_path = home.join(".config/astra/config.toml");
    let contents = fs::read_to_string(&config_path).unwrap();
    assert!(contents.contains("relative"));
    assert!(contents.contains(
        &current_dir
            .join("nested/workspace")
            .to_string_lossy()
            .to_string()
    ));

    let assert = run_workspace_command(&["add", "tilde", "~/project/demo"], &home, &current_dir);
    assert.success();
    let contents = fs::read_to_string(&config_path).unwrap();
    assert!(contents.contains("tilde"));
    assert!(contents.contains(&home.join("project/demo").to_string_lossy().to_string()));
}

#[test]
fn workspace_remove_persists_known_and_unknown_entries() {
    let temp_home = create_temp_home();
    let home = temp_home.path().to_path_buf();
    let temp_dir = tempfile::tempdir().unwrap();
    let current_dir = temp_dir.path().to_path_buf();
    write_config(
        &home,
        r#"[workspace]
root = "/tmp/workspaces"

[editor]
command = "true"

[ai]
provider = "ollama"

[cyber]
labs = "/tmp/cyber"

[workspaces]
alpha = "/tmp/alpha"
"#,
    );

    let remove_known = run_workspace_command(&["remove", "alpha"], &home, &current_dir);
    remove_known.success();

    let config_path = home.join(".config/astra/config.toml");
    let contents = fs::read_to_string(&config_path).unwrap();
    assert!(!contents.contains("alpha"));

    let remove_unknown = run_workspace_command(&["remove", "missing"], &home, &current_dir);
    remove_unknown
        .failure()
        .stderr(predicate::str::contains("unknown workspace"));
}

#[test]
fn workspace_open_errors_for_missing_directory_without_create() {
    let temp_home = create_temp_home();
    let home = temp_home.path().to_path_buf();
    let temp_dir = tempfile::tempdir().unwrap();
    let current_dir = temp_dir.path().to_path_buf();
    let missing_path = current_dir.join("missing-workspace");
    write_config(
        &home,
        &format!(
            "[workspace]
root = \"/tmp/workspaces\"

[editor]
command = \"true\"

[ai]
provider = \"ollama\"

[cyber]
labs = \"/tmp/cyber\"

[workspaces]
alpha = \"{}\"
",
            missing_path.display()
        ),
    );

    let assert = run_workspace_command(&["open", "alpha"], &home, &current_dir);
    assert
        .failure()
        .stderr(predicate::str::contains("does not exist"));
    assert!(!missing_path.exists());
}

#[test]
fn workspace_open_creates_missing_directory_with_create_flag() {
    let temp_home = create_temp_home();
    let home = temp_home.path().to_path_buf();
    let temp_dir = tempfile::tempdir().unwrap();
    let current_dir = temp_dir.path().to_path_buf();
    let missing_path = current_dir.join("missing-workspace");
    write_config(
        &home,
        &format!(
            "[workspace]
root = \"/tmp/workspaces\"

[editor]
command = \"true\"

[ai]
provider = \"ollama\"

[cyber]
labs = \"/tmp/cyber\"

[workspaces]
alpha = \"{}\"
",
            missing_path.display()
        ),
    );

    let assert = run_workspace_command(&["open", "alpha", "--create"], &home, &current_dir);
    assert.success();
    assert!(missing_path.exists());
}
