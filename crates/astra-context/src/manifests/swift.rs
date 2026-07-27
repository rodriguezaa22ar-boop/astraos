use super::{ParseContext, ParseResult};
use crate::{facts::ToolCategory, CommandPurpose};

pub(super) fn parse(context: &ParseContext<'_>) -> ParseResult {
    let mut result = ParseResult::default();
    result.tool(
        context,
        "swiftpm",
        ToolCategory::PackageManager,
        "swift.package_manager",
    );
    result.tool(
        context,
        "swiftpm",
        ToolCategory::BuildSystem,
        "swift.build_system",
    );
    result.tool(
        context,
        "swift-test",
        ToolCategory::TestingFramework,
        "swift.test_harness",
    );
    result.command(
        context,
        "swift",
        &["build"],
        CommandPurpose::Build,
        None,
        "swift.command",
    );
    result.command(
        context,
        "swift",
        &["test"],
        CommandPurpose::Test,
        None,
        "swift.command",
    );

    if let Some(package) = package_initializer(context.text) {
        if let Some(name) = string_argument(package, "name") {
            result.package(context, &name, "swift.package");
        }
    }
    result
}

fn package_initializer(source: &str) -> Option<&str> {
    let start = source.find("Package(")?;
    Some(&source[start + "Package(".len()..])
}

fn string_argument(source: &str, name: &str) -> Option<String> {
    let start = source.find(name)?;
    let value = source[start + name.len()..].trim_start();
    let value = value.strip_prefix(':')?.trim_start();
    let quote = value.chars().next()?;
    if !matches!(quote, '"' | '\'') {
        return None;
    }
    let rest = &value[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::Fact;
    use std::collections::BTreeSet;

    #[test]
    fn extracts_only_the_literal_package_name() {
        let paths = BTreeSet::new();
        let context = ParseContext {
            path: "Package.swift",
            root: ".",
            text: r#"let package = Package(name: "Example", products: [])"#,
            inventory_paths: &paths,
            package_manager: None,
        };
        let parsed = parse(&context);
        assert!(parsed
            .facts
            .iter()
            .any(|fact| matches!(&fact.fact, Fact::Package(package) if package.name == "Example")));
    }
}
