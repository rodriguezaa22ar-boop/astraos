use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub workspace: WorkspaceConfig,
    pub editor: EditorConfig,
    pub ai: AiConfig,
    pub cyber: CyberConfig,

    #[serde(default)]
    pub workspaces: BTreeMap<String, String>,

    #[serde(default)]
    pub terminal: TerminalConfig,

    #[serde(default)]
    pub workspace_layouts: BTreeMap<String, WorkspaceLayout>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    pub command: String,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            command: "wezterm".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceLayout {
    #[serde(default)]
    pub editor: bool,

    #[serde(default)]
    pub ollama: bool,

    #[serde(default)]
    pub tabs: Vec<TabLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabLayout {
    pub name: String,

    #[serde(default)]
    pub command: Vec<String>,

    #[serde(default)]
    pub panes: Vec<PaneLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneLayout {
    pub target: usize,
    pub direction: SplitDirection,
    pub percent: u16,

    #[serde(default)]
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Left,
    Right,
    Top,
    Bottom,
}
