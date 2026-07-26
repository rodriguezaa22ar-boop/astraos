use astra_config::Config;
use astra_system::{DeveloperServicesSnapshot, ServiceStatus, SystemSnapshot};
use astra_workspaces::list_workspaces;
use crossterm::event::KeyCode;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) const RENDER_TICK: Duration = Duration::from_millis(100);
pub(crate) const METRICS_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceEntry {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputAction {
    None,
    Redraw,
    Refresh,
    Quit,
}

#[derive(Debug)]
pub(crate) struct DashboardState {
    snapshot: SystemSnapshot,
    workspaces: Vec<WorkspaceEntry>,
    selected_workspace: Option<usize>,
    last_refresh: Option<Instant>,
    refresh_requested: bool,
    force_service_refresh: bool,
    status_message: Option<String>,
}

impl DashboardState {
    pub(crate) fn new(config: &Config) -> Self {
        let mut state = Self {
            snapshot: SystemSnapshot::default(),
            workspaces: Vec::new(),
            selected_workspace: None,
            last_refresh: None,
            refresh_requested: true,
            force_service_refresh: true,
            status_message: None,
        };
        state.update_workspaces(config);
        state
    }

    pub(crate) fn snapshot(&self) -> &SystemSnapshot {
        &self.snapshot
    }

    pub(crate) fn workspaces(&self) -> &[WorkspaceEntry] {
        &self.workspaces
    }

    pub(crate) fn selected_workspace(&self) -> Option<usize> {
        self.selected_workspace
    }

    pub(crate) fn selected_filesystem_path(&self) -> &Path {
        self.selected_workspace
            .and_then(|index| self.workspaces.get(index))
            .map(|workspace| workspace.path.as_path())
            .unwrap_or_else(|| Path::new("/"))
    }

    pub(crate) fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }

    pub(crate) fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    pub(crate) fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    pub(crate) fn update_workspaces(&mut self, config: &Config) {
        let previously_selected = self
            .selected_workspace
            .and_then(|index| self.workspaces.get(index))
            .map(|workspace| workspace.name.clone());
        let previous_index = self.selected_workspace.unwrap_or(0);

        self.workspaces = workspace_entries(config);
        self.selected_workspace = if self.workspaces.is_empty() {
            None
        } else {
            previously_selected
                .and_then(|name| {
                    self.workspaces
                        .iter()
                        .position(|workspace| workspace.name == name)
                })
                .or_else(|| Some(previous_index.min(self.workspaces.len().saturating_sub(1))))
        };
    }

    pub(crate) fn should_refresh(&self, now: Instant) -> bool {
        self.refresh_requested
            || self
                .last_refresh
                .is_none_or(|last| now.saturating_duration_since(last) >= METRICS_REFRESH_INTERVAL)
    }

    pub(crate) fn force_service_refresh(&self) -> bool {
        self.force_service_refresh
    }

    pub(crate) fn complete_refresh(&mut self, snapshot: SystemSnapshot, now: Instant) {
        merge_snapshot(&mut self.snapshot, snapshot);
        self.last_refresh = Some(now);
        self.refresh_requested = false;
        self.force_service_refresh = false;
    }

    pub(crate) fn handle_key(&mut self, code: KeyCode) -> InputAction {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => InputAction::Quit,
            KeyCode::Char('r') => {
                self.refresh_requested = true;
                self.force_service_refresh = true;
                InputAction::Refresh
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.select_previous() {
                    self.refresh_requested = true;
                    InputAction::Redraw
                } else {
                    InputAction::None
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.select_next() {
                    self.refresh_requested = true;
                    InputAction::Redraw
                } else {
                    InputAction::None
                }
            }
            _ => InputAction::None,
        }
    }

    fn select_previous(&mut self) -> bool {
        let Some(selected) = self.selected_workspace else {
            return false;
        };
        let next = selected.saturating_sub(1);
        let changed = next != selected;
        self.selected_workspace = Some(next);
        changed
    }

    fn select_next(&mut self) -> bool {
        let Some(selected) = self.selected_workspace else {
            return false;
        };
        let last = self.workspaces.len().saturating_sub(1);
        let next = selected.saturating_add(1).min(last);
        let changed = next != selected;
        self.selected_workspace = Some(next);
        changed
    }
}

fn workspace_entries(config: &Config) -> Vec<WorkspaceEntry> {
    let mut entries = list_workspaces(config)
        .into_iter()
        .map(|(name, path)| WorkspaceEntry {
            name,
            path: PathBuf::from(path),
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.path.cmp(&right.path))
    });
    entries
}

fn merge_snapshot(current: &mut SystemSnapshot, incoming: SystemSnapshot) {
    if incoming.operating_system.is_some() {
        current.operating_system = incoming.operating_system;
    }
    if incoming.hostname.is_some() {
        current.hostname = incoming.hostname;
    }
    if incoming.cpu.is_some() {
        current.cpu = incoming.cpu;
    }
    if incoming.memory.is_some() {
        current.memory = incoming.memory;
    }
    if incoming.disk.is_some() {
        current.disk = incoming.disk;
    }
    if incoming.battery.is_some() {
        current.battery = incoming.battery;
    }
    if incoming.uptime.is_some() {
        current.uptime = incoming.uptime;
    }

    current.services = DeveloperServicesSnapshot {
        docker: merge_service_status(current.services.docker, incoming.services.docker),
        ollama: merge_service_status(current.services.ollama, incoming.services.ollama),
    };
}

fn merge_service_status(current: ServiceStatus, incoming: ServiceStatus) -> ServiceStatus {
    if incoming == ServiceStatus::Unknown && current != ServiceStatus::Unknown {
        current
    } else {
        incoming
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use astra_config::{AiConfig, CyberConfig, EditorConfig, WorkspaceConfig};
    use astra_system::{CpuSnapshot, MemorySnapshot};
    use std::collections::BTreeMap;

    fn config(entries: &[(&str, &str)]) -> Config {
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
            workspaces: entries
                .iter()
                .map(|(name, path)| ((*name).to_string(), (*path).to_string()))
                .collect::<BTreeMap<_, _>>(),
            terminal: Default::default(),
            workspace_layouts: BTreeMap::new(),
        }
    }

    #[test]
    fn initial_state_requests_refresh_and_selects_first_workspace() {
        let state = DashboardState::new(&config(&[("beta", "/b"), ("alpha", "/a")]));

        assert!(state.should_refresh(Instant::now()));
        assert_eq!(state.selected_workspace(), Some(0));
        assert_eq!(state.workspaces()[0].name, "alpha");
    }

    #[test]
    fn refresh_schedule_uses_metrics_interval() {
        let now = Instant::now();
        let mut state = DashboardState::new(&config(&[]));
        state.complete_refresh(SystemSnapshot::default(), now);

        assert!(!state.should_refresh(now + METRICS_REFRESH_INTERVAL / 2));
        assert!(state.should_refresh(now + METRICS_REFRESH_INTERVAL));
    }

    #[test]
    fn manual_refresh_is_immediately_due_and_forces_services() {
        let now = Instant::now();
        let mut state = DashboardState::new(&config(&[]));
        state.complete_refresh(SystemSnapshot::default(), now);

        assert_eq!(state.handle_key(KeyCode::Char('r')), InputAction::Refresh);
        assert!(state.should_refresh(now));
        assert!(state.force_service_refresh());
    }

    #[test]
    fn quit_keys_exit() {
        let mut state = DashboardState::new(&config(&[]));

        assert_eq!(state.handle_key(KeyCode::Char('q')), InputAction::Quit);
        assert_eq!(state.handle_key(KeyCode::Esc), InputAction::Quit);
    }

    #[test]
    fn selection_is_safe_for_empty_and_single_registries() {
        let mut empty = DashboardState::new(&config(&[]));
        assert_eq!(empty.handle_key(KeyCode::Down), InputAction::None);
        assert_eq!(empty.selected_workspace(), None);

        let mut single = DashboardState::new(&config(&[("only", "/only")]));
        assert_eq!(single.handle_key(KeyCode::Down), InputAction::None);
        assert_eq!(single.handle_key(KeyCode::Up), InputAction::None);
        assert_eq!(single.selected_workspace(), Some(0));
    }

    #[test]
    fn selection_stays_within_multiple_workspace_bounds() {
        let mut state =
            DashboardState::new(&config(&[("alpha", "/a"), ("beta", "/b"), ("gamma", "/g")]));

        assert_eq!(state.handle_key(KeyCode::Down), InputAction::Redraw);
        assert_eq!(state.handle_key(KeyCode::Char('j')), InputAction::Redraw);
        assert_eq!(state.handle_key(KeyCode::Down), InputAction::None);
        assert_eq!(state.selected_workspace(), Some(2));

        assert_eq!(state.handle_key(KeyCode::Char('k')), InputAction::Redraw);
        assert_eq!(state.handle_key(KeyCode::Up), InputAction::Redraw);
        assert_eq!(state.handle_key(KeyCode::Up), InputAction::None);
        assert_eq!(state.selected_workspace(), Some(0));
    }

    #[test]
    fn selection_tracks_name_and_clamps_when_registry_changes() {
        let mut state =
            DashboardState::new(&config(&[("alpha", "/a"), ("beta", "/b"), ("gamma", "/g")]));
        state.handle_key(KeyCode::Down);
        state.handle_key(KeyCode::Down);

        state.update_workspaces(&config(&[("beta", "/new-b"), ("gamma", "/new-g")]));
        assert_eq!(state.selected_workspace(), Some(1));
        assert_eq!(state.workspaces()[1].name, "gamma");

        state.update_workspaces(&config(&[("alpha", "/a")]));
        assert_eq!(state.selected_workspace(), Some(0));
    }

    #[test]
    fn deterministic_workspace_order_does_not_depend_on_input_order() {
        let state = DashboardState::new(&config(&[
            ("zeta", "/z"),
            ("alpha", "/a"),
            ("middle", "/m"),
        ]));
        let names = state
            .workspaces()
            .iter()
            .map(|workspace| workspace.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, ["alpha", "middle", "zeta"]);
    }

    #[test]
    fn refresh_preserves_last_valid_optional_values_and_service_statuses() {
        let now = Instant::now();
        let mut state = DashboardState::new(&config(&[]));
        let valid = SystemSnapshot {
            cpu: Some(CpuSnapshot {
                usage_percent: 42.0,
            }),
            memory: Some(MemorySnapshot {
                used_bytes: 10,
                total_bytes: 20,
            }),
            services: DeveloperServicesSnapshot {
                docker: ServiceStatus::Running,
                ollama: ServiceStatus::Stopped,
            },
            ..SystemSnapshot::default()
        };
        state.complete_refresh(valid, now);

        state.complete_refresh(SystemSnapshot::default(), now + Duration::from_secs(1));

        assert_eq!(
            state.snapshot().cpu,
            Some(CpuSnapshot {
                usage_percent: 42.0
            })
        );
        assert_eq!(
            state.snapshot().memory.map(|memory| memory.used_bytes),
            Some(10)
        );
        assert_eq!(state.snapshot().services.docker, ServiceStatus::Running);
        assert_eq!(state.snapshot().services.ollama, ServiceStatus::Stopped);
    }
}
