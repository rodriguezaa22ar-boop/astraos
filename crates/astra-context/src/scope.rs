#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SemanticScope {
    Primary,
    Fixture,
    Example,
}

pub(crate) fn classify_project_path(path: &str) -> SemanticScope {
    let mut components = path
        .split('/')
        .filter(|component| !component.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    components.pop();

    for (index, component) in components.iter().enumerate() {
        if matches!(
            component.as_str(),
            "testdata" | "test-data" | "test_fixtures" | "test-fixtures"
        ) {
            return SemanticScope::Fixture;
        }
        if component == "fixtures"
            && (index == 0 || components[..index].iter().any(|value| test_boundary(value)))
        {
            return SemanticScope::Fixture;
        }
        if fixture_container(component)
            && components[..index].iter().any(|value| test_boundary(value))
        {
            return SemanticScope::Fixture;
        }
    }

    if components.iter().any(|component| {
        matches!(
            component.as_str(),
            "example" | "examples" | "sample" | "samples"
        )
    }) {
        SemanticScope::Example
    } else {
        SemanticScope::Primary
    }
}

fn test_boundary(component: &str) -> bool {
    matches!(
        component,
        "test"
            | "tests"
            | "testing"
            | "test-support"
            | "test_support"
            | "__tests__"
            | "spec"
            | "specs"
    )
}

fn fixture_container(component: &str) -> bool {
    matches!(
        component,
        "fixture"
            | "fixtures"
            | "data"
            | "resources"
            | "snapshot"
            | "snapshots"
            | "__snapshots__"
            | "golden"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_nested_fixture_and_example_boundaries() {
        for path in [
            "fixtures/project/Cargo.toml",
            "tests/fixtures/project/Cargo.toml",
            "test/fixtures/project/package.json",
            "crates/demo/tests/support/fixtures/input.json",
            "crates/demo/test-support/fixtures/input.json",
            "src/test/resources/application.yml",
            "pkg/testdata/input.txt",
        ] {
            assert_eq!(
                classify_project_path(path),
                SemanticScope::Fixture,
                "{path}"
            );
        }
        assert_eq!(
            classify_project_path("examples/demo/src/main.rs"),
            SemanticScope::Example
        );
        assert_eq!(
            classify_project_path("samples/demo/src/main.rs"),
            SemanticScope::Example
        );
    }

    #[test]
    fn ordinary_tests_and_a_selected_fixture_root_remain_primary() {
        for path in ["tests/api.rs", "src/tests.rs", "Cargo.toml", "src/main.rs"] {
            assert_eq!(
                classify_project_path(path),
                SemanticScope::Primary,
                "{path}"
            );
        }
    }
}
