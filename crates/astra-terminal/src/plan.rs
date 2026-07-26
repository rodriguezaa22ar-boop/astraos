use crate::TerminalError;
use astra_config::{Config, SplitDirection, WorkspaceLayout};
use std::{fmt::Write, path::PathBuf};

pub const MIN_SPLIT_PERCENT: u16 = 10;
pub const MAX_SPLIT_PERCENT: u16 = 90;

#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub(crate) terminal: String,
    pub(crate) editor: String,
    pub(crate) workspace_name: String,
    pub(crate) layout_name: String,
    pub(crate) mux_workspace: String,
    pub(crate) workspace_path: PathBuf,
    pub(crate) editor_enabled: bool,
    pub(crate) ollama_enabled: bool,
    pub(crate) tabs: Vec<PlanTab>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlanTab {
    pub(crate) name: String,
    pub(crate) command: Vec<String>,
    pub(crate) panes: Vec<PlanPane>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlanPane {
    pub(crate) target: usize,
    pub(crate) direction: SplitDirection,
    pub(crate) percent: u16,
    pub(crate) command: Vec<String>,
}

impl LaunchPlan {
    pub fn workspace_name(&self) -> &str {
        &self.workspace_name
    }

    pub fn layout_name(&self) -> &str {
        &self.layout_name
    }

    pub fn mux_workspace(&self) -> &str {
        &self.mux_workspace
    }

    pub fn workspace_path(&self) -> &std::path::Path {
        &self.workspace_path
    }

    pub fn render_dry_run(&self) -> String {
        crate::wezterm::render_dry_run(self)
    }
}

pub fn build_launch_plan(
    config: &Config,
    workspace_name: &str,
    requested_layout: Option<&str>,
) -> Result<LaunchPlan, TerminalError> {
    let workspace_path = config
        .workspaces
        .get(workspace_name)
        .map(PathBuf::from)
        .ok_or_else(|| TerminalError::UnknownWorkspace(workspace_name.to_string()))?;

    if !workspace_path.is_dir() {
        return Err(TerminalError::WorkspaceDirectory(workspace_path));
    }

    let layout_name = requested_layout.unwrap_or(workspace_name);
    let layout = config.workspace_layouts.get(layout_name).ok_or_else(|| {
        if requested_layout.is_some() {
            TerminalError::UnknownLayout(layout_name.to_string())
        } else {
            TerminalError::MissingDefaultLayout {
                workspace: workspace_name.to_string(),
                layout: layout_name.to_string(),
            }
        }
    })?;

    validate_layout(layout_name, layout)?;

    if config.terminal.command.trim().is_empty() {
        return Err(TerminalError::InvalidConfiguration {
            layout: layout_name.to_string(),
            message: "terminal.command must not be empty".to_string(),
        });
    }

    if layout.editor && config.editor.command.trim().is_empty() {
        return Err(TerminalError::InvalidConfiguration {
            layout: layout_name.to_string(),
            message: "editor.command must not be empty when editor = true".to_string(),
        });
    }

    Ok(LaunchPlan {
        terminal: config.terminal.command.clone(),
        editor: config.editor.command.clone(),
        workspace_name: workspace_name.to_string(),
        layout_name: layout_name.to_string(),
        mux_workspace: format!("astra:{workspace_name}"),
        workspace_path,
        editor_enabled: layout.editor,
        ollama_enabled: layout.ollama,
        tabs: layout
            .tabs
            .iter()
            .map(|tab| PlanTab {
                name: tab.name.clone(),
                command: tab.command.clone(),
                panes: tab
                    .panes
                    .iter()
                    .map(|pane| PlanPane {
                        target: pane.target,
                        direction: pane.direction,
                        percent: pane.percent,
                        command: pane.command.clone(),
                    })
                    .collect(),
            })
            .collect(),
    })
}

pub fn describe_layout(config: &Config, layout_name: &str) -> Result<String, TerminalError> {
    let layout = config
        .workspace_layouts
        .get(layout_name)
        .ok_or_else(|| TerminalError::UnknownLayout(layout_name.to_string()))?;
    validate_layout(layout_name, layout)?;

    let mut output = String::new();
    let _ = writeln!(output, "Layout: {layout_name}");
    let _ = writeln!(output, "Editor: {}", enabled(layout.editor));
    let _ = writeln!(output, "Ollama: {}", enabled(layout.ollama));

    for (tab_index, tab) in layout.tabs.iter().enumerate() {
        let _ = writeln!(
            output,
            "Tab {tab_index}: {} — {}",
            tab.name,
            render_command(&tab.command)
        );

        for (pane_offset, pane) in tab.panes.iter().enumerate() {
            let pane_index = pane_offset + 1;
            let _ = writeln!(
                output,
                "  Pane {pane_index}: split {} from {} at {}% — {}",
                direction_name(pane.direction),
                pane.target,
                pane.percent,
                render_command(&pane.command)
            );
        }
    }

    Ok(output)
}

fn validate_layout(layout_name: &str, layout: &WorkspaceLayout) -> Result<(), TerminalError> {
    if layout.tabs.is_empty() {
        return invalid(layout_name, "at least one tab is required");
    }

    for (tab_index, tab) in layout.tabs.iter().enumerate() {
        if tab.name.trim().is_empty() {
            return invalid(layout_name, format!("tab {tab_index} must have a name"));
        }
        validate_command(layout_name, &format!("tab {tab_index}"), &tab.command)?;

        for (pane_offset, pane) in tab.panes.iter().enumerate() {
            let pane_index = pane_offset + 1;
            if pane.target >= pane_index {
                return invalid(
                    layout_name,
                    format!(
                        "tab {tab_index} pane {pane_index} targets pane {}, but only panes 0..{} have been created",
                        pane.target,
                        pane_index - 1
                    ),
                );
            }
            if !(MIN_SPLIT_PERCENT..=MAX_SPLIT_PERCENT).contains(&pane.percent) {
                return invalid(
                    layout_name,
                    format!(
                        "tab {tab_index} pane {pane_index} split percent {} is outside the supported {MIN_SPLIT_PERCENT}..={MAX_SPLIT_PERCENT} range",
                        pane.percent
                    ),
                );
            }
            validate_command(
                layout_name,
                &format!("tab {tab_index} pane {pane_index}"),
                &pane.command,
            )?;
        }
    }

    Ok(())
}

fn validate_command(
    layout_name: &str,
    owner: &str,
    command: &[String],
) -> Result<(), TerminalError> {
    if command.first().is_some_and(|value| value.is_empty()) {
        return invalid(
            layout_name,
            format!("{owner} command executable must not be empty"),
        );
    }
    Ok(())
}

fn invalid<T>(layout: &str, message: impl Into<String>) -> Result<T, TerminalError> {
    Err(TerminalError::InvalidConfiguration {
        layout: layout.to_string(),
        message: message.into(),
    })
}

fn enabled(value: bool) -> &'static str {
    if value {
        "enabled"
    } else {
        "disabled"
    }
}

fn render_command(command: &[String]) -> String {
    if command.is_empty() {
        "default shell".to_string()
    } else {
        serde_json::to_string(command).unwrap_or_else(|_| "[invalid command]".to_string())
    }
}

pub(crate) fn direction_name(direction: SplitDirection) -> &'static str {
    match direction {
        SplitDirection::Left => "left",
        SplitDirection::Right => "right",
        SplitDirection::Top => "top",
        SplitDirection::Bottom => "bottom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use astra_config::{
        AiConfig, CyberConfig, EditorConfig, PaneLayout, TabLayout, TerminalConfig,
        WorkspaceConfig, WorkspaceLayout,
    };
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn config(workspace: &TempDir) -> Config {
        let mut workspaces = BTreeMap::new();
        workspaces.insert(
            "project".to_string(),
            workspace.path().to_string_lossy().into_owned(),
        );
        let mut workspace_layouts = BTreeMap::new();
        workspace_layouts.insert(
            "rust-development".to_string(),
            WorkspaceLayout {
                editor: false,
                ollama: false,
                tabs: vec![TabLayout {
                    name: "development".to_string(),
                    command: Vec::new(),
                    panes: Vec::new(),
                }],
            },
        );

        Config {
            workspace: WorkspaceConfig {
                root: "/tmp".to_string(),
            },
            editor: EditorConfig {
                command: "code".to_string(),
            },
            ai: AiConfig {
                provider: "ollama".to_string(),
            },
            cyber: CyberConfig {
                labs: "/tmp".to_string(),
            },
            workspaces,
            terminal: TerminalConfig::default(),
            workspace_layouts,
        }
    }

    #[test]
    fn workspace_and_layout_names_may_differ() {
        let directory = tempfile::tempdir().expect("temp directory");
        let plan = build_launch_plan(&config(&directory), "project", Some("rust-development"))
            .expect("different names should be supported");

        assert_eq!(plan.workspace_name(), "project");
        assert_eq!(plan.layout_name(), "rust-development");
        assert_eq!(plan.mux_workspace(), "astra:project");
    }

    #[test]
    fn omitted_layout_resolves_same_name_default() {
        let directory = tempfile::tempdir().expect("temp directory");
        let mut config = config(&directory);
        let layout = config
            .workspace_layouts
            .remove("rust-development")
            .expect("sample layout");
        config
            .workspace_layouts
            .insert("project".to_string(), layout);

        let plan = build_launch_plan(&config, "project", None).expect("default layout");
        assert_eq!(plan.layout_name(), "project");
    }

    #[test]
    fn omitted_layout_reports_missing_default() {
        let directory = tempfile::tempdir().expect("temp directory");
        let error = build_launch_plan(&config(&directory), "project", None)
            .expect_err("same-name layout is absent");

        assert!(matches!(
            &error,
            TerminalError::MissingDefaultLayout { workspace, layout }
                if workspace == "project" && layout == "project"
        ));
        assert!(error.to_string().contains("--layout"));
    }

    #[test]
    fn pane_forward_reference_is_rejected() {
        let directory = tempfile::tempdir().expect("temp directory");
        let mut config = config(&directory);
        config
            .workspace_layouts
            .get_mut("rust-development")
            .unwrap()
            .tabs[0]
            .panes
            .push(PaneLayout {
                target: 1,
                direction: SplitDirection::Right,
                percent: 45,
                command: Vec::new(),
            });

        let error = build_launch_plan(&config, "project", Some("rust-development"))
            .expect_err("forward target must fail");
        assert!(error.to_string().contains("only panes 0..0"));
    }

    #[test]
    fn pane_targets_are_scoped_to_each_tab() {
        let directory = tempfile::tempdir().expect("temp directory");
        let mut config = config(&directory);
        let layout = config
            .workspace_layouts
            .get_mut("rust-development")
            .unwrap();
        layout.tabs[0].panes.push(PaneLayout {
            target: 0,
            direction: SplitDirection::Right,
            percent: 45,
            command: Vec::new(),
        });
        layout.tabs.push(TabLayout {
            name: "second".to_string(),
            command: Vec::new(),
            panes: vec![PaneLayout {
                target: 1,
                direction: SplitDirection::Bottom,
                percent: 50,
                command: Vec::new(),
            }],
        });

        let error = build_launch_plan(&config, "project", Some("rust-development"))
            .expect_err("a second tab cannot target the first tab's pane 1");
        assert!(error.to_string().contains("tab 1 pane 1"));
    }

    #[test]
    fn zero_and_one_hundred_percent_splits_are_rejected() {
        for percent in [0, 100] {
            let directory = tempfile::tempdir().expect("temp directory");
            let mut config = config(&directory);
            config
                .workspace_layouts
                .get_mut("rust-development")
                .unwrap()
                .tabs[0]
                .panes
                .push(PaneLayout {
                    target: 0,
                    direction: SplitDirection::Right,
                    percent,
                    command: Vec::new(),
                });

            assert!(
                build_launch_plan(&config, "project", Some("rust-development")).is_err(),
                "{percent}% should be invalid"
            );
        }
    }

    #[test]
    fn empty_additional_pane_command_is_a_default_shell() {
        let directory = tempfile::tempdir().expect("temp directory");
        let mut config = config(&directory);
        config
            .workspace_layouts
            .get_mut("rust-development")
            .unwrap()
            .tabs[0]
            .panes
            .push(PaneLayout {
                target: 0,
                direction: SplitDirection::Right,
                percent: 45,
                command: Vec::new(),
            });

        let plan = build_launch_plan(&config, "project", Some("rust-development"))
            .expect("empty command should be valid");
        assert!(plan.tabs[0].panes[0].command.is_empty());
    }
}
