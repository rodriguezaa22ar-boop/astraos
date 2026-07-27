use crate::{
    facts::{
        DocumentationFact, Fact, FactGraphBuilder, FactProvenance, FileFact, FileRole, MarkerFact,
        MarkerKind, ToolCategory, ToolFact,
    },
    policy::{excluded_path, normalized_relative, sensitive_path},
    scanner::metadata,
    scope::{classify_project_path, SemanticScope},
    Confidence, Diagnostic, DiagnosticSeverity, DocumentKind, Evidence, EvidenceSource,
    ProjectPath, ScanOptions, ScannerMetadata, ScannerResult, ScannerStatus,
};
use ignore::WalkBuilder;
use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub(crate) struct InventoryFile {
    pub(crate) relative: String,
    pub(crate) absolute: PathBuf,
    pub(crate) bytes: u64,
    pub(crate) role: FileRole,
    pub(crate) extension: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) scope: SemanticScope,
}

#[derive(Debug)]
pub(crate) struct Inventory {
    root: PathBuf,
    files: Vec<InventoryFile>,
    observations: Vec<(Fact, FactProvenance)>,
    incomplete: bool,
    truncated: bool,
}

#[derive(Debug)]
pub(crate) struct InventoryOutput {
    pub(crate) inventory: Inventory,
    pub(crate) result: ScannerResult,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl Inventory {
    pub(crate) fn files(&self) -> &[InventoryFile] {
        &self.files
    }

    pub(crate) fn incomplete(&self) -> bool {
        self.incomplete
    }

    pub(crate) fn ingest(self, builder: &mut FactGraphBuilder) {
        builder.add_fact(
            Fact::ProjectRoot(self.root.to_string_lossy().into_owned()),
            FactProvenance {
                scanner: "inventory".to_string(),
                scope: SemanticScope::Primary,
                confidence: Confidence::Certain,
                evidence: vec![Evidence {
                    source: EvidenceSource::Convention,
                    path: None,
                    locator: None,
                    rule: "inventory.selected_root".to_string(),
                }],
            },
        );
        for file in &self.files {
            builder.add_fact(
                Fact::File(FileFact {
                    path: file.relative.clone(),
                    bytes: file.bytes,
                    role: file.role,
                    extension: file.extension.clone(),
                    language: file.language.clone(),
                }),
                provenance(
                    "inventory",
                    file.scope,
                    Confidence::Certain,
                    EvidenceSource::File,
                    &file.relative,
                    "inventory.file",
                ),
            );
        }
        for (fact, provenance) in self.observations {
            builder.add_fact(fact, provenance);
        }
        let (kind, rule, confidence) = if self.truncated {
            (
                MarkerKind::InventoryTruncated,
                "inventory.scan_truncated",
                Confidence::Low,
            )
        } else if self.incomplete {
            (
                MarkerKind::InventoryPartial,
                "inventory.scan_partial",
                Confidence::Low,
            )
        } else {
            (
                MarkerKind::InventoryComplete,
                "inventory.scan_complete",
                Confidence::Certain,
            )
        };
        builder.add_fact(
            Fact::Marker(MarkerFact {
                kind,
                id: "inventory".to_string(),
                path: String::new(),
                detail: None,
            }),
            FactProvenance {
                scanner: "inventory".to_string(),
                scope: SemanticScope::Primary,
                confidence,
                evidence: vec![Evidence {
                    source: EvidenceSource::Convention,
                    path: None,
                    locator: None,
                    rule: rule.to_string(),
                }],
            },
        );
    }
}

pub(crate) fn scan(root: &Path, options: &ScanOptions) -> InventoryOutput {
    let metadata = scanner_metadata();
    let mut diagnostics = Vec::new();
    let mut files = Vec::new();
    let mut observations = Vec::new();
    let mut incomplete = false;
    let mut truncated = false;
    let mut depth_limited = false;
    let mut entry_limited = false;
    let mut file_limited = false;
    let mut entries_seen = 0_usize;

    let mut walker = WalkBuilder::new(root);
    walker
        .hidden(false)
        .parents(false)
        .git_global(false)
        .git_exclude(false)
        .require_git(false)
        .follow_links(false)
        .max_depth(Some(options.max_depth))
        .sort_by_file_path(|left, right| left.cmp(right));
    let filter_root = root.to_path_buf();
    walker.filter_entry(move |entry| {
        if entry.depth() == 0 {
            return true;
        }
        entry
            .path()
            .strip_prefix(&filter_root)
            .is_ok_and(|relative| !excluded_path(relative) && !sensitive_path(relative))
    });

    for result in walker.build() {
        let counts_toward_limit = !matches!(&result, Ok(entry) if entry.depth() == 0);
        if counts_toward_limit {
            if entries_seen >= options.max_entries {
                incomplete = true;
                truncated = true;
                entry_limited = true;
                break;
            }
            entries_seen += 1;
        }
        let entry = match result {
            Ok(entry) => entry,
            Err(error) => {
                incomplete = true;
                diagnostics.push(Diagnostic::new(
                    "inventory",
                    "inventory.read_failed",
                    DiagnosticSeverity::Warning,
                    None,
                    bounded_message(&error.to_string()),
                ));
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }
        let path = entry.path();
        let relative_native = match path.strip_prefix(root) {
            Ok(relative) => relative,
            Err(_) => {
                incomplete = true;
                continue;
            }
        };
        if excluded_path(relative_native) || sensitive_path(relative_native) {
            continue;
        }
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if entry.depth() >= options.max_depth {
                incomplete = true;
                truncated = true;
                depth_limited = true;
            }
            continue;
        }
        if file_type.is_symlink() {
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if files.len() >= options.max_files {
            incomplete = true;
            truncated = true;
            file_limited = true;
            break;
        }
        let Some(relative) = normalized_relative(relative_native) else {
            diagnostics.push(Diagnostic::new(
                "inventory",
                "inventory.non_utf8_path",
                DiagnosticSeverity::Warning,
                None,
                "a non-UTF-8 path was omitted",
            ));
            incomplete = true;
            continue;
        };
        let file_metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                incomplete = true;
                diagnostics.push(Diagnostic::new(
                    "inventory",
                    "inventory.metadata_failed",
                    DiagnosticSeverity::Warning,
                    Some(ProjectPath(relative)),
                    bounded_message(&error.to_string()),
                ));
                continue;
            }
        };
        let role = classify_role(&relative);
        let extension = Path::new(&relative)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase);
        let language = extension
            .as_deref()
            .and_then(language_for_extension)
            .map(str::to_string);
        let file = InventoryFile {
            relative: relative.clone(),
            absolute: path.to_path_buf(),
            bytes: file_metadata.len(),
            role,
            extension,
            language,
            scope: classify_project_path(&relative),
        };

        add_file_observations(&file, options, &mut observations, &mut diagnostics);
        files.push(file);
    }

    if entry_limited {
        diagnostics.push(Diagnostic::new(
            "inventory",
            "inventory.entry_limit",
            DiagnosticSeverity::Warning,
            None,
            format!(
                "scan stopped after inspecting {} filesystem entries",
                options.max_entries
            ),
        ));
    }
    if file_limited {
        diagnostics.push(Diagnostic::new(
            "inventory",
            "inventory.file_limit",
            DiagnosticSeverity::Warning,
            None,
            format!("scan stopped after {} files", options.max_files),
        ));
    }
    if depth_limited {
        diagnostics.push(Diagnostic::new(
            "inventory",
            "inventory.depth_limit",
            DiagnosticSeverity::Warning,
            None,
            format!("scan did not descend beyond depth {}", options.max_depth),
        ));
    }

    let status = if diagnostics.is_empty() {
        ScannerStatus::Complete
    } else {
        ScannerStatus::Partial
    };
    let mut diagnostic_codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.clone())
        .collect::<Vec<_>>();
    diagnostic_codes.sort();
    diagnostic_codes.dedup();
    let findings = files.len() + observations.len();

    InventoryOutput {
        inventory: Inventory {
            root: root.to_path_buf(),
            files,
            observations,
            incomplete,
            truncated,
        },
        result: ScannerResult {
            metadata,
            status,
            findings,
            diagnostic_codes,
        },
        diagnostics,
    }
}

fn add_file_observations(
    file: &InventoryFile,
    options: &ScanOptions,
    observations: &mut Vec<(Fact, FactProvenance)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(kind) = document_kind(&file.relative) {
        let (title, headings) = read_headings(file, options.max_file_read_bytes, diagnostics);
        observations.push((
            Fact::Documentation(DocumentationFact {
                path: file.relative.clone(),
                kind,
                title,
                headings,
                bytes: file.bytes,
            }),
            provenance(
                "inventory",
                file.scope,
                Confidence::Certain,
                EvidenceSource::File,
                &file.relative,
                "documentation.known_path",
            ),
        ));
    }

    if let Some((provider, rule)) = ci_marker(&file.relative) {
        observations.push(marker_observation(
            file,
            MarkerKind::Ci,
            provider,
            None,
            rule,
        ));
    }
    if let Some((tool, rule)) = configuration_marker(&file.relative) {
        observations.push(marker_observation(
            file,
            MarkerKind::Configuration,
            tool,
            None,
            rule,
        ));
    }
    if let Some((kind, language, rule)) = entry_point_marker(&file.relative) {
        observations.push(marker_observation(
            file,
            MarkerKind::EntryPoint,
            kind,
            language.map(str::to_string),
            rule,
        ));
    }
    if is_license_file(&file.relative) {
        observations.push(marker_observation(
            file,
            MarkerKind::LicenseFile,
            "license",
            None,
            "license.filename",
        ));
    }
    if let Some((id, category, rule)) = tool_marker(&file.relative) {
        observations.push((
            Fact::Tool(ToolFact {
                id: id.to_string(),
                category,
                source_path: file.relative.clone(),
            }),
            provenance(
                "inventory",
                file.scope,
                Confidence::High,
                EvidenceSource::File,
                &file.relative,
                rule,
            ),
        ));
    }
}

fn marker_observation(
    file: &InventoryFile,
    kind: MarkerKind,
    id: &str,
    detail: Option<String>,
    rule: &str,
) -> (Fact, FactProvenance) {
    (
        Fact::Marker(MarkerFact {
            kind,
            id: id.to_string(),
            path: file.relative.clone(),
            detail,
        }),
        provenance(
            "inventory",
            file.scope,
            Confidence::High,
            EvidenceSource::Convention,
            &file.relative,
            rule,
        ),
    )
}

fn read_headings(
    file: &InventoryFile,
    max_bytes: u64,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Option<String>, Vec<String>) {
    let mut source = match File::open(&file.absolute) {
        Ok(source) => source,
        Err(error) => {
            diagnostics.push(file_read_diagnostic(file, &error));
            return (None, Vec::new());
        }
    };
    let mut bytes = Vec::new();
    if let Err(error) = source
        .by_ref()
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
    {
        diagnostics.push(file_read_diagnostic(file, &error));
        return (None, Vec::new());
    }
    if bytes.len() as u64 > max_bytes {
        bytes.truncate(max_bytes as usize);
        diagnostics.push(Diagnostic::new(
            "inventory",
            "documentation.read_limit",
            DiagnosticSeverity::Info,
            Some(ProjectPath(file.relative.clone())),
            "documentation headings were read from a truncated prefix",
        ));
    }
    let text = String::from_utf8_lossy(&bytes);
    let headings = text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let content = trimmed.strip_prefix('#')?.trim_start_matches('#').trim();
            (!content.is_empty()).then(|| content.to_string())
        })
        .take(20)
        .collect::<Vec<_>>();
    (headings.first().cloned(), headings)
}

fn file_read_diagnostic(file: &InventoryFile, error: &io::Error) -> Diagnostic {
    let (code, message) = match error.kind() {
        io::ErrorKind::NotFound => (
            "documentation.missing",
            "documentation disappeared after inventory".to_string(),
        ),
        io::ErrorKind::PermissionDenied => (
            "documentation.permission_denied",
            "permission was denied while reading documentation".to_string(),
        ),
        _ => (
            "documentation.read_failed",
            bounded_message(&error.to_string()),
        ),
    };
    Diagnostic::new(
        "inventory",
        code,
        DiagnosticSeverity::Warning,
        Some(ProjectPath(file.relative.clone())),
        message,
    )
}

fn classify_role(path: &str) -> FileRole {
    let lower = path.to_ascii_lowercase();
    let filename = lower.rsplit('/').next().unwrap_or(&lower);
    if lower.split('/').any(is_test_component)
        || filename.contains(".test.")
        || filename.contains(".spec.")
        || filename.ends_with("_test.go")
        || filename.starts_with("test_")
    {
        FileRole::Test
    } else if document_kind(path).is_some() {
        FileRole::Documentation
    } else if configuration_marker(path).is_some() {
        FileRole::Configuration
    } else if language_for_extension(
        Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default(),
    )
    .is_some()
    {
        FileRole::Source
    } else {
        FileRole::Other
    }
}

fn is_test_component(component: &str) -> bool {
    matches!(component, "test" | "tests" | "__tests__")
}

pub(crate) fn language_for_extension(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "c" => Some("c"),
        "cc" | "cpp" | "cxx" | "h" | "hh" | "hpp" | "hxx" => Some("cpp"),
        "go" => Some("go"),
        "java" => Some("java"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "kt" | "kts" => Some("kotlin"),
        "nix" => Some("nix"),
        "py" | "pyi" => Some("python"),
        "rs" => Some("rust"),
        "swift" => Some("swift"),
        "ts" | "tsx" | "mts" | "cts" => Some("typescript"),
        _ => None,
    }
}

fn document_kind(path: &str) -> Option<DocumentKind> {
    let lower = path.to_ascii_lowercase();
    let filename = lower.rsplit('/').next()?;
    if filename.starts_with("readme") {
        Some(DocumentKind::Readme)
    } else if filename == "architecture.md" {
        Some(DocumentKind::Architecture)
    } else if lower.contains("/adr/") || lower.starts_with("adr/") {
        Some(DocumentKind::Adr)
    } else if lower.contains("/milestones/") || lower.starts_with("milestones/") {
        Some(DocumentKind::Milestone)
    } else if filename.starts_with("contributing") {
        Some(DocumentKind::Contributing)
    } else if filename.starts_with("changelog") {
        Some(DocumentKind::Changelog)
    } else if lower.starts_with("docs/") && filename.ends_with(".md") {
        Some(DocumentKind::Other)
    } else {
        None
    }
}

fn ci_marker(path: &str) -> Option<(&'static str, &'static str)> {
    let lower = path.to_ascii_lowercase();
    if lower.starts_with(".github/workflows/")
        && matches!(
            Path::new(&lower)
                .extension()
                .and_then(|value| value.to_str()),
            Some("yml" | "yaml")
        )
    {
        Some(("github-actions", "ci.github_workflow"))
    } else if lower == ".gitlab-ci.yml" {
        Some(("gitlab-ci", "ci.gitlab"))
    } else if lower == "azure-pipelines.yml" {
        Some(("azure-pipelines", "ci.azure"))
    } else if lower == ".circleci/config.yml" {
        Some(("circleci", "ci.circleci"))
    } else {
        None
    }
}

fn configuration_marker(path: &str) -> Option<(&'static str, &'static str)> {
    let lower = path.to_ascii_lowercase();
    let filename = lower.rsplit('/').next()?;
    match filename {
        "cargo.toml" => Some(("cargo", "configuration.cargo")),
        "clippy.toml" | ".clippy.toml" => Some(("clippy", "configuration.clippy")),
        "cmakelists.txt" => Some(("cmake", "configuration.cmake")),
        "flake.nix" => Some(("nix", "configuration.nix")),
        "go.mod" => Some(("go", "configuration.go")),
        "makefile" => Some(("make", "configuration.make")),
        "package.json" => Some(("node", "configuration.node")),
        "pyproject.toml" => Some(("python", "configuration.python")),
        "rustfmt.toml" | ".rustfmt.toml" => Some(("rustfmt", "configuration.rustfmt")),
        "tsconfig.json" => Some(("typescript", "configuration.typescript")),
        "vitest.config.js" | "vitest.config.ts" => Some(("vitest", "configuration.vitest")),
        _ if filename.starts_with("eslint.config.") => Some(("eslint", "configuration.eslint")),
        _ if filename.starts_with(".prettier") => Some(("prettier", "configuration.prettier")),
        _ => None,
    }
}

fn entry_point_marker(path: &str) -> Option<(&'static str, Option<&'static str>, &'static str)> {
    let lower = path.to_ascii_lowercase();
    let filename = lower.rsplit('/').next()?;
    if lower.ends_with("src/main.rs") {
        Some(("binary", Some("rust"), "entry.rust_main"))
    } else if lower.ends_with("src/lib.rs") {
        Some(("library", Some("rust"), "entry.rust_lib"))
    } else if filename == "__main__.py" || filename == "main.py" {
        Some(("application", Some("python"), "entry.python_main"))
    } else if (filename == "index.ts" || filename == "index.js") && lower.contains("/src/") {
        Some(("application", Some("typescript"), "entry.node_index"))
    } else if filename == "main.go" && (lower.starts_with("cmd/") || lower.contains("/cmd/")) {
        Some(("binary", Some("go"), "entry.go_main"))
    } else {
        None
    }
}

fn is_license_file(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .map(str::to_ascii_lowercase)
        .is_some_and(|name| name.starts_with("license") || name.starts_with("copying"))
}

fn tool_marker(path: &str) -> Option<(&'static str, ToolCategory, &'static str)> {
    let filename = path.rsplit('/').next()?.to_ascii_lowercase();
    match filename.as_str() {
        "cargo.lock" => Some(("cargo", ToolCategory::PackageManager, "tool.cargo_lock")),
        "package-lock.json" => Some(("npm", ToolCategory::PackageManager, "tool.npm_lock")),
        "pnpm-lock.yaml" => Some(("pnpm", ToolCategory::PackageManager, "tool.pnpm_lock")),
        "yarn.lock" => Some(("yarn", ToolCategory::PackageManager, "tool.yarn_lock")),
        "bun.lock" | "bun.lockb" => Some(("bun", ToolCategory::PackageManager, "tool.bun_lock")),
        "uv.lock" => Some(("uv", ToolCategory::PackageManager, "tool.uv_lock")),
        "poetry.lock" => Some(("poetry", ToolCategory::PackageManager, "tool.poetry_lock")),
        "cmakelists.txt" => Some(("cmake", ToolCategory::BuildSystem, "tool.cmake")),
        "makefile" => Some(("make", ToolCategory::BuildSystem, "tool.make")),
        "flake.nix" => Some(("nix", ToolCategory::BuildSystem, "tool.nix")),
        _ => None,
    }
}

fn provenance(
    scanner: &str,
    scope: SemanticScope,
    confidence: Confidence,
    source: EvidenceSource,
    path: &str,
    rule: &str,
) -> FactProvenance {
    FactProvenance {
        scanner: scanner.to_string(),
        scope,
        confidence,
        evidence: vec![Evidence {
            source,
            path: Some(ProjectPath(path.to_string())),
            locator: None,
            rule: rule.to_string(),
        }],
    }
}

fn scanner_metadata() -> ScannerMetadata {
    metadata(
        "inventory",
        1,
        "Builds the bounded, deterministic project file inventory",
    )
}

fn bounded_message(message: &str) -> String {
    const MAX_CHARS: usize = 512;
    let mut value = message.chars().take(MAX_CHARS).collect::<String>();
    if message.chars().count() > MAX_CHARS {
        value.push('…');
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn inventory_is_sorted_and_excludes_sensitive_generated_content() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::create_dir_all(directory.path().join("src")).expect("src");
        fs::create_dir_all(directory.path().join("node_modules/pkg")).expect("generated");
        fs::write(directory.path().join("src/z.rs"), "fn z() {}").expect("z");
        fs::write(directory.path().join("src/a.rs"), "fn a() {}").expect("a");
        fs::write(directory.path().join(".env"), "TOKEN=test").expect("env");
        fs::write(
            directory.path().join("node_modules/pkg/index.js"),
            "ignored",
        )
        .expect("ignored");

        let output = scan(directory.path(), &ScanOptions::default());
        let paths = output
            .inventory
            .files()
            .iter()
            .map(|file| file.relative.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, vec!["src/a.rs", "src/z.rs"]);
    }

    #[test]
    fn inventory_reports_deterministic_file_limit() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::write(directory.path().join("b.rs"), "").expect("b");
        fs::write(directory.path().join("a.rs"), "").expect("a");
        let options = ScanOptions {
            max_files: 1,
            ..ScanOptions::default()
        };

        let output = scan(directory.path(), &options);
        assert_eq!(output.inventory.files()[0].relative, "a.rs");
        assert!(output.inventory.incomplete());
        assert!(output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "inventory.file_limit"));
    }

    #[test]
    fn inventory_reports_deterministic_entry_limit() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::create_dir_all(directory.path().join("a/b/c")).expect("nested directories");
        fs::write(directory.path().join("a/b/c/main.rs"), "fn main() {}").expect("source");
        let options = ScanOptions {
            max_entries: 2,
            ..ScanOptions::default()
        };

        let output = scan(directory.path(), &options);
        assert!(output.inventory.incomplete());
        assert!(output.inventory.truncated);
        assert!(output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "inventory.entry_limit"));
    }

    #[test]
    fn inventory_honors_project_ignore_rules_without_global_state() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::write(directory.path().join(".gitignore"), "ignored.rs\n").expect("ignore file");
        fs::write(directory.path().join("ignored.rs"), "fn ignored() {}").expect("ignored");
        fs::write(directory.path().join("visible.rs"), "fn visible() {}").expect("visible");

        let output = scan(directory.path(), &ScanOptions::default());
        let paths = output
            .inventory
            .files()
            .iter()
            .map(|file| file.relative.as_str())
            .collect::<Vec<_>>();
        assert!(paths.contains(&".gitignore"));
        assert!(paths.contains(&"visible.rs"));
        assert!(!paths.contains(&"ignored.rs"));
    }

    #[test]
    fn inventory_retains_fixture_manifests_with_non_primary_scope() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::create_dir_all(directory.path().join("tests/fixtures/demo"))
            .expect("fixture directory");
        fs::write(
            directory.path().join("tests/fixtures/demo/Cargo.toml"),
            "[package]\nname = \"embedded-fixture\"\nversion = \"0.1.0\"\n",
        )
        .expect("fixture manifest");

        let output = scan(directory.path(), &ScanOptions::default());
        let manifest = output
            .inventory
            .files()
            .iter()
            .find(|file| file.relative == "tests/fixtures/demo/Cargo.toml")
            .expect("inventoried fixture manifest");

        assert_eq!(manifest.scope, SemanticScope::Fixture);
    }

    #[test]
    fn documentation_permission_errors_have_a_stable_code() {
        let file = InventoryFile {
            relative: "README.md".to_string(),
            absolute: PathBuf::from("README.md"),
            bytes: 1,
            role: FileRole::Documentation,
            extension: Some("md".to_string()),
            language: None,
            scope: SemanticScope::Primary,
        };
        let diagnostic = file_read_diagnostic(
            &file,
            &io::Error::new(io::ErrorKind::PermissionDenied, "injected"),
        );
        assert_eq!(diagnostic.code, "documentation.permission_denied");
        assert!(!diagnostic.message.contains("injected"));
    }

    #[test]
    fn depth_limit_is_reported_as_truncation() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::create_dir_all(directory.path().join("one/two")).expect("nested directories");
        fs::write(directory.path().join("one/two/main.rs"), "fn main() {}").expect("nested source");
        let options = ScanOptions {
            max_depth: 1,
            ..ScanOptions::default()
        };

        let output = scan(directory.path(), &options);
        assert!(output.inventory.truncated);
        assert!(output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "inventory.depth_limit"));
        assert!(output.inventory.files().is_empty());
    }

    #[test]
    fn language_extensions_cover_the_supported_baseline() {
        for (extension, language) in [
            ("rs", "rust"),
            ("py", "python"),
            ("js", "javascript"),
            ("ts", "typescript"),
            ("go", "go"),
            ("java", "java"),
            ("swift", "swift"),
            ("c", "c"),
            ("cpp", "cpp"),
            ("kt", "kotlin"),
            ("nix", "nix"),
        ] {
            assert_eq!(language_for_extension(extension), Some(language));
        }
    }
}
