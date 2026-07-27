use super::{clean_identifier, safe_requirement, ParseContext, ParseResult};
use crate::{facts::ToolCategory, CommandPurpose, DependencyScope};
use toml::Value;

pub(super) fn parse(context: &ParseContext<'_>) -> ParseResult {
    let mut result = ParseResult::default();
    let document = match toml::from_str::<Value>(context.text) {
        Ok(document) => document,
        Err(_) => {
            result.warn(
                "manifest.parse_failed",
                "pyproject.toml could not be parsed as TOML",
            );
            return result;
        }
    };
    let project = document.get("project").and_then(Value::as_table);
    let poetry = document
        .get("tool")
        .and_then(Value::as_table)
        .and_then(|tool| tool.get("poetry"))
        .and_then(Value::as_table);
    let package_name = project
        .and_then(|table| table.get("name"))
        .and_then(Value::as_str)
        .or_else(|| {
            poetry
                .and_then(|table| table.get("name"))
                .and_then(Value::as_str)
        })
        .and_then(clean_identifier);
    if let Some(name) = &package_name {
        result.package(context, name, "python.package");
    }
    let owner = package_name
        .clone()
        .unwrap_or_else(|| context.root.to_string());

    let uv = document
        .get("tool")
        .and_then(Value::as_table)
        .and_then(|tool| tool.get("uv"))
        .and_then(Value::as_table);
    let manager = if uv.is_some() || context.package_manager == Some("uv") {
        "uv"
    } else if poetry.is_some() || context.package_manager == Some("poetry") {
        "poetry"
    } else if context.package_manager == Some("pipenv") {
        "pipenv"
    } else {
        "pip"
    };
    result.tool(
        context,
        manager,
        ToolCategory::PackageManager,
        "python.package_manager",
    );

    if let Some(workspace) = uv
        .and_then(|table| table.get("workspace"))
        .and_then(Value::as_table)
    {
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
        result.workspace(context, "uv", members, "python.uv_workspace");
    }

    if let Some(dependencies) = project
        .and_then(|table| table.get("dependencies"))
        .and_then(Value::as_array)
    {
        parse_pep508_array(
            context,
            &mut result,
            &owner,
            dependencies,
            DependencyScope::Runtime,
            "project.dependencies",
        );
    }
    if let Some(groups) = project
        .and_then(|table| table.get("optional-dependencies"))
        .and_then(Value::as_table)
    {
        for (group, dependencies) in groups {
            if let Some(dependencies) = dependencies.as_array() {
                parse_pep508_array(
                    context,
                    &mut result,
                    &owner,
                    dependencies,
                    DependencyScope::Optional,
                    &format!("project.optional-dependencies.{group}"),
                );
            }
        }
    }
    if let Some(groups) = document.get("dependency-groups").and_then(Value::as_table) {
        for (group, dependencies) in groups {
            if let Some(dependencies) = dependencies.as_array() {
                parse_pep508_array(
                    context,
                    &mut result,
                    &owner,
                    dependencies,
                    DependencyScope::Development,
                    &format!("dependency-groups.{group}"),
                );
            }
        }
    }
    if let Some(requirements) = document
        .get("build-system")
        .and_then(Value::as_table)
        .and_then(|table| table.get("requires"))
        .and_then(Value::as_array)
    {
        parse_pep508_array(
            context,
            &mut result,
            &owner,
            requirements,
            DependencyScope::Build,
            "build-system.requires",
        );
    }
    parse_poetry_dependencies(context, &mut result, poetry, &owner);

    if let Some(backend) = document
        .get("build-system")
        .and_then(Value::as_table)
        .and_then(|table| table.get("build-backend"))
        .and_then(Value::as_str)
    {
        if let Some(tool) = backend_tool(backend) {
            result.tool(
                context,
                tool,
                ToolCategory::BuildSystem,
                "python.build_backend",
            );
        }
    }

    let has_pytest_table = document
        .get("tool")
        .and_then(Value::as_table)
        .is_some_and(|tool| tool.contains_key("pytest"));
    let has_pytest_dependency = result.facts.iter().any(|fact| {
        matches!(
            &fact.fact,
            crate::facts::Fact::Dependency(dependency) if dependency.name == "pytest"
        )
    });
    if has_pytest_table || has_pytest_dependency {
        result.tool(
            context,
            "pytest",
            ToolCategory::TestingFramework,
            "python.pytest",
        );
        match manager {
            "uv" => result.command(
                context,
                "uv",
                &["run", "pytest"],
                CommandPurpose::Test,
                None,
                "python.pytest_command",
            ),
            "poetry" => result.command(
                context,
                "poetry",
                &["run", "pytest"],
                CommandPurpose::Test,
                None,
                "python.pytest_command",
            ),
            _ => result.command(
                context,
                "python",
                &["-m", "pytest"],
                CommandPurpose::Test,
                None,
                "python.pytest_command",
            ),
        }
    }

    if let Some(scripts) = project
        .and_then(|table| table.get("scripts"))
        .and_then(Value::as_table)
    {
        parse_scripts(context, &mut result, scripts, "project.scripts");
    }
    if let Some(scripts) = poetry
        .and_then(|table| table.get("scripts"))
        .and_then(Value::as_table)
    {
        parse_scripts(context, &mut result, scripts, "tool.poetry.scripts");
    }

    if let Some(license) = project.and_then(|table| table.get("license")) {
        match license {
            Value::String(value) => result.license(
                context,
                value,
                "project.license".to_string(),
                "python.license",
            ),
            Value::Table(value) => {
                if let Some(text) = value.get("text").and_then(Value::as_str) {
                    result.license(
                        context,
                        text,
                        "project.license.text".to_string(),
                        "python.license",
                    );
                }
                if let Some(path) = value.get("file").and_then(Value::as_str) {
                    result.license_file(
                        context,
                        path,
                        "project.license.file".to_string(),
                        "python.license_file",
                    );
                }
            }
            _ => {}
        }
    }
    if let Some(license) = poetry
        .and_then(|table| table.get("license"))
        .and_then(Value::as_str)
    {
        result.license(
            context,
            license,
            "tool.poetry.license".to_string(),
            "python.license",
        );
    }
    result
}

fn parse_pep508_array(
    context: &ParseContext<'_>,
    result: &mut ParseResult,
    owner: &str,
    values: &[Value],
    scope: DependencyScope,
    prefix: &str,
) {
    for (index, value) in values.iter().enumerate() {
        let Some(value) = value.as_str() else {
            continue;
        };
        let Some((name, requirement)) = pep508(value) else {
            continue;
        };
        result.dependency(
            context,
            owner,
            &name,
            requirement,
            scope,
            format!("{prefix}[{index}]"),
        );
    }
}

fn parse_poetry_dependencies(
    context: &ParseContext<'_>,
    result: &mut ParseResult,
    poetry: Option<&toml::map::Map<String, Value>>,
    owner: &str,
) {
    if let Some(dependencies) = poetry
        .and_then(|table| table.get("dependencies"))
        .and_then(Value::as_table)
    {
        parse_poetry_table(
            context,
            result,
            owner,
            dependencies,
            DependencyScope::Runtime,
            "tool.poetry.dependencies",
        );
    }
    if let Some(dependencies) = poetry
        .and_then(|table| table.get("dev-dependencies"))
        .and_then(Value::as_table)
    {
        parse_poetry_table(
            context,
            result,
            owner,
            dependencies,
            DependencyScope::Development,
            "tool.poetry.dev-dependencies",
        );
    }
    if let Some(groups) = poetry
        .and_then(|table| table.get("group"))
        .and_then(Value::as_table)
    {
        for (group, group_value) in groups {
            let Some(dependencies) = group_value
                .as_table()
                .and_then(|table| table.get("dependencies"))
                .and_then(Value::as_table)
            else {
                continue;
            };
            parse_poetry_table(
                context,
                result,
                owner,
                dependencies,
                DependencyScope::Development,
                &format!("tool.poetry.group.{group}.dependencies"),
            );
        }
    }
}

fn parse_poetry_table(
    context: &ParseContext<'_>,
    result: &mut ParseResult,
    owner: &str,
    dependencies: &toml::map::Map<String, Value>,
    scope: DependencyScope,
    prefix: &str,
) {
    for (name, value) in dependencies {
        if name.eq_ignore_ascii_case("python") {
            continue;
        }
        let requirement = match value {
            Value::String(value) => Some(value.clone()),
            Value::Table(value) => value
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_string),
            _ => None,
        }
        .and_then(safe_requirement);
        result.dependency(
            context,
            owner,
            name,
            requirement,
            scope,
            format!("{prefix}.{name}"),
        );
    }
}

fn pep508(value: &str) -> Option<(String, Option<String>)> {
    let value = value.split(';').next()?.trim();
    let name_end = value
        .char_indices()
        .find(|(_, character)| {
            !character.is_ascii_alphanumeric() && !matches!(character, '-' | '_' | '.')
        })
        .map_or(value.len(), |(index, _)| index);
    let name = clean_identifier(&value[..name_end])?;
    let suffix = value[name_end..].trim();
    if suffix.starts_with('[') {
        let close = suffix.find(']')?;
        let suffix = suffix[close + 1..].trim();
        if let Some((_, direct)) = suffix.split_once('@') {
            let _ = direct;
            return Some((name, None));
        }
        return Some((name, safe_requirement(suffix.to_string())));
    }
    if let Some((_, direct)) = suffix.split_once('@') {
        let _ = direct;
        return Some((name, None));
    }
    Some((name, safe_requirement(suffix.to_string())))
}

fn backend_tool(backend: &str) -> Option<&'static str> {
    if backend.starts_with("hatchling") {
        Some("hatchling")
    } else if backend.starts_with("setuptools") {
        Some("setuptools")
    } else if backend.starts_with("flit") {
        Some("flit")
    } else if backend.starts_with("poetry") {
        Some("poetry")
    } else if backend.starts_with("maturin") {
        Some("maturin")
    } else if backend.starts_with("pdm") {
        Some("pdm")
    } else {
        None
    }
}

fn parse_scripts(
    context: &ParseContext<'_>,
    result: &mut ParseResult,
    scripts: &toml::map::Map<String, Value>,
    prefix: &str,
) {
    for (name, value) in scripts {
        let value = match value {
            Value::String(value) => Some(value.as_str()),
            Value::Table(value) => value.get("callable").and_then(Value::as_str),
            _ => None,
        };
        let Some(value) = value else {
            continue;
        };
        let module = value.split(':').next().unwrap_or(value).trim();
        if module.is_empty() {
            continue;
        }
        let relative = format!("{}.py", module.replace('.', "/"));
        let candidates = [relative.clone(), format!("src/{relative}")];
        let Some(path) = candidates.iter().find(|path| {
            context
                .resolve(path)
                .is_some_and(|path| context.has_path(&path))
        }) else {
            continue;
        };
        result.entry(
            context,
            path,
            "script",
            "python",
            format!("{prefix}.{name}"),
            "python.script",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::Fact;
    use std::collections::BTreeSet;

    #[test]
    fn parses_pep621_groups_backend_and_script() {
        let paths = BTreeSet::from(["src/demo.py".to_string()]);
        let context = ParseContext {
            path: "pyproject.toml",
            root: ".",
            text: r#"
                [project]
                name = "demo"
                dependencies = ["requests>=2"]
                [project.scripts]
                demo = "demo:main"
                [dependency-groups]
                dev = ["pytest>=8"]
                [build-system]
                requires = ["hatchling"]
                build-backend = "hatchling.build"
                [tool.pytest.ini_options]
                testpaths = ["tests"]
            "#,
            inventory_paths: &paths,
            package_manager: Some("uv"),
        };
        let parsed = parse(&context);
        assert!(parsed.facts.iter().any(|fact| matches!(
            &fact.fact,
            Fact::Dependency(dependency)
                if dependency.name == "pytest"
                    && dependency.scope == DependencyScope::Development
        )));
        assert!(parsed.facts.iter().any(|fact| matches!(
            &fact.fact,
            Fact::Marker(marker)
                if marker.kind == crate::facts::MarkerKind::EntryPoint
                    && marker.path == "src/demo.py"
        )));
    }

    #[test]
    fn direct_urls_are_not_retained() {
        let (name, requirement) =
            pep508("demo @ https://example.invalid/archive.whl").expect("dependency");
        assert_eq!(name, "demo");
        assert_eq!(requirement, None);
    }
}
