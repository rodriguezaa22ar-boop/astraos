use std::path::{Component, Path};

const GENERATED_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".cache",
    ".next",
    ".tox",
    ".venv",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "target",
    "venv",
    "vendor",
];

const SENSITIVE_NAMES: &[&str] = &[
    ".env",
    ".npmrc",
    ".pypirc",
    ".netrc",
    "credentials",
    "credentials.json",
    "credentials.toml",
    "credentials.yaml",
    "credentials.yml",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    "id_rsa",
    "secrets",
    ".secrets",
    "secrets.json",
    "secrets.toml",
    "secrets.yaml",
    "secrets.yml",
    ".envrc",
    "terraform.tfstate",
];

pub(crate) fn excluded_path(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(value) = component else {
            return false;
        };
        value
            .to_str()
            .map(str::to_ascii_lowercase)
            .is_some_and(|name| GENERATED_DIRECTORIES.contains(&name.as_str()))
    })
}

pub(crate) fn sensitive_path(path: &Path) -> bool {
    let sensitive_component = path.components().any(|component| {
        let Component::Normal(value) = component else {
            return false;
        };
        let Some(name) = value.to_str() else {
            return true;
        };
        let lower = name.to_ascii_lowercase();
        let documented_environment_example = matches!(
            lower.as_str(),
            ".env.example" | ".env.sample" | ".env.template"
        );
        SENSITIVE_NAMES.contains(&lower.as_str())
            || (lower.starts_with(".env.") && !documented_environment_example)
    });

    sensitive_component
        || matches!(
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("key" | "p12" | "pem")
        )
}

pub(crate) fn normalized_relative(path: &Path) -> Option<String> {
    let mut values = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => values.push(value.to_str()?.to_string()),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(values.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_generated_and_sensitive_paths() {
        assert!(excluded_path(Path::new("packages/web/node_modules/a.js")));
        assert!(excluded_path(Path::new(".git/config")));
        assert!(sensitive_path(Path::new(".env")));
        assert!(sensitive_path(Path::new(".env.local/leak.py")));
        assert!(sensitive_path(Path::new("secrets/token.rs")));
        assert!(sensitive_path(Path::new("keys/private.pem")));
        assert!(!sensitive_path(Path::new(".env.example")));
        assert!(!excluded_path(Path::new(".github/workflows/ci.yml")));
    }

    #[test]
    fn relative_paths_use_forward_slashes() {
        assert_eq!(
            normalized_relative(Path::new("src/example/main.rs")),
            Some("src/example/main.rs".to_string())
        );
    }
}
