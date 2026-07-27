use super::{workspace_owner, ParseContext, ParseResult};
use crate::{facts::ToolCategory, CommandPurpose, DependencyScope};

pub(super) fn parse(context: &ParseContext<'_>, maven: bool, gradle_settings: bool) -> ParseResult {
    if maven {
        parse_maven(context)
    } else {
        parse_gradle(context, gradle_settings)
    }
}

fn parse_maven(context: &ParseContext<'_>) -> ParseResult {
    let mut result = ParseResult::default();
    add_maven_tools_and_commands(context, &mut result);
    let source = strip_xml_comments(context.text);
    if source.contains("<project") && !source.contains("</project>") {
        result.warn(
            "manifest.parse_failed",
            "pom.xml has no closing project element",
        );
        return result;
    }

    let package_name = tag_value(&source, "artifactId");
    if let Some(name) = &package_name {
        result.package(context, name, "maven.project");
    }
    let owner = package_name.unwrap_or_else(|| workspace_owner(context.root));

    let members = tag_blocks(&source, "modules")
        .into_iter()
        .flat_map(|block| tag_values(block, "module"))
        .collect::<Vec<_>>();
    if !members.is_empty() {
        result.workspace(context, "maven", members, "maven.modules");
    }

    for (index, dependency) in tag_blocks(&source, "dependency").into_iter().enumerate() {
        let Some(artifact) = tag_value(dependency, "artifactId") else {
            continue;
        };
        let group = tag_value(dependency, "groupId");
        let name = group.map_or_else(|| artifact.clone(), |group| format!("{group}:{artifact}"));
        let requirement = tag_value(dependency, "version");
        let optional = tag_value(dependency, "optional").is_some_and(|value| value == "true");
        let scope = if optional {
            DependencyScope::Optional
        } else {
            match tag_value(dependency, "scope").as_deref() {
                Some("test") => DependencyScope::Development,
                Some("provided") | Some("system") => DependencyScope::Build,
                _ => DependencyScope::Runtime,
            }
        };
        result.dependency(
            context,
            &owner,
            &name,
            requirement,
            scope,
            format!("dependencies[{index}]"),
        );
        if is_jvm_test_dependency(&name) {
            result.tool(
                context,
                testing_tool(&name),
                ToolCategory::TestingFramework,
                "maven.testing_framework",
            );
        }
    }
    if let Some(license) = tag_blocks(&source, "licenses")
        .into_iter()
        .flat_map(|block| tag_blocks(block, "license"))
        .find_map(|block| tag_value(block, "name"))
    {
        result.license(
            context,
            &license,
            "licenses.license.name".to_string(),
            "maven.license",
        );
    }
    result
}

fn parse_gradle(context: &ParseContext<'_>, settings: bool) -> ParseResult {
    let mut result = ParseResult::default();
    add_gradle_tools_and_commands(context, &mut result);
    if settings {
        let members = quoted_arguments_for_calls(context.text, "include")
            .into_iter()
            .map(|member| member.trim_start_matches(':').replace(':', "/"))
            .filter(|member| !member.is_empty())
            .collect();
        result.workspace(context, "gradle", members, "gradle.settings");
        if let Some(name) = gradle_assignment(context.text, "rootProject.name") {
            result.package(context, &name, "gradle.root_project");
        }
        return result;
    }

    let owner = workspace_owner(context.root);
    for (line_number, raw_line) in context.text.lines().enumerate() {
        let line = raw_line.split("//").next().unwrap_or(raw_line).trim();
        let Some((configuration, coordinate)) = gradle_dependency(line) else {
            continue;
        };
        let mut parts = coordinate.split(':');
        let Some(group) = parts.next() else {
            continue;
        };
        let Some(artifact) = parts.next() else {
            continue;
        };
        let name = format!("{group}:{artifact}");
        let requirement = parts.next().map(str::to_string);
        let scope = if configuration.to_ascii_lowercase().contains("test") {
            DependencyScope::Development
        } else if configuration.contains("annotationProcessor") || configuration.contains("kapt") {
            DependencyScope::Build
        } else if configuration.contains("compileOnly") {
            DependencyScope::Optional
        } else {
            DependencyScope::Runtime
        };
        result.dependency(
            context,
            &owner,
            &name,
            requirement,
            scope,
            format!("line:{}", line_number + 1),
        );
        if is_jvm_test_dependency(&name) {
            result.tool(
                context,
                testing_tool(&name),
                ToolCategory::TestingFramework,
                "gradle.testing_framework",
            );
        }
    }
    result
}

fn add_maven_tools_and_commands(context: &ParseContext<'_>, result: &mut ParseResult) {
    result.tool(context, "maven", ToolCategory::PackageManager, "maven.tool");
    result.tool(context, "maven", ToolCategory::BuildSystem, "maven.tool");
    let executable = context.package_manager.unwrap_or("mvn");
    for (argument, purpose) in [
        ("package", CommandPurpose::Build),
        ("test", CommandPurpose::Test),
        ("verify", CommandPurpose::Validate),
    ] {
        result.command(
            context,
            executable,
            &[argument],
            purpose,
            None,
            "maven.command",
        );
    }
}

fn add_gradle_tools_and_commands(context: &ParseContext<'_>, result: &mut ParseResult) {
    result.tool(
        context,
        "gradle",
        ToolCategory::PackageManager,
        "gradle.tool",
    );
    result.tool(context, "gradle", ToolCategory::BuildSystem, "gradle.tool");
    let executable = context.package_manager.unwrap_or("gradle");
    for (argument, purpose) in [
        ("build", CommandPurpose::Build),
        ("test", CommandPurpose::Test),
        ("check", CommandPurpose::Validate),
    ] {
        result.command(
            context,
            executable,
            &[argument],
            purpose,
            None,
            "gradle.command",
        );
    }
}

fn strip_xml_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find("<!--") {
        output.push_str(&rest[..start]);
        let Some(end) = rest[start + 4..].find("-->") else {
            return output;
        };
        rest = &rest[start + 4 + end + 3..];
    }
    output.push_str(rest);
    output
}

fn tag_value(source: &str, tag: &str) -> Option<String> {
    tag_values(source, tag).into_iter().next()
}

fn tag_values(source: &str, tag: &str) -> Vec<String> {
    tag_blocks(source, tag)
        .into_iter()
        .filter_map(|value| {
            let value = decode_xml_entities(value.trim());
            (!value.is_empty() && !value.contains('<')).then_some(value)
        })
        .collect()
}

fn tag_blocks<'a>(source: &'a str, tag: &str) -> Vec<&'a str> {
    let mut values = Vec::new();
    let mut rest = source;
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    while let Some(start) = rest.find(&open) {
        let after_start = &rest[start + open.len()..];
        let Some(open_end) = after_start.find('>') else {
            break;
        };
        let content = &after_start[open_end + 1..];
        let Some(close_start) = content.find(&close) else {
            break;
        };
        values.push(&content[..close_start]);
        rest = &content[close_start + close.len()..];
    }
    values
}

fn decode_xml_entities(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn quoted_arguments_for_calls(source: &str, call: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let line = line.split("//").next()?.trim();
            line.strip_prefix(call)
        })
        .flat_map(quoted_values)
        .collect()
}

fn quoted_values(source: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut quote = None;
    let mut start = 0;
    for (index, character) in source.char_indices() {
        match quote {
            None if matches!(character, '"' | '\'') => {
                quote = Some(character);
                start = index + character.len_utf8();
            }
            Some(open) if character == open => {
                values.push(source[start..index].to_string());
                quote = None;
            }
            _ => {}
        }
    }
    values
}

fn gradle_assignment(source: &str, key: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let line = line.split("//").next()?.trim();
        let value = line.strip_prefix(key)?.trim_start();
        let value = value.strip_prefix('=')?.trim();
        quoted_values(value).into_iter().next()
    })
}

fn gradle_dependency(line: &str) -> Option<(&str, String)> {
    let configuration_end = line
        .char_indices()
        .find(|(_, character)| character.is_whitespace() || *character == '(')
        .map(|(index, _)| index)?;
    let configuration = &line[..configuration_end];
    let supported = [
        "api",
        "implementation",
        "runtimeOnly",
        "compileOnly",
        "testImplementation",
        "testRuntimeOnly",
        "annotationProcessor",
        "kapt",
    ];
    if !supported.contains(&configuration) {
        return None;
    }
    let coordinate = quoted_values(&line[configuration_end..])
        .into_iter()
        .next()?;
    Some((configuration, coordinate))
}

fn is_jvm_test_dependency(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("junit") || lower.contains("testng")
}

fn testing_tool(name: &str) -> &'static str {
    if name.to_ascii_lowercase().contains("testng") {
        "testng"
    } else {
        "junit"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::Fact;
    use std::collections::BTreeSet;

    #[test]
    fn extracts_basic_maven_coordinates_without_external_xml_state() {
        let paths = BTreeSet::new();
        let context = ParseContext {
            path: "pom.xml",
            root: ".",
            text: r#"
                <project>
                  <groupId>dev.example</groupId><artifactId>demo</artifactId>
                  <dependencies><dependency>
                    <groupId>org.junit.jupiter</groupId>
                    <artifactId>junit-jupiter</artifactId><version>5.11</version>
                    <scope>test</scope>
                  </dependency></dependencies>
                </project>
            "#,
            inventory_paths: &paths,
            package_manager: None,
        };
        let parsed = parse(&context, true, false);
        assert!(parsed.facts.iter().any(|fact| matches!(
            &fact.fact,
            Fact::Dependency(dependency)
                if dependency.name == "org.junit.jupiter:junit-jupiter"
                    && dependency.scope == DependencyScope::Development
        )));
    }

    #[test]
    fn extracts_literal_gradle_includes_and_dependencies() {
        assert_eq!(
            quoted_arguments_for_calls("include(\":app\", \":libs:core\")", "include"),
            vec![":app", ":libs:core"]
        );
        assert_eq!(
            gradle_dependency("implementation(\"org.example:library:1.2\")"),
            Some(("implementation", "org.example:library:1.2".to_string()))
        );
    }
}
