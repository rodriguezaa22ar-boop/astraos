use crate::{
    facts::FactGraphBuilder,
    insights::InsightsEngine,
    inventory, manifests,
    process::{CommandInvocation, ProcessOutput, ProcessRunner, SystemProcessRunner},
    projection,
    scanner::{ScannerInput, ScannerOutput},
    scanners, ContextError, ProjectContext, ScanOptions, ScanReport, ToolingSummary,
    PROJECT_CONTEXT_SCHEMA_VERSION,
};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Instant,
};

pub struct ProjectAnalyzer {
    options: ScanOptions,
    runner: Box<dyn ProcessRunner>,
}

#[derive(Debug, Default)]
struct NoProcessRunner;

impl ProcessRunner for NoProcessRunner {
    fn run(
        &self,
        _invocation: &CommandInvocation,
        _timeout: std::time::Duration,
        _output_limit: usize,
    ) -> io::Result<ProcessOutput> {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "process execution disabled",
        ))
    }
}

impl ProjectAnalyzer {
    pub fn new(options: ScanOptions) -> Result<Self, ContextError> {
        options.validate()?;
        Ok(Self {
            options,
            runner: Box::<SystemProcessRunner>::default(),
        })
    }

    /// Creates an analyzer whose repository-process boundary is disabled.
    ///
    /// Filesystem and manifest discovery remain enabled, while Git inspection
    /// degrades to the existing unavailable state without starting a process.
    pub fn without_processes(options: ScanOptions) -> Result<Self, ContextError> {
        options.validate()?;
        Ok(Self {
            options,
            runner: Box::<NoProcessRunner>::default(),
        })
    }

    pub fn analyze(&self, root: impl AsRef<Path>) -> Result<ScanReport, ContextError> {
        self.options.validate()?;
        let root = validated_root(root.as_ref())?;
        let started = Instant::now();

        // Phase 1: bounded filesystem inventory.
        let inventory_output = inventory::scan(&root, &self.options);

        // Phase 2: parse each recognized manifest once into the catalog.
        let manifest_output = manifests::scan(&inventory_output.inventory, &self.options);

        // Phase 3: normalize all raw observations, then freeze the graph.
        let mut builder = FactGraphBuilder::new();
        inventory_output.inventory.ingest(&mut builder);
        manifest_output.catalog.ingest(&mut builder);
        let git_output =
            scanners::git::scan(&root, &self.options, self.runner.as_ref(), &mut builder);
        let facts = builder.finish()?;

        // Phase 4: all semantic projections receive only immutable facts.
        let input = ScannerInput::new(&facts);
        let identity = projection::identity(&input);
        let repository = projection::repository(&input);
        let languages = scanners::languages::scan(&input);
        let workspace = scanners::workspace::scan(&input);
        let dependencies = scanners::dependencies::scan(&input);
        let documentation = scanners::documentation::scan(&input);
        let ci = scanners::ci::scan(&input);
        let configuration = scanners::configuration::scan(&input);
        let validation = scanners::validation::scan(&input);
        let build = scanners::build::scan(&input);
        let testing = scanners::testing::scan(&input);
        let entry_points = scanners::entry_points::scan(&input);
        let license = scanners::license::scan(&input);
        let size = projection::size(&input);

        let ScannerOutput {
            value: languages,
            result: languages_result,
            diagnostics: languages_diagnostics,
        } = languages;
        let ScannerOutput {
            value: workspace,
            result: workspace_result,
            diagnostics: workspace_diagnostics,
        } = workspace;
        let ScannerOutput {
            value: dependencies,
            result: dependencies_result,
            diagnostics: dependencies_diagnostics,
        } = dependencies;
        let ScannerOutput {
            value: documentation,
            result: documentation_result,
            diagnostics: documentation_diagnostics,
        } = documentation;
        let ScannerOutput {
            value: ci,
            result: ci_result,
            diagnostics: ci_diagnostics,
        } = ci;
        let ScannerOutput {
            value: configuration,
            result: configuration_result,
            diagnostics: configuration_diagnostics,
        } = configuration;
        let ScannerOutput {
            value: validation,
            result: validation_result,
            diagnostics: validation_diagnostics,
        } = validation;
        let ScannerOutput {
            value: build,
            result: build_result,
            diagnostics: build_diagnostics,
        } = build;
        let ScannerOutput {
            value: testing,
            result: testing_result,
            diagnostics: testing_diagnostics,
        } = testing;
        let ScannerOutput {
            value: entry_points,
            result: entry_points_result,
            diagnostics: entry_points_diagnostics,
        } = entry_points;
        let ScannerOutput {
            value: license,
            result: license_result,
            diagnostics: license_diagnostics,
        } = license;
        let context = ProjectContext {
            identity,
            repository,
            languages,
            workspace: workspace.summary,
            tooling: ToolingSummary {
                package_managers: workspace.package_managers,
                build_systems: build,
                testing_frameworks: testing,
            },
            dependencies,
            documentation,
            ci,
            configuration,
            entry_points,
            development_commands: validation.development,
            validation_commands: validation.validation,
            size,
            license,
        };

        // Phase 5: pure, factual derivation over completed context and facts.
        let insights = InsightsEngine::derive(&context, &facts);

        let mut scanners = vec![
            inventory_output.result,
            manifest_output.result,
            git_output.result,
            languages_result,
            workspace_result,
            dependencies_result,
            documentation_result,
            ci_result,
            configuration_result,
            validation_result,
            build_result,
            testing_result,
            entry_points_result,
            license_result,
        ];
        scanners.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));

        let mut diagnostics = Vec::new();
        diagnostics.extend(inventory_output.diagnostics);
        diagnostics.extend(manifest_output.diagnostics);
        diagnostics.extend(git_output.diagnostics);
        diagnostics.extend(languages_diagnostics);
        diagnostics.extend(workspace_diagnostics);
        diagnostics.extend(dependencies_diagnostics);
        diagnostics.extend(documentation_diagnostics);
        diagnostics.extend(ci_diagnostics);
        diagnostics.extend(configuration_diagnostics);
        diagnostics.extend(validation_diagnostics);
        diagnostics.extend(build_diagnostics);
        diagnostics.extend(testing_diagnostics);
        diagnostics.extend(entry_points_diagnostics);
        diagnostics.extend(license_diagnostics);
        diagnostics.sort_by(|left, right| {
            left.scanner
                .cmp(&right.scanner)
                .then_with(|| left.code.cmp(&right.code))
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| left.message.cmp(&right.message))
        });

        Ok(ScanReport {
            schema_version: PROJECT_CONTEXT_SCHEMA_VERSION,
            context,
            scanners,
            diagnostics,
            insights,
            duration: started.elapsed(),
        })
    }

    #[cfg(test)]
    pub(crate) fn with_runner(
        options: ScanOptions,
        runner: Box<dyn ProcessRunner>,
    ) -> Result<Self, ContextError> {
        options.validate()?;
        Ok(Self { options, runner })
    }
}

impl Default for ProjectAnalyzer {
    fn default() -> Self {
        Self {
            options: ScanOptions::default(),
            runner: Box::<SystemProcessRunner>::default(),
        }
    }
}

pub fn analyze(root: impl AsRef<Path>) -> Result<ScanReport, ContextError> {
    ProjectAnalyzer::default().analyze(root)
}

fn validated_root(root: &Path) -> Result<PathBuf, ContextError> {
    let metadata = fs::metadata(root).map_err(|source| root_io_error(root, source))?;
    if !metadata.is_dir() {
        return Err(ContextError::RootNotDirectory(root.to_path_buf()));
    }
    let canonical = root
        .canonicalize()
        .map_err(|source| ContextError::RootCanonicalization {
            path: root.to_path_buf(),
            source,
        })?;
    fs::read_dir(&canonical).map_err(|source| root_io_error(&canonical, source))?;
    Ok(canonical)
}

fn root_io_error(path: &Path, source: io::Error) -> ContextError {
    match source.kind() {
        io::ErrorKind::NotFound => ContextError::RootNotFound(path.to_path_buf()),
        io::ErrorKind::PermissionDenied => ContextError::RootPermissionDenied {
            path: path.to_path_buf(),
            source,
        },
        _ => ContextError::RootRead {
            path: path.to_path_buf(),
            source,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::FakeProcessRunner;
    use std::fs;

    #[test]
    fn missing_root_is_a_typed_error() {
        let directory = tempfile::tempdir().expect("temp directory");
        let missing = directory.path().join("missing");
        assert!(matches!(
            analyze(&missing),
            Err(ContextError::RootNotFound(path)) if path == missing
        ));
    }

    #[test]
    fn repeated_analysis_has_deterministic_serialized_output() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::create_dir(directory.path().join("src")).expect("src");
        fs::write(
            directory.path().join("Cargo.toml"),
            "[package]\nname = \"sample\"\nversion = \"0.1.0\"\n",
        )
        .expect("manifest");
        fs::write(directory.path().join("src/main.rs"), "fn main() {}\n").expect("source");

        let analyzer = ProjectAnalyzer::with_runner(
            ScanOptions::default(),
            Box::new(FakeProcessRunner::default()),
        )
        .expect("analyzer");
        let first = analyzer.analyze(directory.path()).expect("first report");
        let second = analyzer.analyze(directory.path()).expect("second report");
        assert_eq!(
            crate::render_json(&first).expect("first JSON"),
            crate::render_json(&second).expect("second JSON")
        );
    }

    #[test]
    fn analyzer_accepts_an_injected_process_boundary() {
        let directory = tempfile::tempdir().expect("temp directory");
        let runner = FakeProcessRunner::default();
        let analyzer = ProjectAnalyzer::with_runner(ScanOptions::default(), Box::new(runner))
            .expect("analyzer");
        let report = analyzer.analyze(directory.path()).expect("report");
        assert_eq!(report.schema_version, PROJECT_CONTEXT_SCHEMA_VERSION);
    }

    #[test]
    fn no_process_analyzer_marks_git_unavailable_without_running_it() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::create_dir(directory.path().join(".git")).expect("git marker");
        let analyzer =
            ProjectAnalyzer::without_processes(ScanOptions::default()).expect("analyzer");
        let report = analyzer.analyze(directory.path()).expect("report");
        let git = report
            .scanners
            .iter()
            .find(|scanner| scanner.metadata.id == "git")
            .expect("git scanner result");

        assert_eq!(git.status, crate::ScannerStatus::Unavailable);
    }

    #[test]
    fn invalid_options_are_rejected_before_scanning() {
        let invalid = [
            ScanOptions {
                max_entries: 0,
                ..ScanOptions::default()
            },
            ScanOptions {
                max_files: 0,
                ..ScanOptions::default()
            },
            ScanOptions {
                max_depth: 0,
                ..ScanOptions::default()
            },
            ScanOptions {
                max_file_read_bytes: 0,
                ..ScanOptions::default()
            },
            ScanOptions {
                max_git_output_bytes: 0,
                ..ScanOptions::default()
            },
            ScanOptions {
                git_timeout: std::time::Duration::ZERO,
                ..ScanOptions::default()
            },
        ];
        for options in invalid {
            assert!(matches!(
                ProjectAnalyzer::new(options),
                Err(ContextError::InvalidOptions(_))
            ));
        }
    }

    #[test]
    fn permission_errors_keep_their_typed_identity() {
        let path = Path::new("/private/project");
        let error = root_io_error(
            path,
            io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
        );
        assert!(matches!(
            error,
            ContextError::RootPermissionDenied { path: denied, .. } if denied == path
        ));
    }
}
