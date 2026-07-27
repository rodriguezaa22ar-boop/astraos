use super::{clean_identifier, safe_requirement, ParseContext, ParseResult};
use crate::{facts::ToolCategory, CommandPurpose, DependencyScope};
use serde_json::{Map, Value};

pub(super) fn parse(context: &ParseContext<'_>, pnpm_workspace: bool) -> ParseResult {
    if pnpm_workspace {
        return parse_pnpm_workspace(context);
    }

    let mut result = ParseResult::default();
    let document = match serde_json::from_str::<Value>(context.text) {
        Ok(Value::Object(document)) => document,
        Ok(_) | Err(_) => {
            result.warn(
                "manifest.parse_failed",
                "package.json could not be parsed as a JSON object",
            );
            return result;
        }
    };

    let package_name = document
        .get("name")
        .and_then(Value::as_str)
        .and_then(clean_identifier);
    if let Some(name) = &package_name {
        result.package(context, name, "node.package");
    }
    let owner = package_name
        .clone()
        .unwrap_or_else(|| context.root.to_string());
    let manager = package_manager(&document)
        .or(context.package_manager)
        .unwrap_or("npm");
    result.tool(
        context,
        manager,
        ToolCategory::PackageManager,
        "node.package_manager",
    );

    if let Some(members) = workspace_members(&document) {
        let kind = if manager == "npm" { "node" } else { manager };
        result.workspace(context, kind, members, "node.workspace");
    }

    for (field, scope) in [
        ("dependencies", DependencyScope::Runtime),
        ("devDependencies", DependencyScope::Development),
        ("optionalDependencies", DependencyScope::Optional),
        ("peerDependencies", DependencyScope::Peer),
    ] {
        let Some(dependencies) = document.get(field).and_then(Value::as_object) else {
            continue;
        };
        for (name, requirement) in dependencies {
            result.dependency(
                context,
                &owner,
                name,
                requirement
                    .as_str()
                    .map(str::to_string)
                    .and_then(safe_requirement),
                scope,
                format!("{field}.{name}"),
            );
            if let Some(framework) = testing_framework(name) {
                result.tool(
                    context,
                    framework,
                    ToolCategory::TestingFramework,
                    "node.testing_framework",
                );
            }
        }
    }

    if let Some(scripts) = document.get("scripts").and_then(Value::as_object) {
        for (name, body) in scripts {
            if !body.is_string() {
                continue;
            }
            let Some(purpose) = script_purpose(name) else {
                continue;
            };
            result.command(
                context,
                manager,
                &["run", name],
                purpose,
                Some(format!("scripts.{name}")),
                "node.script",
            );
        }
    }

    parse_entries(context, &document, &mut result);
    if let Some(license) = document.get("license").and_then(|value| match value {
        Value::String(value) => Some(value.as_str()),
        Value::Object(value) => value.get("type").and_then(Value::as_str),
        _ => None,
    }) {
        result.license(context, license, "license".to_string(), "node.license");
    }
    result
}

fn parse_pnpm_workspace(context: &ParseContext<'_>) -> ParseResult {
    let mut result = ParseResult::default();
    let mut members = Vec::new();
    let mut in_packages = false;
    for line in context.text.lines() {
        let trimmed = line.trim();
        if trimmed == "packages:" {
            in_packages = true;
            continue;
        }
        if in_packages && !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
            in_packages = false;
        }
        if !in_packages {
            continue;
        }
        let Some(value) = trimmed.strip_prefix('-') else {
            continue;
        };
        let value = value
            .split(" #")
            .next()
            .unwrap_or(value)
            .trim()
            .trim_matches(['"', '\'']);
        if !value.is_empty() {
            members.push(value.to_string());
        }
    }
    result.workspace(context, "pnpm", members, "node.pnpm_workspace");
    result.tool(
        context,
        "pnpm",
        ToolCategory::PackageManager,
        "node.pnpm_workspace",
    );
    result
}

fn package_manager(document: &Map<String, Value>) -> Option<&str> {
    let value = document.get("packageManager")?.as_str()?;
    let manager = value.split('@').next().unwrap_or(value);
    matches!(manager, "npm" | "pnpm" | "yarn" | "bun").then_some(manager)
}

fn workspace_members(document: &Map<String, Value>) -> Option<Vec<String>> {
    let value = document.get("workspaces")?;
    let values = match value {
        Value::Array(values) => Some(values),
        Value::Object(values) => values.get("packages").and_then(Value::as_array),
        _ => None,
    }?;
    Some(
        values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
    )
}

fn testing_framework(name: &str) -> Option<&'static str> {
    if name == "vitest" || name.starts_with("@vitest/") {
        Some("vitest")
    } else if name == "jest" || name.starts_with("@jest/") {
        Some("jest")
    } else if name == "mocha" {
        Some("mocha")
    } else {
        None
    }
}

fn script_purpose(name: &str) -> Option<CommandPurpose> {
    match name.split(':').next()? {
        "dev" | "start" | "serve" => Some(CommandPurpose::Develop),
        "build" | "compile" => Some(CommandPurpose::Build),
        "test" => Some(CommandPurpose::Test),
        "lint" => Some(CommandPurpose::Lint),
        "fmt" | "format" => Some(CommandPurpose::Format),
        "check" | "typecheck" | "validate" => Some(CommandPurpose::Validate),
        _ => None,
    }
}

fn parse_entries(
    context: &ParseContext<'_>,
    document: &Map<String, Value>,
    result: &mut ParseResult,
) {
    if let Some(path) = document.get("main").and_then(Value::as_str) {
        add_entry(context, result, path, "application", "main");
    }
    if let Some(path) = document.get("module").and_then(Value::as_str) {
        add_entry(context, result, path, "library", "module");
    }
    if let Some(exports) = document.get("exports") {
        let mut paths = Vec::new();
        collect_export_paths(exports, &mut paths);
        for (index, path) in paths.into_iter().enumerate() {
            add_entry(
                context,
                result,
                path,
                "library",
                &format!("exports[{index}]"),
            );
        }
    }
    if let Some(bin) = document.get("bin") {
        match bin {
            Value::String(path) => add_entry(context, result, path, "binary", "bin"),
            Value::Object(values) => {
                for (name, path) in values {
                    if let Some(path) = path.as_str() {
                        add_entry(context, result, path, "binary", &format!("bin.{name}"));
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_export_paths<'a>(value: &'a Value, paths: &mut Vec<&'a str>) {
    match value {
        Value::String(path) if path.starts_with('.') => paths.push(path),
        Value::Object(values) => {
            for value in values.values() {
                collect_export_paths(value, paths);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_export_paths(value, paths);
            }
        }
        _ => {}
    }
}

fn add_entry(
    context: &ParseContext<'_>,
    result: &mut ParseResult,
    path: &str,
    kind: &str,
    locator: &str,
) {
    if path.contains("://") {
        return;
    }
    let language = match path.rsplit('.').next() {
        Some("ts" | "tsx" | "mts" | "cts") => "typescript",
        _ => "javascript",
    };
    result.entry(
        context,
        path,
        kind,
        language,
        locator.to_string(),
        "node.entry",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::Fact;
    use std::collections::BTreeSet;

    #[test]
    fn scripts_are_reported_as_package_manager_argv() {
        let paths = BTreeSet::new();
        let context = ParseContext {
            path: "package.json",
            root: ".",
            text: r#"{
                "name": "demo",
                "packageManager": "pnpm@9",
                "scripts": {"test": "arbitrary shell | syntax"},
                "devDependencies": {"vitest": "^2"}
            }"#,
            inventory_paths: &paths,
            package_manager: None,
        };
        let parsed = parse(&context, false);
        assert!(parsed.facts.iter().any(|fact| matches!(
            &fact.fact,
            Fact::Command(command)
                if command.executable == "pnpm"
                    && command.arguments == ["run", "test"]
        )));
    }

    #[test]
    fn parses_pnpm_members_without_a_yaml_dependency() {
        let paths = BTreeSet::new();
        let context = ParseContext {
            path: "pnpm-workspace.yaml",
            root: ".",
            text: "packages:\n  - 'apps/*'\n  - \"packages/*\"\n",
            inventory_paths: &paths,
            package_manager: Some("pnpm"),
        };
        let parsed = parse(&context, true);
        assert_eq!(parsed.workspaces[0].members, vec!["apps/*", "packages/*"]);
    }
}
