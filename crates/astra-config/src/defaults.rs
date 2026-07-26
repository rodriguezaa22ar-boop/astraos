use crate::model::{AiConfig, Config, CyberConfig, EditorConfig, TerminalConfig, WorkspaceConfig};
use std::collections::BTreeMap;

impl Default for Config {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());

        let mut workspaces = BTreeMap::new();

        workspaces.insert(
            "astraos".to_string(),
            format!("{home}/Developer/projects/astraos"),
        );

        workspaces.insert(
            "omnia".to_string(),
            format!("{home}/Developer/astraeus-omnia"),
        );

        workspaces.insert(
            "api".to_string(),
            format!("{home}/Developer/omnia-api-foundry"),
        );

        workspaces.insert("games".to_string(), format!("{home}/Developer/games"));

        workspaces.insert(
            "cyber".to_string(),
            format!("{home}/Developer/cybersecurity"),
        );

        workspaces.insert("ai".to_string(), format!("{home}/Developer/ai"));

        workspaces.insert("learning".to_string(), format!("{home}/Developer/learning"));

        Self {
            workspace: WorkspaceConfig {
                root: format!("{home}/Developer"),
            },
            editor: EditorConfig {
                command: "code".to_string(),
            },
            ai: AiConfig {
                provider: "ollama".to_string(),
            },
            cyber: CyberConfig {
                labs: format!("{home}/Developer/cybersecurity/labs"),
            },
            workspaces,
            terminal: TerminalConfig::default(),
            workspace_layouts: BTreeMap::new(),
        }
    }
}
