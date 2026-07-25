use crate::model::{AiConfig, Config, CyberConfig, EditorConfig, WorkspaceConfig};

impl Default for Config {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());

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
        }
    }
}
