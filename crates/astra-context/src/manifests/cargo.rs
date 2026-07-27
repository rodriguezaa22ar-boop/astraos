use super::{clean_identifier, workspace_owner, ParseContext, ParseResult};
use crate::{facts::ToolCategory, CommandPurpose, DependencyScope};
use toml::Value;

pub(super) fn parse(context: &ParseContext<'_>) -> ParseResult {
    let mut result = ParseResult::default();
    let document = match toml::from_str::<Value>(context.text) {
        Ok(document) => document,
        Err(_) => {
            result.warn(
                "manifest.parse_failed",
                "Cargo.toml could not be parsed as TOML",
            );
            return result;
        }
    };

    let package = document.get("package").and_then(Value::as_table);
    let package_name = package
        .and_then(|table| table.get("name"))
        .and_then(Value::as_str)
        .and_then(clean_identifier);
    if let Some(name) = &package_name {
        result.package(context, name, "cargo.package");
    }

    let workspace = document.get("workspace").and_then(Value::as_table);
    if let Some(workspace) = workspace {
        let mut members = workspace
            .get("members")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>();
        members.extend(
            workspace
                .get("exclude")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|member| format!("!{member}")),
        );
        result.workspace(context, "cargo", members, "cargo.workspace");
    }

    result.tool(context, "cargo", ToolCategory::PackageManager, "cargo.tool");
    result.tool(context, "cargo", ToolCategory::BuildSystem, "cargo.tool");
    result.tool(
        context,
        "cargo-test",
        ToolCategory::TestingFramework,
        "cargo.test_harness",
    );
    let workspace_arguments = workspace.is_some();
    for (command, purpose) in [
        ("build", CommandPurpose::Build),
        ("test", CommandPurpose::Test),
        ("check", CommandPurpose::Validate),
    ] {
        let arguments = if workspace_arguments {
            vec![command, "--workspace"]
        } else {
            vec![command]
        };
        result.command(context, "cargo", &arguments, purpose, None, "cargo.command");
    }

    let owner = package_name
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| workspace_owner(context.root));
    for (table_name, scope) in [
        ("dependencies", DependencyScope::Runtime),
        ("dev-dependencies", DependencyScope::Development),
        ("build-dependencies", DependencyScope::Build),
    ] {
        if let Some(table) = document.get(table_name).and_then(Value::as_table) {
            parse_dependencies(context, &mut result, &owner, table, scope, table_name);
        }
    }
    if let Some(targets) = document.get("target").and_then(Value::as_table) {
        for (target, target_value) in targets {
            let Some(target_table) = target_value.as_table() else {
                continue;
            };
            for (table_name, scope) in [
                ("dependencies", DependencyScope::Runtime),
                ("dev-dependencies", DependencyScope::Development),
                ("build-dependencies", DependencyScope::Build),
            ] {
                if let Some(table) = target_table.get(table_name).and_then(Value::as_table) {
                    parse_dependencies(
                        context,
                        &mut result,
                        &owner,
                        table,
                        scope,
                        &format!("target.{target}.{table_name}"),
                    );
                }
            }
        }
    }
    if let Some(workspace_dependencies) = workspace
        .and_then(|table| table.get("dependencies"))
        .and_then(Value::as_table)
    {
        parse_dependencies(
            context,
            &mut result,
            &workspace_owner(context.root),
            workspace_dependencies,
            DependencyScope::Runtime,
            "workspace.dependencies",
        );
    }

    if let Some(license) = package
        .and_then(|table| table.get("license"))
        .and_then(Value::as_str)
    {
        result.license(
            context,
            license,
            "package.license".to_string(),
            "cargo.license",
        );
    }
    if let Some(license_file) = package
        .and_then(|table| table.get("license-file"))
        .and_then(Value::as_str)
    {
        result.license_file(
            context,
            license_file,
            "package.license-file".to_string(),
            "cargo.license_file",
        );
    }
    if let Some(license) = workspace
        .and_then(|table| table.get("package"))
        .and_then(Value::as_table)
        .and_then(|table| table.get("license"))
        .and_then(Value::as_str)
    {
        result.license(
            context,
            license,
            "workspace.package.license".to_string(),
            "cargo.workspace_license",
        );
    }

    if package.is_some() {
        parse_entries(context, &document, &mut result);
    }
    result
}

fn parse_dependencies(
    context: &ParseContext<'_>,
    result: &mut ParseResult,
    owner: &str,
    table: &toml::map::Map<String, Value>,
    default_scope: DependencyScope,
    locator_prefix: &str,
) {
    for (name, value) in table {
        let (requirement, scope) = match value {
            Value::String(requirement) => (Some(requirement.clone()), default_scope),
            Value::Table(properties) => {
                let requirement = properties
                    .get("version")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let scope = if properties
                    .get("optional")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    DependencyScope::Optional
                } else {
                    default_scope
                };
                (requirement, scope)
            }
            _ => (None, default_scope),
        };
        result.dependency(
            context,
            owner,
            name,
            requirement,
            scope,
            format!("{locator_prefix}.{name}"),
        );
    }
}

fn parse_entries(context: &ParseContext<'_>, document: &Value, result: &mut ParseResult) {
    if let Some(library) = document.get("lib").and_then(Value::as_table) {
        let path = library
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("src/lib.rs");
        result.entry(
            context,
            path,
            "library",
            "rust",
            "lib.path".to_string(),
            "cargo.entry",
        );
    } else if let Some(path) = context.resolve("src/lib.rs") {
        if context.has_path(&path) {
            result.entry(
                context,
                "src/lib.rs",
                "library",
                "rust",
                "package.default_lib".to_string(),
                "cargo.default_entry",
            );
        }
    }

    if let Some(binaries) = document.get("bin").and_then(Value::as_array) {
        for (index, binary) in binaries.iter().enumerate() {
            let Some(binary) = binary.as_table() else {
                continue;
            };
            let path = binary
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("src/main.rs");
            result.entry(
                context,
                path,
                "binary",
                "rust",
                format!("bin[{index}].path"),
                "cargo.entry",
            );
        }
    } else if let Some(path) = context.resolve("src/main.rs") {
        if context.has_path(&path) {
            result.entry(
                context,
                "src/main.rs",
                "binary",
                "rust",
                "package.default_bin".to_string(),
                "cargo.default_entry",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn parses_package_workspace_scopes_and_license() {
        let paths = BTreeSet::from(["src/main.rs".to_string()]);
        let context = ParseContext {
            path: "Cargo.toml",
            root: ".",
            text: r#"
                [package]
                name = "demo"
                license = "MIT"
                [dependencies]
                serde = "1"
                tracing = { version = "0.1", optional = true }
                [dev-dependencies]
                tempfile = "3"
                [workspace]
                members = ["crates/*"]
            "#,
            inventory_paths: &paths,
            package_manager: None,
        };
        let parsed = parse(&context);

        assert!(parsed.facts.iter().any(
            |fact| matches!(&fact.fact, crate::facts::Fact::Package(package) if package.name == "demo")
        ));
        assert!(parsed.facts.iter().any(|fact| matches!(
            &fact.fact,
            crate::facts::Fact::Dependency(dependency)
                if dependency.name == "tracing" && dependency.scope == DependencyScope::Optional
        )));
        assert_eq!(parsed.workspaces[0].members, vec!["crates/*"]);
    }
}
