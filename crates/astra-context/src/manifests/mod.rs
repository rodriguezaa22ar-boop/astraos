mod cargo;
mod go;
mod jvm;
mod node;
mod python;
mod swift;

use crate::{
    facts::{
        CommandFact, DependencyFact, Fact, FactGraphBuilder, FactKey, FactProvenance, ManifestFact,
        MarkerFact, MarkerKind, PackageFact, RelationKind, ToolCategory, ToolFact, WorkspaceFact,
    },
    inventory::{Inventory, InventoryFile},
    scanner::metadata,
    scope::{classify_project_path, SemanticScope},
    CommandPurpose, Confidence, DependencyScope, Diagnostic, DiagnosticSeverity, Evidence,
    EvidenceSource, ProjectPath, ScanOptions, ScannerResult, ScannerStatus,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, Read},
    path::Path,
};

const SCANNER_ID: &str = "manifests";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestFormat {
    Cargo,
    NodePackage,
    PnpmWorkspace,
    PythonProject,
    GoModule,
    GoWorkspace,
    Maven,
    GradleBuild,
    GradleSettings,
    SwiftPackage,
}

impl ManifestFormat {
    fn classify(path: &str) -> Option<Self> {
        match path.rsplit('/').next()? {
            "Cargo.toml" => Some(Self::Cargo),
            "package.json" => Some(Self::NodePackage),
            "pnpm-workspace.yaml" | "pnpm-workspace.yml" => Some(Self::PnpmWorkspace),
            "pyproject.toml" => Some(Self::PythonProject),
            "go.mod" => Some(Self::GoModule),
            "go.work" => Some(Self::GoWorkspace),
            "pom.xml" => Some(Self::Maven),
            "build.gradle" | "build.gradle.kts" => Some(Self::GradleBuild),
            "settings.gradle" | "settings.gradle.kts" => Some(Self::GradleSettings),
            "Package.swift" => Some(Self::SwiftPackage),
            _ => None,
        }
    }

    fn ecosystem(self) -> &'static str {
        match self {
            Self::Cargo => "rust",
            Self::NodePackage | Self::PnpmWorkspace => "node",
            Self::PythonProject => "python",
            Self::GoModule | Self::GoWorkspace => "go",
            Self::Maven | Self::GradleBuild | Self::GradleSettings => "jvm",
            Self::SwiftPackage => "swift",
        }
    }

    fn kind(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::NodePackage => "package-json",
            Self::PnpmWorkspace => "pnpm-workspace",
            Self::PythonProject => "pyproject",
            Self::GoModule => "go-module",
            Self::GoWorkspace => "go-workspace",
            Self::Maven => "maven",
            Self::GradleBuild => "gradle",
            Self::GradleSettings => "gradle-settings",
            Self::SwiftPackage => "swift-package",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManifestRecord {
    pub(crate) fact: Fact,
    pub(crate) provenance: FactProvenance,
    source_manifest: String,
}

#[derive(Debug)]
pub(crate) struct ManifestCatalog {
    records: Box<[ManifestRecord]>,
}

#[derive(Debug)]
pub(crate) struct ManifestOutput {
    pub(crate) catalog: ManifestCatalog,
    pub(crate) result: ScannerResult,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl ManifestCatalog {
    #[cfg(test)]
    pub(crate) fn records(&self) -> &[ManifestRecord] {
        &self.records
    }

    pub(crate) fn ingest(self, builder: &mut FactGraphBuilder) {
        let records = self.records.into_vec();
        let keys = records
            .iter()
            .map(|record| builder.add_fact(record.fact.clone(), record.provenance.clone()))
            .collect::<Vec<_>>();

        let manifests = records
            .iter()
            .zip(&keys)
            .filter_map(|(record, key)| match &record.fact {
                Fact::Manifest(manifest) => Some((manifest.path.as_str(), key)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();

        for (index, record) in records.iter().enumerate() {
            if !matches!(record.fact, Fact::Manifest(_)) {
                if let Some(manifest) = manifests.get(record.source_manifest.as_str()) {
                    builder.add_relation(&keys[index], manifest, RelationKind::DeclaredBy);
                }
            }
        }

        add_semantic_relations(builder, &records, &keys);
    }
}

fn add_semantic_relations(
    builder: &mut FactGraphBuilder,
    records: &[ManifestRecord],
    keys: &[FactKey],
) {
    let packages = records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| match &record.fact {
            Fact::Package(package) => Some((index, package)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let workspaces = records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| match &record.fact {
            Fact::Workspace(workspace) => Some((index, workspace)),
            _ => None,
        })
        .collect::<Vec<_>>();

    for (package_index, package) in &packages {
        let workspace = workspaces
            .iter()
            .filter(|(_, workspace)| {
                workspace_ecosystem(&workspace.kind) == package.ecosystem
                    && workspace_includes(workspace, &package.root)
            })
            .max_by_key(|(_, workspace)| workspace.root.len());
        if let Some((workspace_index, _)) = workspace {
            builder.add_relation(
                &keys[*package_index],
                &keys[*workspace_index],
                RelationKind::MemberOf,
            );
        }

        for (dependency_index, dependency) in
            records
                .iter()
                .enumerate()
                .filter_map(|(index, record)| match &record.fact {
                    Fact::Dependency(dependency)
                        if dependency.package == package.name
                            && dependency.manifest == package.manifest =>
                    {
                        Some((index, dependency))
                    }
                    _ => None,
                })
        {
            let _ = dependency;
            builder.add_relation(
                &keys[*package_index],
                &keys[dependency_index],
                RelationKind::DependsOn,
            );
        }
    }

    for (index, record) in records.iter().enumerate() {
        let supports_package = matches!(
            record.fact,
            Fact::Command(_)
                | Fact::Tool(_)
                | Fact::Marker(MarkerFact {
                    kind: MarkerKind::DeclaredLicense,
                    ..
                })
        );
        let entry_point = matches!(
            record.fact,
            Fact::Marker(MarkerFact {
                kind: MarkerKind::EntryPoint,
                ..
            })
        );
        if !supports_package && !entry_point {
            continue;
        }
        if let Some((package_index, _)) = packages
            .iter()
            .find(|(_, package)| package.manifest == record.source_manifest)
        {
            builder.add_relation(
                &keys[index],
                &keys[*package_index],
                if entry_point {
                    RelationKind::EntrypointOf
                } else {
                    RelationKind::Supports
                },
            );
        }
    }
}

fn workspace_includes(workspace: &WorkspaceFact, package_root: &str) -> bool {
    if workspace.root == package_root {
        return true;
    }
    let mut included = false;
    for member in &workspace.members {
        let (excluded, member) = member
            .strip_prefix('!')
            .map_or((false, member.as_str()), |member| (true, member));
        let Some(pattern) = resolve_relative(&workspace.root, member) else {
            continue;
        };
        if wildcard_match(&pattern, package_root) {
            if excluded {
                return false;
            }
            included = true;
        }
    }
    included
}

fn workspace_ecosystem(kind: &str) -> &str {
    match kind {
        "cargo" => "rust",
        "node" | "pnpm" | "npm" | "yarn" | "bun" => "node",
        "uv" => "python",
        "go" => "go",
        "maven" | "gradle" => "jvm",
        "swiftpm" => "swift",
        _ => kind,
    }
}

fn path_contains(root: &str, path: &str) -> bool {
    root == "."
        || root == path
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

pub(crate) fn scan(inventory: &Inventory, options: &ScanOptions) -> ManifestOutput {
    scan_with_reader(inventory, options, &mut FileManifestReader)
}

fn scan_with_reader(
    inventory: &Inventory,
    options: &ScanOptions,
    reader: &mut dyn ManifestReader,
) -> ManifestOutput {
    let candidates = inventory
        .files()
        .iter()
        .filter_map(|file| ManifestFormat::classify(&file.relative).map(|format| (file, format)))
        .collect::<Vec<_>>();
    let inventory_paths = inventory
        .files()
        .iter()
        .map(|file| file.relative.clone())
        .collect::<BTreeSet<_>>();

    let mut records = Vec::new();
    let mut diagnostics = Vec::new();
    let mut workspaces = Vec::new();

    for (file, format) in &candidates {
        records.push(manifest_record(file, *format));
        if file.bytes > options.max_file_read_bytes {
            diagnostics.push(read_limit_diagnostic(&file.relative));
            continue;
        }

        let bounded = match reader.read_bounded(&file.absolute, options.max_file_read_bytes) {
            Ok(bounded) => bounded,
            Err(error) => {
                diagnostics.push(manifest_read_diagnostic(&file.relative, &error));
                continue;
            }
        };
        if bounded.truncated {
            diagnostics.push(read_limit_diagnostic(&file.relative));
            continue;
        }
        let text = match String::from_utf8(bounded.bytes) {
            Ok(text) => text,
            Err(_) => {
                diagnostics.push(Diagnostic::new(
                    SCANNER_ID,
                    "manifest.invalid_utf8",
                    DiagnosticSeverity::Warning,
                    Some(ProjectPath(file.relative.clone())),
                    "the manifest is not valid UTF-8",
                ));
                continue;
            }
        };

        let root = manifest_root(&file.relative);
        let context = ParseContext {
            path: &file.relative,
            root: &root,
            text: &text,
            inventory_paths: &inventory_paths,
            package_manager: inferred_manager(*format, &file.relative, inventory.files()),
        };
        let parsed = match format {
            ManifestFormat::Cargo => cargo::parse(&context),
            ManifestFormat::NodePackage | ManifestFormat::PnpmWorkspace => {
                node::parse(&context, *format == ManifestFormat::PnpmWorkspace)
            }
            ManifestFormat::PythonProject => python::parse(&context),
            ManifestFormat::GoModule | ManifestFormat::GoWorkspace => {
                go::parse(&context, *format == ManifestFormat::GoWorkspace)
            }
            ManifestFormat::Maven
            | ManifestFormat::GradleBuild
            | ManifestFormat::GradleSettings => jvm::parse(
                &context,
                matches!(format, ManifestFormat::Maven),
                matches!(format, ManifestFormat::GradleSettings),
            ),
            ManifestFormat::SwiftPackage => swift::parse(&context),
        };

        for fact in parsed.facts {
            records.push(ManifestRecord {
                fact: fact.fact,
                provenance: provenance(
                    file.scope,
                    fact.confidence,
                    &file.relative,
                    fact.locator,
                    fact.rule,
                ),
                source_manifest: file.relative.clone(),
            });
        }
        workspaces.extend(parsed.workspaces);
        diagnostics.extend(parsed.diagnostics.into_iter().map(|issue| {
            Diagnostic::new(
                SCANNER_ID,
                issue.code,
                DiagnosticSeverity::Warning,
                Some(ProjectPath(file.relative.clone())),
                issue.message,
            )
        }));
    }

    if !inventory.incomplete() {
        validate_workspace_members(&workspaces, &candidates, &mut records, &mut diagnostics);
    }

    sort_diagnostics(&mut diagnostics);

    let status = if candidates.is_empty() {
        ScannerStatus::NotApplicable
    } else if diagnostics.is_empty() {
        ScannerStatus::Complete
    } else {
        ScannerStatus::Partial
    };
    records.push(manifest_status_record(status));
    records.sort_by(|left, right| {
        (&left.fact, &left.provenance, &left.source_manifest).cmp(&(
            &right.fact,
            &right.provenance,
            &right.source_manifest,
        ))
    });
    records.dedup();
    let diagnostic_codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let findings = records.len();

    ManifestOutput {
        catalog: ManifestCatalog {
            records: records.into_boxed_slice(),
        },
        result: ScannerResult {
            metadata: metadata(
                SCANNER_ID,
                1,
                "Parses bounded project manifests into normalized facts",
            ),
            status,
            findings,
            diagnostic_codes,
        },
        diagnostics,
    }
}

fn manifest_status_record(status: ScannerStatus) -> ManifestRecord {
    let complete = matches!(
        status,
        ScannerStatus::Complete | ScannerStatus::NotApplicable
    );
    ManifestRecord {
        fact: Fact::Marker(MarkerFact {
            kind: if complete {
                MarkerKind::ManifestComplete
            } else {
                MarkerKind::ManifestPartial
            },
            id: "manifests".to_string(),
            path: String::new(),
            detail: None,
        }),
        provenance: FactProvenance {
            scanner: SCANNER_ID.to_string(),
            scope: SemanticScope::Primary,
            confidence: if complete {
                Confidence::Certain
            } else {
                Confidence::Low
            },
            evidence: vec![Evidence {
                source: EvidenceSource::Convention,
                path: None,
                locator: None,
                rule: if complete {
                    "manifest.scan_complete".to_string()
                } else {
                    "manifest.scan_partial".to_string()
                },
            }],
        },
        source_manifest: String::new(),
    }
}

fn manifest_record(file: &InventoryFile, format: ManifestFormat) -> ManifestRecord {
    ManifestRecord {
        fact: Fact::Manifest(ManifestFact {
            path: file.relative.clone(),
            ecosystem: format.ecosystem().to_string(),
            kind: format.kind().to_string(),
        }),
        provenance: provenance(
            file.scope,
            Confidence::Certain,
            &file.relative,
            None,
            "manifest.known_filename",
        ),
        source_manifest: file.relative.clone(),
    }
}

fn provenance(
    scope: SemanticScope,
    confidence: Confidence,
    path: &str,
    locator: Option<String>,
    rule: &str,
) -> FactProvenance {
    FactProvenance {
        scanner: SCANNER_ID.to_string(),
        scope,
        confidence,
        evidence: vec![Evidence {
            source: EvidenceSource::Manifest,
            path: Some(ProjectPath(path.to_string())),
            locator,
            rule: rule.to_string(),
        }],
    }
}

fn read_limit_diagnostic(path: &str) -> Diagnostic {
    Diagnostic::new(
        SCANNER_ID,
        "manifest.read_limit",
        DiagnosticSeverity::Warning,
        Some(ProjectPath(path.to_string())),
        "the manifest exceeds max_file_read_bytes and was not parsed",
    )
}

fn manifest_read_diagnostic(path: &str, error: &io::Error) -> Diagnostic {
    let (code, message) = match error.kind() {
        io::ErrorKind::NotFound => (
            "manifest.missing",
            "the manifest disappeared after inventory",
        ),
        io::ErrorKind::PermissionDenied => (
            "manifest.permission_denied",
            "permission was denied while reading the manifest",
        ),
        _ => ("manifest.read_failed", "the manifest could not be read"),
    };
    Diagnostic::new(
        SCANNER_ID,
        code,
        DiagnosticSeverity::Warning,
        Some(ProjectPath(path.to_string())),
        message,
    )
}

fn sort_diagnostics(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.sort_by(|left, right| {
        (
            left.path.as_ref().map(|path| path.as_str()),
            left.code.as_str(),
            left.message.as_str(),
        )
            .cmp(&(
                right.path.as_ref().map(|path| path.as_str()),
                right.code.as_str(),
                right.message.as_str(),
            ))
    });
    diagnostics.dedup_by(|left, right| {
        left.path == right.path && left.code == right.code && left.message == right.message
    });
}

fn validate_workspace_members(
    workspaces: &[WorkspaceDeclaration],
    candidates: &[(&InventoryFile, ManifestFormat)],
    records: &mut Vec<ManifestRecord>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let roots = candidates
        .iter()
        .filter(|(_, format)| is_package_manifest(*format))
        .map(|(file, format)| (format.ecosystem(), manifest_root(&file.relative)))
        .collect::<Vec<_>>();

    for workspace in workspaces {
        for member in &workspace.members {
            if member.starts_with('!') {
                continue;
            }
            let Some(resolved) = resolve_relative(&workspace.root, member) else {
                push_missing_workspace_member(workspace, member, records, diagnostics);
                continue;
            };
            let found = roots.iter().any(|(ecosystem, root)| {
                *ecosystem == workspace.ecosystem && wildcard_match(&resolved, root)
            });
            if !found {
                push_missing_workspace_member(workspace, &resolved, records, diagnostics);
            }
        }
    }
}

fn push_missing_workspace_member(
    workspace: &WorkspaceDeclaration,
    member: &str,
    records: &mut Vec<ManifestRecord>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let display = clean_path_value(member);
    records.push(ManifestRecord {
        fact: Fact::Marker(MarkerFact {
            kind: MarkerKind::MissingWorkspaceMember,
            id: workspace.ecosystem.to_string(),
            path: display.clone(),
            detail: Some(workspace.manifest.clone()),
        }),
        provenance: provenance(
            classify_project_path(&workspace.manifest),
            Confidence::High,
            &workspace.manifest,
            Some(display.clone()),
            "manifest.workspace_member_missing",
        ),
        source_manifest: workspace.manifest.clone(),
    });
    diagnostics.push(Diagnostic::new(
        SCANNER_ID,
        "manifest.workspace_member_missing",
        DiagnosticSeverity::Warning,
        Some(ProjectPath(workspace.manifest.clone())),
        format!("workspace member `{display}` has no recognized package manifest"),
    ));
}

fn is_package_manifest(format: ManifestFormat) -> bool {
    matches!(
        format,
        ManifestFormat::Cargo
            | ManifestFormat::NodePackage
            | ManifestFormat::PythonProject
            | ManifestFormat::GoModule
            | ManifestFormat::Maven
            | ManifestFormat::GradleBuild
            | ManifestFormat::SwiftPackage
    )
}

fn inferred_manager(
    format: ManifestFormat,
    manifest: &str,
    files: &[InventoryFile],
) -> Option<&'static str> {
    match format {
        ManifestFormat::NodePackage | ManifestFormat::PnpmWorkspace => nearest_marker(
            manifest,
            files,
            &[
                ("pnpm-lock.yaml", "pnpm"),
                ("pnpm-workspace.yaml", "pnpm"),
                ("pnpm-workspace.yml", "pnpm"),
                ("yarn.lock", "yarn"),
                ("bun.lock", "bun"),
                ("bun.lockb", "bun"),
                ("package-lock.json", "npm"),
            ],
        )
        .or(Some("npm")),
        ManifestFormat::PythonProject => nearest_marker(
            manifest,
            files,
            &[
                ("uv.lock", "uv"),
                ("poetry.lock", "poetry"),
                ("Pipfile.lock", "pipenv"),
            ],
        ),
        ManifestFormat::Maven => nearest_marker(manifest, files, &[("mvnw", "./mvnw")]),
        ManifestFormat::GradleBuild | ManifestFormat::GradleSettings => {
            nearest_marker(manifest, files, &[("gradlew", "./gradlew")])
        }
        _ => None,
    }
}

fn nearest_marker<'a>(
    manifest: &str,
    files: &[InventoryFile],
    markers: &[(&str, &'a str)],
) -> Option<&'a str> {
    let current_root = manifest_root(manifest);
    let mut best = None::<(usize, &'a str)>;
    for file in files {
        let Some((_, value)) = markers
            .iter()
            .find(|(name, _)| file.relative.rsplit('/').next() == Some(*name))
        else {
            continue;
        };
        let marker_root = manifest_root(&file.relative);
        if path_contains(&marker_root, &current_root) {
            let length = marker_root.len();
            if best.is_none_or(|(best_length, _)| length > best_length) {
                best = Some((length, *value));
            }
        }
    }
    best.map(|(_, value)| value)
}

struct BoundedRead {
    bytes: Vec<u8>,
    truncated: bool,
}

trait ManifestReader {
    fn read_bounded(&mut self, path: &Path, limit: u64) -> io::Result<BoundedRead>;
}

struct FileManifestReader;

impl ManifestReader for FileManifestReader {
    fn read_bounded(&mut self, path: &Path, limit: u64) -> io::Result<BoundedRead> {
        let mut bytes = Vec::new();
        File::open(path)?
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)?;
        let truncated = bytes.len() as u64 > limit;
        if truncated {
            bytes.truncate(limit as usize);
        }
        Ok(BoundedRead { bytes, truncated })
    }
}

pub(super) struct ParseContext<'a> {
    pub(super) path: &'a str,
    pub(super) root: &'a str,
    pub(super) text: &'a str,
    pub(super) inventory_paths: &'a BTreeSet<String>,
    pub(super) package_manager: Option<&'static str>,
}

impl ParseContext<'_> {
    pub(super) fn resolve(&self, path: &str) -> Option<String> {
        resolve_relative(self.root, path)
    }

    pub(super) fn has_path(&self, path: &str) -> bool {
        self.inventory_paths.contains(path)
    }
}

#[derive(Default)]
pub(super) struct ParseResult {
    facts: Vec<ParsedFact>,
    workspaces: Vec<WorkspaceDeclaration>,
    diagnostics: Vec<ParseIssue>,
}

impl ParseResult {
    pub(super) fn fact(
        &mut self,
        fact: Fact,
        confidence: Confidence,
        rule: &'static str,
        locator: impl Into<Option<String>>,
    ) {
        let locator = locator
            .into()
            .and_then(|value| clean_text_value(&value, 256));
        self.facts.push(ParsedFact {
            fact,
            confidence,
            rule,
            locator,
        });
    }

    pub(super) fn package(&mut self, context: &ParseContext<'_>, name: &str, rule: &'static str) {
        if let Some(name) = clean_identifier(name) {
            self.fact(
                Fact::Package(PackageFact {
                    name,
                    root: context.root.to_string(),
                    ecosystem: ecosystem_for_manifest(context.path).to_string(),
                    manifest: context.path.to_string(),
                }),
                Confidence::High,
                rule,
                Some("name".to_string()),
            );
        }
    }

    pub(super) fn workspace(
        &mut self,
        context: &ParseContext<'_>,
        kind: &str,
        members: Vec<String>,
        rule: &'static str,
    ) {
        let Some(kind) = clean_identifier(kind) else {
            return;
        };
        let members = members
            .into_iter()
            .filter_map(|member| clean_text_value(&member, 256))
            .collect::<Vec<_>>();
        self.fact(
            Fact::Workspace(WorkspaceFact {
                kind,
                root: context.root.to_string(),
                manifest: context.path.to_string(),
                members: members.clone(),
            }),
            Confidence::High,
            rule,
            None,
        );
        self.workspaces.push(WorkspaceDeclaration {
            ecosystem: ecosystem_for_manifest(context.path),
            root: context.root.to_string(),
            manifest: context.path.to_string(),
            members,
        });
    }

    pub(super) fn dependency(
        &mut self,
        context: &ParseContext<'_>,
        package: &str,
        name: &str,
        requirement: Option<String>,
        scope: DependencyScope,
        locator: String,
    ) {
        let Some(name) = clean_identifier(name) else {
            return;
        };
        let Some(package) = clean_identifier(package) else {
            return;
        };
        let rule = match ecosystem_for_manifest(context.path) {
            "rust" => "cargo.dependency",
            "node" => "node.dependency",
            "python" => "python.dependency",
            "go" => "go.require",
            "jvm" => "jvm.dependency",
            "swift" => "swift.dependency",
            _ => "manifest.dependency",
        };
        self.fact(
            Fact::Dependency(DependencyFact {
                ecosystem: ecosystem_for_manifest(context.path).to_string(),
                package,
                name,
                requirement: requirement.and_then(safe_requirement),
                scope,
                manifest: context.path.to_string(),
            }),
            Confidence::High,
            rule,
            Some(locator),
        );
    }

    pub(super) fn tool(
        &mut self,
        context: &ParseContext<'_>,
        id: &str,
        category: ToolCategory,
        rule: &'static str,
    ) {
        let Some(id) = clean_identifier(id) else {
            return;
        };
        self.fact(
            Fact::Tool(ToolFact {
                id,
                category,
                source_path: context.path.to_string(),
            }),
            Confidence::High,
            rule,
            None,
        );
    }

    pub(super) fn command(
        &mut self,
        context: &ParseContext<'_>,
        executable: &str,
        arguments: &[&str],
        purpose: CommandPurpose,
        locator: impl Into<Option<String>>,
        rule: &'static str,
    ) {
        let Some(executable) = clean_text_value(executable, 256) else {
            return;
        };
        let Some(arguments) = arguments
            .iter()
            .map(|argument| clean_text_value(argument, 512))
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        self.fact(
            Fact::Command(CommandFact {
                executable,
                arguments,
                working_directory: context.root.to_string(),
                purpose,
                source_path: context.path.to_string(),
            }),
            Confidence::High,
            rule,
            locator,
        );
    }

    pub(super) fn entry(
        &mut self,
        context: &ParseContext<'_>,
        path: &str,
        kind: &str,
        language: &str,
        locator: String,
        rule: &'static str,
    ) {
        let Some(path) = context.resolve(path) else {
            return;
        };
        if !context.has_path(&path) {
            return;
        }
        self.fact(
            Fact::Marker(MarkerFact {
                kind: MarkerKind::EntryPoint,
                id: kind.to_string(),
                path,
                detail: Some(language.to_string()),
            }),
            Confidence::High,
            rule,
            Some(locator),
        );
    }

    pub(super) fn license(
        &mut self,
        context: &ParseContext<'_>,
        value: &str,
        locator: String,
        rule: &'static str,
    ) {
        if value.contains("://") || value.starts_with("git@") || value.starts_with("ssh:") {
            return;
        }
        let Some(value) = clean_text_value(value, 160) else {
            return;
        };
        self.fact(
            Fact::Marker(MarkerFact {
                kind: MarkerKind::DeclaredLicense,
                id: value,
                path: context.path.to_string(),
                detail: None,
            }),
            Confidence::High,
            rule,
            Some(locator),
        );
    }

    pub(super) fn license_file(
        &mut self,
        context: &ParseContext<'_>,
        path: &str,
        locator: String,
        rule: &'static str,
    ) {
        let Some(path) = context.resolve(path) else {
            return;
        };
        if !context.has_path(&path) {
            return;
        }
        self.fact(
            Fact::Marker(MarkerFact {
                kind: MarkerKind::LicenseFile,
                id: "license".to_string(),
                path,
                detail: None,
            }),
            Confidence::High,
            rule,
            Some(locator),
        );
    }

    pub(super) fn warn(&mut self, code: &'static str, message: &'static str) {
        self.diagnostics.push(ParseIssue { code, message });
    }
}

struct ParsedFact {
    fact: Fact,
    confidence: Confidence,
    rule: &'static str,
    locator: Option<String>,
}

struct WorkspaceDeclaration {
    ecosystem: &'static str,
    root: String,
    manifest: String,
    members: Vec<String>,
}

struct ParseIssue {
    code: &'static str,
    message: &'static str,
}

pub(super) fn manifest_root(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(|| ".".to_string(), |(root, _)| root.to_string())
}

fn ecosystem_for_manifest(path: &str) -> &'static str {
    ManifestFormat::classify(path)
        .map(ManifestFormat::ecosystem)
        .unwrap_or("unknown")
}

pub(super) fn workspace_owner(root: &str) -> String {
    if root == "." {
        "workspace".to_string()
    } else {
        root.to_string()
    }
}

pub(super) fn safe_requirement(value: String) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.contains("://")
        || value.starts_with("git+")
        || value.starts_with("git@")
        || value.starts_with("ssh:")
        || value.starts_with("github:")
        || value.starts_with("gitlab:")
        || value.starts_with("bitbucket:")
    {
        return None;
    }
    clean_text_value(value, 256)
}

pub(super) fn clean_identifier(value: &str) -> Option<String> {
    let value = value.trim();
    if value.contains("://")
        || value.starts_with("git+")
        || value.starts_with("git@")
        || value.starts_with("ssh:")
    {
        return None;
    }
    clean_text_value(value, 256)
}

fn clean_text_value(value: &str, max_chars: usize) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    Some(value.chars().take(max_chars).collect())
}

fn clean_path_value(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(256)
        .collect()
}

pub(super) fn resolve_relative(root: &str, path: &str) -> Option<String> {
    let path = path
        .trim()
        .trim_matches(['"', '\''])
        .trim_start_matches("./");
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return None;
    }
    let mut components = if root == "." {
        Vec::new()
    } else {
        root.split('/').map(str::to_string).collect::<Vec<_>>()
    };
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop()?;
            }
            value => components.push(value.to_string()),
        }
    }
    Some(if components.is_empty() {
        ".".to_string()
    } else {
        components.join("/")
    })
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let (mut pattern_index, mut value_index) = (0, 0);
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let (mut star, mut retry) = (None, 0);
    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == value[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star = Some(pattern_index);
            pattern_index += 1;
            retry = value_index;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            retry += 1;
            value_index = retry;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory;
    use std::{collections::BTreeMap, fs, path::PathBuf};

    #[derive(Default)]
    struct CountingReader {
        reads: BTreeMap<PathBuf, usize>,
    }

    struct FailingReader(io::ErrorKind);

    impl ManifestReader for FailingReader {
        fn read_bounded(&mut self, _path: &Path, _limit: u64) -> io::Result<BoundedRead> {
            Err(io::Error::new(self.0, "injected failure"))
        }
    }

    impl ManifestReader for CountingReader {
        fn read_bounded(&mut self, path: &Path, limit: u64) -> io::Result<BoundedRead> {
            *self.reads.entry(path.to_path_buf()).or_default() += 1;
            FileManifestReader.read_bounded(path, limit)
        }
    }

    #[test]
    fn reads_each_known_manifest_once_and_sorts_records() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::write(
            directory.path().join("package.json"),
            r#"{"name":"web","dependencies":{"z":"1","a":"2"}}"#,
        )
        .expect("package manifest");
        fs::write(
            directory.path().join("Cargo.toml"),
            "[package]\nname = \"crate\"\nversion = \"0.1.0\"\n",
        )
        .expect("cargo manifest");
        let options = ScanOptions::default();
        let inventory = inventory::scan(directory.path(), &options).inventory;
        let mut reader = CountingReader::default();

        let output = scan_with_reader(&inventory, &options, &mut reader);

        assert_eq!(
            reader.reads.values().copied().collect::<Vec<_>>(),
            vec![1, 1]
        );
        assert!(output.catalog.records().windows(2).all(|records| {
            (&records[0].fact, &records[0].provenance) <= (&records[1].fact, &records[1].provenance)
        }));
    }

    #[test]
    fn oversized_and_malformed_manifests_do_not_hide_valid_ones() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::write(directory.path().join("package.json"), "{broken").expect("bad manifest");
        fs::write(
            directory.path().join("Cargo.toml"),
            "[package]\nname = \"valid\"\n",
        )
        .expect("valid manifest");
        let options = ScanOptions::default();
        let inventory = inventory::scan(directory.path(), &options).inventory;

        let output = scan(&inventory, &options);

        assert_eq!(output.result.status, ScannerStatus::Partial);
        assert!(output.catalog.records().iter().any(|record| matches!(
            &record.fact,
            Fact::Package(package) if package.name == "valid"
        )));
        assert!(output.catalog.records().iter().any(|record| matches!(
            &record.fact,
            Fact::Manifest(manifest) if manifest.path == "package.json"
        )));
    }

    #[test]
    fn permission_failures_are_identified_without_machine_permissions() {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::write(
            directory.path().join("Cargo.toml"),
            "[package]\nname = \"valid\"\n",
        )
        .expect("manifest");
        let options = ScanOptions::default();
        let inventory = inventory::scan(directory.path(), &options).inventory;

        let output = scan_with_reader(
            &inventory,
            &options,
            &mut FailingReader(io::ErrorKind::PermissionDenied),
        );
        assert_eq!(output.result.status, ScannerStatus::Partial);
        assert_eq!(output.diagnostics[0].code, "manifest.permission_denied");
        assert!(output.catalog.records().iter().any(|record| matches!(
            &record.fact,
            Fact::Manifest(manifest) if manifest.path == "Cargo.toml"
        )));
    }

    #[test]
    fn resolves_workspace_globs_without_machine_state() {
        assert!(wildcard_match("packages/*", "packages/core"));
        assert!(wildcard_match("crates/**", "crates/group/member"));
        assert!(!wildcard_match("packages/*", "apps/web"));
        assert_eq!(
            resolve_relative("workspaces/root", "../shared"),
            Some("workspaces/shared".to_string())
        );
        assert_eq!(resolve_relative(".", "../outside"), None);
    }
}
