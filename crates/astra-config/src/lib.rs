mod defaults;
mod error;
mod loader;
mod model;
mod paths;

pub use error::ConfigError;
pub use loader::{load, save};
pub use model::{
    AiConfig, Config, CyberConfig, EditorConfig, PaneLayout, SplitDirection, TabLayout,
    TerminalConfig, WorkspaceConfig, WorkspaceLayout,
};
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

    #[test]
    fn existing_toml_without_terminal_fields_remains_compatible() {
        let config: Config = toml::from_str(
            r#"
[workspace]
root = "/tmp/workspaces"

[editor]
command = "code"

[ai]
provider = "ollama"

[cyber]
labs = "/tmp/cyber"

[workspaces]
alpha = "/tmp/alpha"
"#,
        )
        .expect("milestone 6 configuration should deserialize");

        assert_eq!(config.terminal.command, "wezterm");
        assert!(config.workspace_layouts.is_empty());
        assert_eq!(config.workspaces["alpha"], "/tmp/alpha");
    }

    #[test]
    fn empty_workspace_layout_map_round_trips() {
        let config = Config::default();
        let rendered = toml::to_string(&config).expect("configuration should serialize");
        let reparsed: Config =
            toml::from_str(&rendered).expect("serialized configuration should deserialize");

        assert!(reparsed.workspace_layouts.is_empty());
        assert_eq!(reparsed.terminal.command, "wezterm");
    }

    #[test]
    fn terminal_layout_fields_round_trip_without_command_flattening() {
        let config: Config = toml::from_str(
            r#"
[workspace]
root = "/tmp/workspaces"

[editor]
command = "/Applications/Visual Studio Code.app/Contents/MacOS/Electron"

[ai]
provider = "ollama"

[cyber]
labs = "/tmp/cyber"

[terminal]
command = "/Applications/WezTerm Dev.app/Contents/MacOS/wezterm"

[workspaces]
project = "/tmp/project with spaces"

[workspace_layouts.rust-development]
editor = true
ollama = false

[[workspace_layouts.rust-development.tabs]]
name = "development"
command = ["cargo", "watch", "-x", "check all"]

[[workspace_layouts.rust-development.tabs.panes]]
target = 0
direction = "right"
percent = 45
command = ["git", "status", "--short"]
"#,
        )
        .expect("milestone 7 configuration should deserialize");

        let rendered = toml::to_string(&config).expect("configuration should serialize");
        let reparsed: Config =
            toml::from_str(&rendered).expect("serialized configuration should deserialize");
        let layout = &reparsed.workspace_layouts["rust-development"];

        assert_eq!(
            reparsed.terminal.command,
            "/Applications/WezTerm Dev.app/Contents/MacOS/wezterm"
        );
        assert_eq!(layout.tabs[0].command[3], "check all");
        assert_eq!(layout.tabs[0].panes[0].command[1], "status");
    }
}
