mod defaults;
mod error;
mod loader;
mod model;
mod paths;

pub use error::ConfigError;
pub use loader::{load, load_if_present, save};
pub use model::{AiConfig, Config, CyberConfig, EditorConfig, WorkspaceConfig};
pub use paths::{config_dir, config_path};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_workspace_contains_developer() {
        let config = Config::default();
        assert!(config.workspace.root.contains("Developer"));
    }

    #[test]
    fn config_path_ends_with_config_toml() {
        assert!(config_path().ends_with("config.toml"));
    }

    #[test]
    fn default_editor_is_code() {
        assert_eq!(Config::default().editor.command, "code");
    }

    #[test]
    fn default_ai_provider_is_ollama() {
        assert_eq!(Config::default().ai.provider, "ollama");
    }
}
