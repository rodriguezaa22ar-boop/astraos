use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub workspace: WorkspaceConfig,
    pub editor: EditorConfig,
    pub ai: AiConfig,
    pub cyber: CyberConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CyberConfig {
    pub labs: String,
}

impl Default for Config {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());

        Self {
            workspace: WorkspaceConfig {
                root: format!("{home}/Developer"),
            },
            editor: EditorConfig {
                command: "code".into(),
            },
            ai: AiConfig {
                provider: "ollama".into(),
            },
            cyber: CyberConfig {
                labs: format!("{home}/Developer/cybersecurity/labs"),
            },
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("astra")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn load() -> io::Result<Config> {
    let path = config_path();

    if !path.exists() {
        let config = Config::default();
        save(&config)?;
        return Ok(config);
    }

    let contents = fs::read_to_string(path)?;

    let config: Config =
        toml::from_str(&contents).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    Ok(config)
}

pub fn save(config: &Config) -> io::Result<()> {
    fs::create_dir_all(config_dir())?;

    let toml = toml::to_string_pretty(config).map_err(io::Error::other)?;

    fs::write(config_path(), toml)
}

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
        let config = Config::default();

        assert_eq!(config.editor.command, "code");
    }

    #[test]
    fn default_ai_provider_is_ollama() {
        let config = Config::default();

        assert_eq!(config.ai.provider, "ollama");
    }
}
