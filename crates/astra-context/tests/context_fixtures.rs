use astra_context::{
    render_json, render_text, render_tree, Confidence, DocumentKind, EntryPointKind,
    ProjectAnalyzer, RepositoryState, ScanOptions,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn isolated_tempdir() -> tempfile::TempDir {
    let mut bases = Vec::new();
    #[cfg(unix)]
    {
        bases.push(PathBuf::from("/tmp"));
        bases.push(PathBuf::from("/var/tmp"));
    }
    bases.push(std::env::temp_dir());

    for base in bases {
        let Ok(base) = base.canonicalize() else {
            continue;
        };
        if base
            .ancestors()
            .any(|ancestor| ancestor.join(".git").exists())
        {
            continue;
        }
        if let Ok(directory) = tempfile::Builder::new()
            .prefix("astra-context-test-")
            .tempdir_in(base)
        {
            return directory;
        }
    }

    panic!("no writable temporary directory without a .git ancestor was available");
}

fn fixture(name: &str) -> tempfile::TempDir {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let target = isolated_tempdir();
    copy_directory(&source, target.path()).expect("copy fixture");
    target
}

fn copy_directory(source: &Path, target: &Path) -> std::io::Result<()> {
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir(&target_path)?;
            copy_directory(&source_path, &target_path)?;
        } else {
            fs::copy(source_path, target_path)?;
        }
    }
    Ok(())
}

#[test]
fn rust_workspace_projects_packages_documentation_ci_and_license() {
    let fixture = fixture("rust-workspace");
    let report = ProjectAnalyzer::default()
        .analyze(fixture.path())
        .expect("Rust report");

    assert_eq!(
        report.context.repository.state.value,
        RepositoryState::NotRepository
    );
    assert!(report
        .context
        .languages
        .iter()
        .any(|language| language.value.id == "rust"));
    assert_eq!(report.context.workspace.packages.len(), 2);
    assert!(report
        .context
        .workspace
        .packages
        .iter()
        .any(|package| package.value.name == "fixture-app"));
    assert!(report.context.documentation.iter().any(|document| {
        document.value.kind == DocumentKind::Architecture
            && document.value.path.as_str() == "docs/ARCHITECTURE.md"
    }));
    assert!(report
        .context
        .ci
        .iter()
        .any(|ci| ci.value.provider == "github-actions"));
    assert!(report
        .context
        .license
        .files
        .iter()
        .any(|license| license.value.as_str() == "LICENSE"));
    assert!(report
        .context
        .dependencies
        .iter()
        .any(|dependency| dependency.value.name == "fixture-core"));
    let scanner_ids = report
        .scanners
        .iter()
        .map(|scanner| scanner.metadata.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        scanner_ids,
        [
            "build",
            "ci",
            "configuration",
            "dependencies",
            "documentation",
            "entry_points",
            "git",
            "inventory",
            "languages",
            "license",
            "manifests",
            "testing",
            "validation",
            "workspace",
        ]
    );
    assert!(report.scanners.iter().all(|scanner| {
        scanner.metadata.version > 0 && !scanner.metadata.description.is_empty()
    }));
}

#[test]
fn node_monorepo_preserves_argv_commands_and_package_order() {
    let fixture = fixture("node-monorepo");
    let report = ProjectAnalyzer::default()
        .analyze(fixture.path())
        .expect("Node report");

    let package_paths = report
        .context
        .workspace
        .packages
        .iter()
        .map(|package| package.value.path.as_str())
        .collect::<Vec<_>>();
    assert!(package_paths.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(report
        .context
        .languages
        .iter()
        .any(|language| language.value.id == "typescript"));
    assert!(report
        .context
        .tooling
        .package_managers
        .iter()
        .any(|tool| tool.value.id == "pnpm"));
    assert!(report.context.validation_commands.iter().any(|command| {
        command.value.executable == "pnpm" && command.value.arguments == ["run", "test"]
    }));
    let core_entries = report
        .context
        .entry_points
        .iter()
        .filter(|entry| entry.value.path.as_str() == "packages/core/src/index.ts")
        .collect::<Vec<_>>();
    assert_eq!(core_entries.len(), 1);
    assert_eq!(core_entries[0].value.kind, EntryPointKind::Library);
    assert_eq!(
        core_entries[0].value.package.as_deref(),
        Some("@fixture/core")
    );
}

#[test]
fn polyglot_fixture_reports_multiple_ecosystems_without_execution() {
    let fixture = fixture("polyglot");
    let report = ProjectAnalyzer::default()
        .analyze(fixture.path())
        .expect("polyglot report");
    let languages = report
        .context
        .languages
        .iter()
        .map(|language| language.value.id.as_str())
        .collect::<Vec<_>>();
    assert!(languages.contains(&"go"));
    assert!(languages.contains(&"python"));
    assert!(languages.contains(&"nix"));
    assert!(report
        .context
        .workspace
        .packages
        .iter()
        .any(|package| package.value.ecosystem == "python"));
}

#[test]
fn repeated_fixture_serialization_is_byte_for_byte_stable() {
    let fixture = fixture("rust-workspace");
    let analyzer = ProjectAnalyzer::default();
    let first = analyzer.analyze(fixture.path()).expect("first");
    let second = analyzer.analyze(fixture.path()).expect("second");
    assert_eq!(
        render_json(&first).expect("first JSON"),
        render_json(&second).expect("second JSON")
    );
    assert_eq!(render_text(&first), render_text(&second));
    assert_eq!(render_tree(&first), render_tree(&second));
}

#[test]
fn malformed_manifests_produce_a_partial_report() {
    let fixture = isolated_tempdir();
    fs::write(fixture.path().join("Cargo.toml"), "[package\nname =").expect("malformed manifest");
    let report = ProjectAnalyzer::default()
        .analyze(fixture.path())
        .expect("partial report");
    assert!(report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "manifest.parse_failed"));
    assert!(report
        .scanners
        .iter()
        .any(|scanner| scanner.metadata.id == "manifests"));
    let manifest = report
        .scanners
        .iter()
        .find(|scanner| scanner.metadata.id == "manifests")
        .expect("manifest scanner");
    assert_eq!(manifest.diagnostic_codes, ["manifest.parse_failed"]);
}

#[cfg(unix)]
#[test]
fn file_and_directory_symlinks_are_not_followed() {
    use std::os::unix::fs::symlink;

    let outside = isolated_tempdir();
    fs::write(outside.path().join("hidden.rs"), "fn hidden() {}").expect("outside file");
    let fixture = isolated_tempdir();
    fs::write(fixture.path().join("visible.py"), "print('visible')").expect("visible");
    symlink(outside.path(), fixture.path().join("linked")).expect("symlink");
    symlink(
        outside.path().join("hidden.rs"),
        fixture.path().join("linked.rs"),
    )
    .expect("file symlink");

    let report = ProjectAnalyzer::default()
        .analyze(fixture.path())
        .expect("report");
    assert!(report
        .context
        .languages
        .iter()
        .any(|language| language.value.id == "python"));
    assert!(!report
        .context
        .languages
        .iter()
        .any(|language| language.value.id == "rust"));
}

#[test]
fn evidence_paths_are_project_relative() {
    let fixture = fixture("rust-workspace");
    let report = ProjectAnalyzer::default()
        .analyze(fixture.path())
        .expect("report");
    for evidence in report
        .context
        .languages
        .iter()
        .flat_map(|language| &language.evidence)
    {
        if let Some(path) = &evidence.path {
            assert!(!PathBuf::from(path.as_str()).is_absolute());
        }
    }
}

#[test]
fn empty_inventory_and_absence_insights_have_completion_evidence() {
    let fixture = isolated_tempdir();
    let report = ProjectAnalyzer::default()
        .analyze(fixture.path())
        .expect("report");
    assert_eq!(report.context.size.confidence, Confidence::Certain);
    assert!(!report.context.size.evidence.is_empty());
    assert!(report
        .insights
        .iter()
        .filter(|insight| insight.value.code.ends_with("not_detected"))
        .all(|insight| !insight.evidence.is_empty()));
    let testing = report
        .insights
        .iter()
        .find(|insight| insight.value.code == "testing.not_detected")
        .expect("testing absence insight");
    let rules = testing
        .evidence
        .iter()
        .map(|evidence| evidence.rule.as_str())
        .collect::<Vec<_>>();
    assert!(rules.contains(&"inventory.scan_complete"));
    assert!(rules.contains(&"manifest.scan_complete"));
}

#[test]
fn truncated_inventory_does_not_assert_absent_project_features() {
    let fixture = isolated_tempdir();
    fs::write(fixture.path().join("a.txt"), "").expect("first file");
    fs::write(fixture.path().join("b.txt"), "").expect("second file");
    let analyzer = ProjectAnalyzer::new(ScanOptions {
        max_files: 1,
        ..ScanOptions::default()
    })
    .expect("analyzer");
    let report = analyzer.analyze(fixture.path()).expect("report");

    assert!(report.context.size.value.truncated);
    assert!(report
        .insights
        .iter()
        .any(|insight| insight.value.code == "inventory.truncated"));
    assert!(!report
        .insights
        .iter()
        .any(|insight| insight.value.code.ends_with("not_detected")));
}

#[test]
fn rescanning_observes_changes_without_creating_cache_files() {
    let fixture = isolated_tempdir();
    fs::write(fixture.path().join("main.py"), "print('first')\n").expect("first source");
    let analyzer = ProjectAnalyzer::default();
    let before_paths = relative_files(fixture.path());
    let first = analyzer.analyze(fixture.path()).expect("first report");
    assert_eq!(relative_files(fixture.path()), before_paths);

    fs::write(fixture.path().join("main.go"), "package main\n").expect("second source");
    let expected_paths = relative_files(fixture.path());
    let second = analyzer.analyze(fixture.path()).expect("second report");
    assert_eq!(relative_files(fixture.path()), expected_paths);
    assert_eq!(
        second.context.size.value.files,
        first.context.size.value.files + 1
    );
    assert!(second
        .context
        .languages
        .iter()
        .any(|language| language.value.id == "go"));
}

#[test]
fn sensitive_paths_and_remote_dependency_credentials_never_reach_json() {
    let fixture = isolated_tempdir();
    fs::create_dir(fixture.path().join(".env.local")).expect("environment directory");
    fs::create_dir(fixture.path().join("secrets")).expect("secrets directory");
    fs::create_dir(fixture.path().join("keys")).expect("keys directory");
    fs::write(fixture.path().join(".env"), "TOKEN=top-secret").expect("environment");
    fs::write(
        fixture.path().join(".env.local/leak.py"),
        "PASSWORD='hidden'",
    )
    .expect("nested environment");
    fs::write(
        fixture.path().join("secrets/token.rs"),
        "const TOKEN: &str = \"hidden\";",
    )
    .expect("secret source");
    fs::write(fixture.path().join("keys/private.pem"), "PRIVATE KEY").expect("private key");
    fs::write(
        fixture.path().join("package.json"),
        r#"{
            "name": "safe-package",
            "main": ".env",
            "dependencies": {
                "remote": "https://user:super-secret@example.invalid/package.tgz"
            }
        }"#,
    )
    .expect("package manifest");

    let report = ProjectAnalyzer::default()
        .analyze(fixture.path())
        .expect("report");
    let json = render_json(&report).expect("JSON");
    for forbidden in [
        "top-secret",
        "super-secret",
        "PRIVATE KEY",
        "secrets/token.rs",
        ".env.local/leak.py",
        "keys/private.pem",
    ] {
        assert!(!json.contains(forbidden), "JSON leaked {forbidden}");
    }
    assert!(!report
        .context
        .entry_points
        .iter()
        .any(|entry| entry.value.path.as_str() == ".env"));
}

#[test]
fn embedded_fixture_repositories_are_not_promoted_to_project_context() {
    let fixture = isolated_tempdir();
    write_file(
        fixture.path(),
        "Cargo.toml",
        r#"
            [package]
            name = "production-root"
            version = "0.1.0"
            edition = "2021"

            [workspace]
            members = ["crates/app"]
        "#,
    );
    write_file(fixture.path(), "src/lib.rs", "pub fn production() {}\n");
    write_file(
        fixture.path(),
        "tests/api.rs",
        "#[test]\nfn api_works() {}\n",
    );
    write_file(fixture.path(), "README.md", "# Production project\n");
    write_file(
        fixture.path(),
        "crates/app/Cargo.toml",
        r#"
            [package]
            name = "production-app"
            version = "0.1.0"
            edition = "2021"
        "#,
    );
    write_file(
        fixture.path(),
        "crates/app/src/lib.rs",
        "pub fn application() {}\n",
    );
    let embedded = fixture.path().join("tests/fixtures");
    fs::create_dir_all(&embedded).expect("embedded fixtures directory");
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
        &embedded,
    )
    .expect("copy embedded fixture repositories");

    let report = ProjectAnalyzer::default()
        .analyze(fixture.path())
        .expect("project report");

    let packages = report
        .context
        .workspace
        .packages
        .iter()
        .map(|package| package.value.name.as_str())
        .collect::<Vec<_>>();
    assert!(packages.contains(&"production-root"));
    assert!(packages.contains(&"production-app"));
    for fixture_package in [
        "node-monorepo-fixture",
        "@fixture/web",
        "@fixture/core",
        "polyglot-fixture",
        "fixture-app",
        "fixture-core",
    ] {
        assert!(!packages.contains(&fixture_package), "{fixture_package}");
    }

    let languages = report
        .context
        .languages
        .iter()
        .map(|language| language.value.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(languages, ["rust"]);

    let build_systems = report
        .context
        .tooling
        .build_systems
        .iter()
        .map(|tool| tool.value.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(build_systems, ["cargo"]);
    let testing_frameworks = report
        .context
        .tooling
        .testing_frameworks
        .iter()
        .map(|tool| tool.value.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(testing_frameworks, ["cargo-test"]);
    let package_managers = report
        .context
        .tooling
        .package_managers
        .iter()
        .map(|tool| tool.value.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(package_managers, ["cargo"]);
    let workspace_kinds = report
        .context
        .workspace
        .kinds
        .iter()
        .map(|kind| kind.value.as_str())
        .collect::<Vec<_>>();
    assert_eq!(workspace_kinds, ["cargo"]);
    assert!(report.context.dependencies.is_empty());

    assert!(report.context.documentation.iter().all(|document| !document
        .value
        .path
        .as_str()
        .contains("/fixtures/")));
    assert!(report.context.documentation.iter().any(|document| {
        document.value.path.as_str() == "README.md" && document.value.kind == DocumentKind::Readme
    }));
    assert!(report.context.entry_points.iter().all(|entry| !entry
        .value
        .path
        .as_str()
        .contains("/fixtures/")));
    assert!(report
        .context
        .configuration
        .iter()
        .all(|configuration| !configuration.value.path.as_str().contains("/fixtures/")));

    assert_eq!(report.context.size.value.test_files, 1);
    assert!(report.context.size.evidence.iter().any(|evidence| {
        evidence
            .path
            .as_ref()
            .is_some_and(|path| path.as_str() == "tests/api.rs")
    }));
    assert!(!report
        .insights
        .iter()
        .any(|insight| insight.value.code == "testing.not_detected"));

    assert_eq!(report.context.validation_commands.len(), 3);
    assert!(report.context.validation_commands.iter().all(|command| {
        command.value.executable == "cargo"
            && command.value.working_directory.as_str() == "."
            && command.value.arguments.contains(&"--workspace".to_string())
    }));
    assert!(report
        .context
        .validation_commands
        .iter()
        .all(|command| command.value.executable != "pnpm"
            && command.value.executable != "go"
            && command.value.executable != "uv"));
}

#[test]
fn a_selected_project_root_named_fixtures_remains_production_scope() {
    let parent = isolated_tempdir();
    let root = parent.path().join("fixtures");
    write_file(
        &root,
        "Cargo.toml",
        r#"
            [package]
            name = "legitimate-fixtures-project"
            version = "0.1.0"
            edition = "2021"
        "#,
    );
    write_file(&root, "src/lib.rs", "pub fn library() {}\n");

    let report = ProjectAnalyzer::default()
        .analyze(&root)
        .expect("project report");

    assert!(report
        .context
        .workspace
        .packages
        .iter()
        .any(|package| package.value.name == "legitimate-fixtures-project"));
    assert!(report
        .context
        .languages
        .iter()
        .any(|language| language.value.id == "rust"));
}

fn write_file(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("file parent");
    }
    fs::write(path, contents).expect("fixture file");
}

fn relative_files(root: &Path) -> Vec<PathBuf> {
    let mut values = fs::read_dir(root)
        .expect("read fixture")
        .map(|entry| {
            entry
                .expect("fixture entry")
                .path()
                .strip_prefix(root)
                .expect("relative fixture entry")
                .to_path_buf()
        })
        .collect::<Vec<_>>();
    values.sort();
    values
}
