use super::{clean_identifier, workspace_owner, ParseContext, ParseResult};
use crate::{facts::ToolCategory, CommandPurpose, DependencyScope};

pub(super) fn parse(context: &ParseContext<'_>, workspace: bool) -> ParseResult {
    if workspace {
        parse_workspace(context)
    } else {
        parse_module(context)
    }
}

fn parse_module(context: &ParseContext<'_>) -> ParseResult {
    let mut result = ParseResult::default();
    let mut module = None;
    let mut dependencies = Vec::new();
    let mut in_require = false;

    for raw_line in context.text.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if !in_require {
            if let Some(value) = directive(trimmed, "module") {
                module = value.split_whitespace().next().and_then(clean_identifier);
            } else if trimmed == "require (" {
                in_require = true;
            } else if let Some(value) = directive(trimmed, "require") {
                if let Some(dependency) = parse_requirement(value) {
                    dependencies.push(dependency);
                }
            }
        } else if trimmed == ")" {
            in_require = false;
        } else if let Some(dependency) = parse_requirement(trimmed) {
            dependencies.push(dependency);
        }
    }

    if in_require {
        result.warn(
            "manifest.parse_failed",
            "go.mod has an unterminated require block",
        );
    }
    if module.is_none() {
        result.warn(
            "manifest.parse_failed",
            "go.mod does not contain a module directive",
        );
    }
    if let Some(module) = &module {
        result.package(context, module, "go.module");
    }
    let owner = module.unwrap_or_else(|| workspace_owner(context.root));
    for (name, version) in dependencies {
        result.dependency(
            context,
            &owner,
            &name,
            Some(version),
            DependencyScope::Runtime,
            format!("require.{name}"),
        );
    }

    add_go_tools_and_commands(context, &mut result);
    result
}

fn parse_workspace(context: &ParseContext<'_>) -> ParseResult {
    let mut result = ParseResult::default();
    let mut members = Vec::new();
    let mut in_use = false;
    for raw_line in context.text.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if !in_use {
            if trimmed == "use (" {
                in_use = true;
            } else if let Some(value) = directive(trimmed, "use") {
                if let Some(value) = value.split_whitespace().next() {
                    members.push(value.to_string());
                }
            }
        } else if trimmed == ")" {
            in_use = false;
        } else if let Some(value) = trimmed.split_whitespace().next() {
            members.push(value.to_string());
        }
    }
    if in_use {
        result.warn(
            "manifest.parse_failed",
            "go.work has an unterminated use block",
        );
    }
    result.workspace(context, "go", members, "go.workspace");
    add_go_tools_and_commands(context, &mut result);
    result
}

fn directive<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let value = line.strip_prefix(name)?;
    value
        .starts_with(char::is_whitespace)
        .then(|| value.trim_start())
}

fn parse_requirement(line: &str) -> Option<(String, String)> {
    if line.contains("// indirect") {
        return None;
    }
    let declaration = line.split("//").next()?.trim();
    let mut parts = declaration.split_whitespace();
    let name = parts.next()?.to_string();
    let version = parts.next()?.to_string();
    Some((name, version))
}

fn add_go_tools_and_commands(context: &ParseContext<'_>, result: &mut ParseResult) {
    result.tool(context, "go", ToolCategory::PackageManager, "go.tool");
    result.tool(context, "go", ToolCategory::BuildSystem, "go.tool");
    result.tool(
        context,
        "go-test",
        ToolCategory::TestingFramework,
        "go.test_harness",
    );
    result.command(
        context,
        "go",
        &["build", "./..."],
        CommandPurpose::Build,
        None,
        "go.command",
    );
    result.command(
        context,
        "go",
        &["test", "./..."],
        CommandPurpose::Test,
        None,
        "go.command",
    );
    result.command(
        context,
        "go",
        &["vet", "./..."],
        CommandPurpose::Validate,
        None,
        "go.command",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::Fact;
    use std::collections::BTreeSet;

    #[test]
    fn parses_direct_requirements_but_not_indirect_ones() {
        let paths = BTreeSet::new();
        let context = ParseContext {
            path: "go.mod",
            root: ".",
            text: r#"
                module example.invalid/demo
                go 1.23
                require (
                    example.invalid/direct v1.2.3
                    example.invalid/transitive v2.0.0 // indirect
                )
            "#,
            inventory_paths: &paths,
            package_manager: None,
        };
        let parsed = parse(&context, false);
        let dependencies = parsed
            .facts
            .iter()
            .filter_map(|fact| match &fact.fact {
                Fact::Dependency(dependency) => Some(dependency.name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(dependencies, vec!["example.invalid/direct"]);
    }

    #[test]
    fn reports_unclosed_blocks_without_dropping_prior_facts() {
        let paths = BTreeSet::new();
        let context = ParseContext {
            path: "go.mod",
            root: ".",
            text: "module example.invalid/demo\nrequire (\n example.invalid/dep v1.0.0\n",
            inventory_paths: &paths,
            package_manager: None,
        };
        let parsed = parse(&context, false);
        assert_eq!(parsed.diagnostics.len(), 1);
        assert!(parsed
            .facts
            .iter()
            .any(|fact| matches!(fact.fact, Fact::Dependency(_))));
    }
}
