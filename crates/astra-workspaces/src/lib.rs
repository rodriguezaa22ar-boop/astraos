use astra_config::Config;
use std::path::PathBuf;

pub fn astra_root(config: &Config) -> PathBuf {
    PathBuf::from(&config.workspace.root)
}

pub fn workspace_path(config: &Config, name: &str) -> Option<PathBuf> {
    config.workspaces.get(name).map(PathBuf::from)
}

pub fn list_workspaces(config: &Config) -> Vec<(&str, &str)> {
    config
        .workspaces
        .iter()
        .map(|(name, path)| (name.as_str(), path.as_str()))
        .collect()
}

pub fn add_workspace(config: &mut Config, name: String, path: String) -> bool {
    config.workspaces.insert(name, path).is_none()
}

pub fn remove_workspace(config: &mut Config, name: &str) -> bool {
    config.workspaces.remove(name).is_some()
}
