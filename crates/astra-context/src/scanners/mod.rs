pub(crate) mod build;
pub(crate) mod ci;
pub(crate) mod configuration;
pub(crate) mod dependencies;
pub(crate) mod documentation;
pub(crate) mod entry_points;
pub(crate) mod git;
pub(crate) mod languages;
pub(crate) mod license;
pub(crate) mod testing;
pub(crate) mod validation;
pub(crate) mod workspace;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        facts::{
            CommandFact, DependencyFact, DocumentationFact, Fact, FactGraphBuilder, FactProvenance,
            FileFact, FileRole, MarkerFact, MarkerKind, PackageFact, RelationKind, ToolCategory,
            ToolFact, WorkspaceFact,
        },
        scanner::ScannerInput,
        scope::SemanticScope,
        CommandPurpose, Confidence, DependencyScope, DocumentKind,
    };

    fn provenance() -> FactProvenance {
        FactProvenance {
            scanner: "synthetic".to_string(),
            scope: SemanticScope::Primary,
            confidence: Confidence::High,
            evidence: Vec::new(),
        }
    }

    #[test]
    fn every_projection_consumes_one_immutable_graph() {
        let mut builder = FactGraphBuilder::new();
        builder.add_fact(
            Fact::File(FileFact {
                path: "src/main.rs".to_string(),
                bytes: 10,
                role: FileRole::Source,
                extension: Some("rs".to_string()),
                language: Some("rust".to_string()),
            }),
            provenance(),
        );
        builder.add_fact(
            Fact::Workspace(WorkspaceFact {
                kind: "cargo_workspace".to_string(),
                root: String::new(),
                manifest: "Cargo.toml".to_string(),
                members: vec!["*".to_string()],
            }),
            provenance(),
        );
        let package = builder.add_fact(
            Fact::Package(PackageFact {
                name: "sample".to_string(),
                root: String::new(),
                ecosystem: "cargo".to_string(),
                manifest: "Cargo.toml".to_string(),
            }),
            provenance(),
        );
        builder.add_fact(
            Fact::Dependency(DependencyFact {
                ecosystem: "cargo".to_string(),
                package: "sample".to_string(),
                name: "serde".to_string(),
                requirement: Some("1".to_string()),
                scope: DependencyScope::Runtime,
                manifest: "Cargo.toml".to_string(),
            }),
            provenance(),
        );
        builder.add_fact(
            Fact::Documentation(DocumentationFact {
                path: "README.md".to_string(),
                kind: DocumentKind::Readme,
                title: Some("Sample".to_string()),
                headings: vec!["Sample".to_string()],
                bytes: 10,
            }),
            provenance(),
        );
        for (kind, id, path, detail) in [
            (
                MarkerKind::Ci,
                "github-actions",
                ".github/workflows/ci.yml",
                None,
            ),
            (MarkerKind::Configuration, "cargo", "Cargo.toml", None),
            (MarkerKind::LicenseFile, "license", "LICENSE", None),
            (MarkerKind::DeclaredLicense, "MIT", "Cargo.toml", None),
        ] {
            builder.add_fact(
                Fact::Marker(MarkerFact {
                    kind,
                    id: id.to_string(),
                    path: path.to_string(),
                    detail: detail.map(str::to_string),
                }),
                provenance(),
            );
        }
        let entry = builder.add_fact(
            Fact::Marker(MarkerFact {
                kind: MarkerKind::EntryPoint,
                id: "binary".to_string(),
                path: "src/main.rs".to_string(),
                detail: Some("rust".to_string()),
            }),
            provenance(),
        );
        builder.add_relation(&entry, &package, RelationKind::EntrypointOf);
        builder.add_fact(
            Fact::Command(CommandFact {
                executable: "cargo".to_string(),
                arguments: vec!["test".to_string()],
                working_directory: String::new(),
                purpose: CommandPurpose::Test,
                source_path: "Cargo.toml".to_string(),
            }),
            provenance(),
        );
        for (id, category) in [
            ("cargo", ToolCategory::PackageManager),
            ("cargo", ToolCategory::BuildSystem),
            ("cargo-test", ToolCategory::TestingFramework),
        ] {
            builder.add_fact(
                Fact::Tool(ToolFact {
                    id: id.to_string(),
                    category,
                    source_path: "Cargo.toml".to_string(),
                }),
                provenance(),
            );
        }

        let graph = builder.finish().expect("graph");
        let input = ScannerInput::new(&graph);
        assert_eq!(languages::scan(&input).value.len(), 1);
        assert_eq!(workspace::scan(&input).value.summary.packages.len(), 1);
        assert_eq!(dependencies::scan(&input).value.len(), 1);
        assert_eq!(documentation::scan(&input).value.len(), 1);
        assert_eq!(ci::scan(&input).value.len(), 1);
        assert_eq!(configuration::scan(&input).value.len(), 1);
        assert_eq!(validation::scan(&input).value.validation.len(), 1);
        assert_eq!(build::scan(&input).value.len(), 1);
        assert_eq!(testing::scan(&input).value.len(), 1);
        assert_eq!(
            entry_points::scan(&input).value[0].value.package.as_deref(),
            Some("sample")
        );
        assert_eq!(license::scan(&input).value.declared.len(), 1);
    }

    #[test]
    fn projection_sources_do_not_import_discovery_boundaries() {
        let sources = [
            ("languages", include_str!("languages.rs")),
            ("workspace", include_str!("workspace.rs")),
            ("dependencies", include_str!("dependencies.rs")),
            ("documentation", include_str!("documentation.rs")),
            ("ci", include_str!("ci.rs")),
            ("configuration", include_str!("configuration.rs")),
            ("validation", include_str!("validation.rs")),
            ("build", include_str!("build.rs")),
            ("testing", include_str!("testing.rs")),
            ("entry_points", include_str!("entry_points.rs")),
            ("license", include_str!("license.rs")),
            ("context_projection", include_str!("../projection.rs")),
        ];
        let forbidden = [
            "std::fs",
            "File::",
            "Command::",
            "ProcessRunner",
            "inventory::",
            "manifests::",
        ];
        for (scanner, source) in sources {
            for boundary in forbidden {
                assert!(
                    !source.contains(boundary),
                    "projection scanner {scanner} references forbidden boundary {boundary}"
                );
            }
        }
    }
}
