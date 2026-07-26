use astra_config::Config;
use std::{
    env,
    path::{Component, Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("invalid workspace name: {0}")]
    InvalidName(String),

    #[error("could not resolve workspace path: {0}")]
    Path(String),

    #[error("workspace already exists: {0}")]
    AlreadyExists(String),

    #[error("unknown workspace: {0}")]
    Unknown(String),
}

pub fn astra_root(config: &Config) -> PathBuf {
    PathBuf::from(&config.workspace.root)
}

pub fn workspace_path(config: &Config, name: &str) -> Option<PathBuf> {
    config.workspaces.get(name).map(PathBuf::from)
}

pub fn list_workspaces(config: &Config) -> Vec<(String, String)> {
    config
        .workspaces
        .iter()
        .map(|(name, path)| (name.clone(), path.clone()))
        .collect()
}

pub fn valid_workspace_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

pub fn normalize_workspace_path(path: &str) -> Result<String, WorkspaceError> {
    let expanded = if let Some(stripped) = path.strip_prefix('~') {
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());

        if stripped.is_empty() {
            home
        } else if stripped.starts_with('/') {
            format!("{home}{stripped}")
        } else {
            format!("{home}/{stripped}")
        }
    } else {
        path.to_string()
    };

    let absolute = if Path::new(&expanded).is_absolute() {
        PathBuf::from(&expanded)
    } else {
        env::current_dir()
            .map_err(|error| WorkspaceError::Path(error.to_string()))?
            .join(&expanded)
    };

    let mut normalized = PathBuf::new();

    for component in absolute.components() {
        match component {
            Component::CurDir => {}

            Component::ParentDir => {
                normalized.pop();
            }

            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    Ok(normalized.to_string_lossy().into_owned())
}

pub fn add_workspace(
    config: &mut Config,
    name: &str,
    path: &str,
    force: bool,
) -> Result<(), WorkspaceError> {
    if !valid_workspace_name(name) {
        return Err(WorkspaceError::InvalidName(name.to_string()));
    }

    if config.workspaces.contains_key(name) && !force {
        return Err(WorkspaceError::AlreadyExists(name.to_string()));
    }

    let normalized_path = normalize_workspace_path(path)?;

    config.workspaces.insert(name.to_string(), normalized_path);

    Ok(())
}

pub fn remove_workspace(config: &mut Config, name: &str) -> Result<(), WorkspaceError> {
    if config.workspaces.remove(name).is_none() {
        return Err(WorkspaceError::Unknown(name.to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use astra_config::{AiConfig, Config, CyberConfig, EditorConfig, WorkspaceConfig};
    use std::{collections::BTreeMap, env, ffi::OsString, path::PathBuf};

    struct EnvironmentGuard {
        original_home: Option<OsString>,
        original_current_dir: PathBuf,
    }

    impl EnvironmentGuard {
        fn capture() -> Self {
            Self {
                original_home: env::var_os("HOME"),
                original_current_dir: env::current_dir()
                    .expect("current directory should be available"),
            }
        }
    }

    impl Drop for EnvironmentGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.original_current_dir);

            match &self.original_home {
                Some(home) => env::set_var("HOME", home),
                None => env::remove_var("HOME"),
            }
        }
    }

    fn sample_config() -> Config {
        Config {
            workspace: WorkspaceConfig {
                root: "/tmp/workspaces".to_string(),
            },
            editor: EditorConfig {
                command: "code".to_string(),
            },
            ai: AiConfig {
                provider: "ollama".to_string(),
            },
            cyber: CyberConfig {
                labs: "/tmp/cyber".to_string(),
            },
            workspaces: BTreeMap::new(),
        }
    }

    #[test]
    fn accepts_valid_workspace_names() {
        assert!(valid_workspace_name("alpha"));
        assert!(valid_workspace_name("my-workspace_01"));
    }

    #[test]
    fn rejects_invalid_workspace_names() {
        assert!(!valid_workspace_name(""));
        assert!(!valid_workspace_name("bad name"));
        assert!(!valid_workspace_name("../danger"));
    }

    #[test]
    fn expands_and_normalizes_paths_from_home_and_current_directory() {
        let _environment_guard = EnvironmentGuard::capture();

        let temp_home = tempfile::tempdir().expect("temporary HOME should be created");
        let temp_dir = tempfile::tempdir().expect("temporary current directory should be created");

        env::set_var("HOME", temp_home.path());
        env::set_current_dir(temp_dir.path())
            .expect("temporary current directory should be accessible");

        let normalized_home =
            normalize_workspace_path("~/projects/demo").expect("HOME path should normalize");

        let expected_home = temp_home.path().join("projects/demo");

        assert_eq!(PathBuf::from(normalized_home), expected_home);

        let normalized_relative =
            normalize_workspace_path("./nested/workspace").expect("relative path should normalize");

        let expected_relative = env::current_dir()
            .expect("current directory should be available")
            .join("nested/workspace");

        assert_eq!(PathBuf::from(normalized_relative), expected_relative);
    }

    #[test]
    fn add_and_remove_workspaces() {
        let mut config = sample_config();

        add_workspace(&mut config, "alpha", "/tmp/alpha", false)
            .expect("workspace should be added");

        assert!(config.workspaces.contains_key("alpha"));

        remove_workspace(&mut config, "alpha").expect("workspace should be removed");

        assert!(!config.workspaces.contains_key("alpha"));
    }
}
