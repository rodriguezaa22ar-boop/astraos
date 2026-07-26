use crate::{
    plan::{LaunchPlan, PlanPane},
    process::{
        bounded_text, status_label, CommandInvocation, ProcessOutput, ProcessRunner,
        SystemProcessRunner,
    },
    TerminalError,
};
use astra_config::SplitDirection;
use serde::Deserialize;
use std::{collections::BTreeSet, thread, time::Duration};

const DISCOVERY_ATTEMPTS: usize = 20;
const DISCOVERY_DELAY: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSummary {
    pub workspace: String,
    pub layout: String,
    pub mux_workspace: String,
    pub pane_count: usize,
}

#[derive(Debug, Deserialize)]
struct ListedPane {
    pane_id: u64,
    workspace: String,
}

trait Sleeper {
    fn sleep(&mut self, duration: Duration);
}

struct ThreadSleeper;

impl Sleeper for ThreadSleeper {
    fn sleep(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
}

pub fn launch(plan: &LaunchPlan) -> Result<LaunchSummary, TerminalError> {
    let mut runner = SystemProcessRunner;
    let mut sleeper = ThreadSleeper;
    launch_with(
        plan,
        &mut runner,
        &mut sleeper,
        DISCOVERY_ATTEMPTS,
        DISCOVERY_DELAY,
    )
}

pub(crate) fn render_dry_run(plan: &LaunchPlan) -> String {
    let mut lines = vec![
        format!(
            "Launch workspace {:?} with layout {:?} as {:?}",
            plan.workspace_name, plan.layout_name, plan.mux_workspace
        ),
        format!(
            "1. inspect existing mux workspaces: {}",
            list_invocation(plan).render()
        ),
        format!(
            "2. create initial pane: {}",
            spawn_initial_invocation(plan).render()
        ),
        format!(
            "   if no GUI/mux is reachable: {}",
            start_invocation(plan).render()
        ),
        format!(
            "   then poll for the initial pane (bounded): {}",
            list_invocation(plan).render()
        ),
    ];

    let mut sequence = 3;
    for (tab_index, tab) in plan.tabs.iter().enumerate() {
        let initial_placeholder = pane_placeholder(tab_index, 0);
        lines.push(format!(
            "{sequence}. name tab {:?}: {}",
            tab.name,
            set_tab_title_invocation(plan, &initial_placeholder, &tab.name).render()
        ));
        sequence += 1;

        for (pane_offset, pane) in tab.panes.iter().enumerate() {
            let pane_index = pane_offset + 1;
            let target = pane_placeholder(tab_index, pane.target);
            lines.push(format!(
                "{sequence}. create pane {}:{}: {}",
                tab_index,
                pane_index,
                split_invocation(plan, &target, pane).render()
            ));
            sequence += 1;
        }

        if let Some(next_tab) = plan.tabs.get(tab_index + 1) {
            let root = pane_placeholder(0, 0);
            lines.push(format!(
                "{sequence}. create tab {}: {}",
                tab_index + 1,
                spawn_tab_invocation(plan, &root, &next_tab.command).render()
            ));
            sequence += 1;
        }
    }

    if plan.editor_enabled {
        lines.push(format!(
            "{sequence}. start configured editor: {}",
            editor_invocation(plan).render()
        ));
        sequence += 1;
    }
    if plan.ollama_enabled {
        lines.push(format!(
            "{sequence}. start explicitly enabled Ollama service: {}",
            ollama_invocation().render()
        ));
    }

    lines.join("\n") + "\n"
}

fn launch_with(
    plan: &LaunchPlan,
    runner: &mut dyn ProcessRunner,
    sleeper: &mut dyn Sleeper,
    discovery_attempts: usize,
    discovery_delay: Duration,
) -> Result<LaunchSummary, TerminalError> {
    if !runner.executable_available(&plan.terminal) {
        return Err(TerminalError::ExecutableUnavailable(plan.terminal.clone()));
    }

    let initial_list = run_raw(runner, &list_invocation(plan), "list WezTerm panes")?;
    let initial_pane = if initial_list.status.success() {
        let panes = parse_list(&initial_list.stdout)?;
        if panes
            .iter()
            .any(|pane| pane.workspace == plan.mux_workspace)
        {
            return Err(TerminalError::ExistingMuxWorkspace(
                plan.mux_workspace.clone(),
            ));
        }

        let output = run_raw(
            runner,
            &spawn_initial_invocation(plan),
            "create initial WezTerm pane",
        )?;
        if !output.status.success() {
            return Err(command_failure("create initial WezTerm pane", &output));
        }
        parse_pane_id("wezterm cli spawn", &output.stdout)?
    } else {
        let spawn_output = run_raw(
            runner,
            &spawn_initial_invocation(plan),
            "create initial WezTerm pane",
        )?;
        if spawn_output.status.success() {
            parse_pane_id("wezterm cli spawn", &spawn_output.stdout)?
        } else {
            runner.start(&start_invocation(plan)).map_err(|source| {
                TerminalError::MuxUnavailable {
                    stderr: format!(
                        "list: {}; spawn: {}; start: {source}",
                        bounded_text(&initial_list.stderr),
                        bounded_text(&spawn_output.stderr)
                    ),
                }
            })?;

            discover_initial_pane(
                plan,
                runner,
                sleeper,
                discovery_attempts,
                discovery_delay,
                &BTreeSet::new(),
            )
            .map_err(|error| TerminalError::partial("discover initial WezTerm pane", error))?
        }
    };

    let mut pane_ids = vec![Vec::new(); plan.tabs.len()];
    pane_ids[0].push(initial_pane);
    let mut pane_count = 1;

    for (tab_index, tab) in plan.tabs.iter().enumerate() {
        if tab_index > 0 {
            let output = execute_after_start(
                runner,
                &spawn_tab_invocation(plan, &initial_pane.to_string(), &tab.command),
                &format!("create tab {tab_index}"),
            )?;
            let pane_id = parse_pane_id("wezterm cli spawn", &output.stdout).map_err(|error| {
                TerminalError::partial(format!("parse pane ID for tab {tab_index}"), error)
            })?;
            pane_ids[tab_index].push(pane_id);
            pane_count += 1;
        }

        let tab_pane = pane_ids[tab_index][0];
        execute_after_start(
            runner,
            &set_tab_title_invocation(plan, &tab_pane.to_string(), &tab.name),
            &format!("name tab {tab_index}"),
        )?;

        for (pane_offset, pane) in tab.panes.iter().enumerate() {
            let pane_index = pane_offset + 1;
            let target_id = pane_ids[tab_index][pane.target];
            let output = execute_after_start(
                runner,
                &split_invocation(plan, &target_id.to_string(), pane),
                &format!("create tab {tab_index} pane {pane_index}"),
            )?;
            let pane_id =
                parse_pane_id("wezterm cli split-pane", &output.stdout).map_err(|error| {
                    TerminalError::partial(
                        format!("parse pane ID for tab {tab_index} pane {pane_index}"),
                        error,
                    )
                })?;
            pane_ids[tab_index].push(pane_id);
            pane_count += 1;
        }
    }

    if plan.editor_enabled {
        if !runner.executable_available(&plan.editor) {
            return Err(TerminalError::partial(
                "start configured editor",
                TerminalError::ExecutableUnavailable(plan.editor.clone()),
            ));
        }
        runner.start(&editor_invocation(plan)).map_err(|source| {
            TerminalError::partial(
                "start configured editor",
                TerminalError::ProcessExecution {
                    operation: "start configured editor".to_string(),
                    source,
                },
            )
        })?;
    }

    if plan.ollama_enabled {
        if !runner.executable_available("brew") {
            return Err(TerminalError::partial(
                "start Ollama service",
                TerminalError::ExecutableUnavailable("brew".to_string()),
            ));
        }
        execute_after_start(runner, &ollama_invocation(), "start Ollama service")?;
    }

    Ok(LaunchSummary {
        workspace: plan.workspace_name.clone(),
        layout: plan.layout_name.clone(),
        mux_workspace: plan.mux_workspace.clone(),
        pane_count,
    })
}

fn execute_after_start(
    runner: &mut dyn ProcessRunner,
    invocation: &CommandInvocation,
    operation: &str,
) -> Result<ProcessOutput, TerminalError> {
    let output = run_raw(runner, invocation, operation)
        .map_err(|error| TerminalError::partial(operation, error))?;
    if !output.status.success() {
        return Err(TerminalError::partial(
            operation,
            command_failure(operation, &output),
        ));
    }
    Ok(output)
}

fn discover_initial_pane(
    plan: &LaunchPlan,
    runner: &mut dyn ProcessRunner,
    sleeper: &mut dyn Sleeper,
    attempts: usize,
    delay: Duration,
    preexisting: &BTreeSet<u64>,
) -> Result<u64, TerminalError> {
    for attempt in 0..attempts {
        let output = run_raw(runner, &list_invocation(plan), "discover WezTerm panes")?;
        if output.status.success() {
            let panes = parse_list(&output.stdout)?;
            let candidates = panes
                .iter()
                .filter(|pane| {
                    pane.workspace == plan.mux_workspace && !preexisting.contains(&pane.pane_id)
                })
                .map(|pane| pane.pane_id)
                .collect::<BTreeSet<_>>();

            match candidates.len() {
                0 => {}
                1 => {
                    return candidates.iter().next().copied().ok_or_else(|| {
                        TerminalError::MalformedOutput {
                            operation: "discover WezTerm panes".to_string(),
                            message: "single candidate disappeared".to_string(),
                        }
                    });
                }
                count => {
                    return Err(TerminalError::AmbiguousStartupDiscovery {
                        workspace: plan.mux_workspace.clone(),
                        candidates: count,
                    });
                }
            }
        }

        if attempt + 1 < attempts {
            sleeper.sleep(delay);
        }
    }

    Err(TerminalError::StartupDiscoveryTimeout {
        workspace: plan.mux_workspace.clone(),
    })
}

fn run_raw(
    runner: &mut dyn ProcessRunner,
    invocation: &CommandInvocation,
    operation: &str,
) -> Result<ProcessOutput, TerminalError> {
    let output = runner
        .run(invocation)
        .map_err(|source| TerminalError::ProcessExecution {
            operation: operation.to_string(),
            source,
        })?;
    if output.timed_out {
        return Err(TerminalError::CommandTimedOut {
            operation: operation.to_string(),
        });
    }
    Ok(output)
}

fn command_failure(operation: &str, output: &ProcessOutput) -> TerminalError {
    TerminalError::CommandFailed {
        operation: operation.to_string(),
        status: status_label(output.status),
        stderr: bounded_text(&output.stderr),
    }
}

fn parse_pane_id(operation: &str, stdout: &[u8]) -> Result<u64, TerminalError> {
    let text = std::str::from_utf8(stdout).map_err(|error| TerminalError::MalformedOutput {
        operation: operation.to_string(),
        message: format!("output was not UTF-8: {error}"),
    })?;
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.split_whitespace().count() != 1 {
        return Err(TerminalError::MalformedOutput {
            operation: operation.to_string(),
            message: "expected exactly one pane ID".to_string(),
        });
    }
    trimmed
        .parse::<u64>()
        .map_err(|error| TerminalError::MalformedOutput {
            operation: operation.to_string(),
            message: format!("invalid pane ID {trimmed:?}: {error}"),
        })
}

fn parse_list(stdout: &[u8]) -> Result<Vec<ListedPane>, TerminalError> {
    serde_json::from_slice(stdout).map_err(|error| TerminalError::MalformedOutput {
        operation: "wezterm cli list --format json".to_string(),
        message: error.to_string(),
    })
}

fn list_invocation(plan: &LaunchPlan) -> CommandInvocation {
    CommandInvocation::new(&plan.terminal, ["cli", "list", "--format", "json"])
}

fn spawn_initial_invocation(plan: &LaunchPlan) -> CommandInvocation {
    let mut arguments = vec![
        "cli".to_string(),
        "spawn".to_string(),
        "--new-window".to_string(),
        "--workspace".to_string(),
        plan.mux_workspace.clone(),
        "--cwd".to_string(),
        plan.workspace_path.to_string_lossy().into_owned(),
    ];
    append_command(&mut arguments, &plan.tabs[0].command);
    CommandInvocation::new(&plan.terminal, arguments)
}

fn start_invocation(plan: &LaunchPlan) -> CommandInvocation {
    let mut arguments = vec![
        "start".to_string(),
        "--workspace".to_string(),
        plan.mux_workspace.clone(),
        "--cwd".to_string(),
        plan.workspace_path.to_string_lossy().into_owned(),
    ];
    append_command(&mut arguments, &plan.tabs[0].command);
    CommandInvocation::new(&plan.terminal, arguments)
}

fn spawn_tab_invocation(plan: &LaunchPlan, pane_id: &str, command: &[String]) -> CommandInvocation {
    let mut arguments = vec![
        "cli".to_string(),
        "spawn".to_string(),
        "--pane-id".to_string(),
        pane_id.to_string(),
        "--cwd".to_string(),
        plan.workspace_path.to_string_lossy().into_owned(),
    ];
    append_command(&mut arguments, command);
    CommandInvocation::new(&plan.terminal, arguments)
}

fn split_invocation(plan: &LaunchPlan, target_id: &str, pane: &PlanPane) -> CommandInvocation {
    let mut arguments = vec![
        "cli".to_string(),
        "split-pane".to_string(),
        "--pane-id".to_string(),
        target_id.to_string(),
        direction_argument(pane.direction).to_string(),
        "--percent".to_string(),
        pane.percent.to_string(),
        "--cwd".to_string(),
        plan.workspace_path.to_string_lossy().into_owned(),
    ];
    append_command(&mut arguments, &pane.command);
    CommandInvocation::new(&plan.terminal, arguments)
}

fn set_tab_title_invocation(plan: &LaunchPlan, pane_id: &str, title: &str) -> CommandInvocation {
    CommandInvocation::new(
        &plan.terminal,
        ["cli", "set-tab-title", "--pane-id", pane_id, title],
    )
}

fn editor_invocation(plan: &LaunchPlan) -> CommandInvocation {
    CommandInvocation::new(
        &plan.editor,
        [plan.workspace_path.to_string_lossy().into_owned()],
    )
}

fn ollama_invocation() -> CommandInvocation {
    CommandInvocation::new("brew", ["services", "start", "ollama"])
}

fn append_command(arguments: &mut Vec<String>, command: &[String]) {
    if !command.is_empty() {
        arguments.push("--".to_string());
        arguments.extend(command.iter().cloned());
    }
}

fn direction_argument(direction: SplitDirection) -> &'static str {
    match direction {
        SplitDirection::Left => "--left",
        SplitDirection::Right => "--right",
        SplitDirection::Top => "--top",
        SplitDirection::Bottom => "--bottom",
    }
}

fn pane_placeholder(tab: usize, pane: usize) -> String {
    format!("<pane:{tab}:{pane}>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_launch_plan;
    use astra_config::{
        AiConfig, Config, CyberConfig, EditorConfig, PaneLayout, TabLayout, TerminalConfig,
        WorkspaceConfig, WorkspaceLayout,
    };
    use std::{
        collections::{BTreeMap, VecDeque},
        io,
        os::unix::process::ExitStatusExt,
        process::ExitStatus,
    };

    struct FakeRunner {
        available: bool,
        outputs: VecDeque<io::Result<ProcessOutput>>,
        invocations: Vec<CommandInvocation>,
        starts: Vec<CommandInvocation>,
    }

    impl FakeRunner {
        fn new(outputs: Vec<ProcessOutput>) -> Self {
            Self {
                available: true,
                outputs: outputs.into_iter().map(Ok).collect(),
                invocations: Vec::new(),
                starts: Vec::new(),
            }
        }
    }

    impl ProcessRunner for FakeRunner {
        fn executable_available(&self, _executable: &str) -> bool {
            self.available
        }

        fn run(&mut self, invocation: &CommandInvocation) -> io::Result<ProcessOutput> {
            self.invocations.push(invocation.clone());
            self.outputs
                .pop_front()
                .unwrap_or_else(|| Err(io::Error::other("unexpected invocation")))
        }

        fn start(&mut self, invocation: &CommandInvocation) -> io::Result<()> {
            self.starts.push(invocation.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct NoDelaySleeper {
        sleeps: usize,
    }

    impl Sleeper for NoDelaySleeper {
        fn sleep(&mut self, _duration: Duration) {
            self.sleeps += 1;
        }
    }

    fn output(success: bool, stdout: &str, stderr: &str) -> ProcessOutput {
        ProcessOutput {
            status: ExitStatus::from_raw(if success { 0 } else { 1 << 8 }),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            timed_out: false,
        }
    }

    fn plan_with(
        terminal: &str,
        workspace_path: &std::path::Path,
        panes: Vec<PaneLayout>,
    ) -> LaunchPlan {
        let mut workspaces = BTreeMap::new();
        workspaces.insert(
            "project".to_string(),
            workspace_path.to_string_lossy().into_owned(),
        );
        let mut layouts = BTreeMap::new();
        layouts.insert(
            "rust".to_string(),
            WorkspaceLayout {
                editor: false,
                ollama: false,
                tabs: vec![TabLayout {
                    name: "dev".to_string(),
                    command: vec!["cargo".to_string(), "check all".to_string()],
                    panes,
                }],
            },
        );
        let config = Config {
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
            terminal: TerminalConfig {
                command: terminal.to_string(),
            },
            workspace_layouts: layouts,
        };
        build_launch_plan(&config, "project", Some("rust")).expect("valid plan")
    }

    #[test]
    fn direct_spawn_pane_id_is_used() {
        let directory = tempfile::tempdir().expect("temp directory");
        let plan = plan_with("wezterm", directory.path(), Vec::new());
        let mut runner = FakeRunner::new(vec![
            output(true, "[]", ""),
            output(true, "42\n", ""),
            output(true, "", ""),
        ]);
        let mut sleeper = NoDelaySleeper::default();

        let summary =
            launch_with(&plan, &mut runner, &mut sleeper, 1, Duration::ZERO).expect("launch");

        assert_eq!(summary.pane_count, 1);
        assert!(runner.invocations[2]
            .arguments
            .windows(2)
            .any(|pair| pair == ["--pane-id", "42"]));
    }

    #[test]
    fn direct_split_pane_id_is_used_and_arguments_stay_separate() {
        let directory = tempfile::tempdir().expect("temp directory");
        let pane = PaneLayout {
            target: 0,
            direction: SplitDirection::Right,
            percent: 45,
            command: vec!["cargo".to_string(), "watch all".to_string()],
        };
        let plan = plan_with("wezterm", directory.path(), vec![pane]);
        let mut runner = FakeRunner::new(vec![
            output(true, "[]", ""),
            output(true, "42", ""),
            output(true, "", ""),
            output(true, "84\n", ""),
        ]);
        let mut sleeper = NoDelaySleeper::default();

        launch_with(&plan, &mut runner, &mut sleeper, 1, Duration::ZERO).expect("launch");

        let split = &runner.invocations[3];
        assert!(split
            .arguments
            .windows(2)
            .any(|pair| pair == ["--pane-id", "42"]));
        assert_eq!(
            &split.arguments[split.arguments.len() - 3..],
            ["--", "cargo", "watch all"]
        );
    }

    #[test]
    fn existing_mux_workspace_is_rejected() {
        let directory = tempfile::tempdir().expect("temp directory");
        let plan = plan_with("wezterm", directory.path(), Vec::new());
        let mut runner = FakeRunner::new(vec![output(
            true,
            r#"[{"pane_id":7,"workspace":"astra:project"}]"#,
            "",
        )]);
        let mut sleeper = NoDelaySleeper::default();

        let error = launch_with(&plan, &mut runner, &mut sleeper, 1, Duration::ZERO)
            .expect_err("existing workspace");

        assert!(
            matches!(error, TerminalError::ExistingMuxWorkspace(name) if name == "astra:project")
        );
        assert_eq!(runner.invocations.len(), 1);
    }

    #[test]
    fn ambiguous_initial_discovery_stops_launch() {
        let directory = tempfile::tempdir().expect("temp directory");
        let plan = plan_with("wezterm", directory.path(), Vec::new());
        let mut runner = FakeRunner::new(vec![
            output(false, "", "no mux"),
            output(false, "", "no mux"),
            output(
                true,
                r#"[
                    {"pane_id":7,"workspace":"astra:project"},
                    {"pane_id":8,"workspace":"astra:project"}
                ]"#,
                "",
            ),
        ]);
        let mut sleeper = NoDelaySleeper::default();

        let error = launch_with(&plan, &mut runner, &mut sleeper, 1, Duration::ZERO)
            .expect_err("ambiguous discovery");

        assert!(matches!(
            error,
            TerminalError::PartialLaunchFailure { source, .. }
                if matches!(*source, TerminalError::AmbiguousStartupDiscovery { candidates: 2, .. })
        ));
        assert_eq!(runner.starts.len(), 1);
        assert_eq!(runner.invocations.len(), 3);
    }

    #[test]
    fn startup_discovery_timeout_is_typed_and_uses_zero_delay_in_tests() {
        let directory = tempfile::tempdir().expect("temp directory");
        let plan = plan_with("wezterm", directory.path(), Vec::new());
        let mut runner = FakeRunner::new(vec![
            output(false, "", "no mux"),
            output(false, "", "no mux"),
            output(true, "[]", ""),
            output(true, "[]", ""),
        ]);
        let mut sleeper = NoDelaySleeper::default();

        let error = launch_with(&plan, &mut runner, &mut sleeper, 2, Duration::ZERO)
            .expect_err("discovery should time out");

        assert!(matches!(
            error,
            TerminalError::PartialLaunchFailure { source, .. }
                if matches!(*source, TerminalError::StartupDiscoveryTimeout { .. })
        ));
        assert_eq!(sleeper.sleeps, 1);
    }

    #[test]
    fn partial_failure_stops_subsequent_operations() {
        let directory = tempfile::tempdir().expect("temp directory");
        let pane = PaneLayout {
            target: 0,
            direction: SplitDirection::Right,
            percent: 45,
            command: Vec::new(),
        };
        let plan = plan_with("wezterm", directory.path(), vec![pane]);
        let mut runner = FakeRunner::new(vec![
            output(true, "[]", ""),
            output(true, "42", ""),
            output(false, "", "title failure"),
            output(true, "84", ""),
        ]);
        let mut sleeper = NoDelaySleeper::default();

        let error = launch_with(&plan, &mut runner, &mut sleeper, 1, Duration::ZERO)
            .expect_err("title failure should stop launch");

        assert!(matches!(
            &error,
            TerminalError::PartialLaunchFailure { operation, .. }
                if operation == "name tab 0"
        ));
        assert_eq!(runner.invocations.len(), 3);
        assert!(error
            .to_string()
            .contains("partial WezTerm layout may remain"));
    }

    #[test]
    fn pane_id_parser_rejects_empty_malformed_and_ambiguous_output() {
        for value in [b"".as_slice(), b"abc".as_slice(), b"1 2".as_slice()] {
            assert!(parse_pane_id("test", value).is_err());
        }
        assert_eq!(parse_pane_id("test", b"91\n").expect("pane ID"), 91);
    }

    #[test]
    fn dry_run_is_deterministic_and_executes_nothing() {
        let directory = tempfile::tempdir().expect("temp directory");
        let terminal = "/Applications/WezTerm Dev.app/Contents/MacOS/wezterm";
        let plan = plan_with(terminal, directory.path(), Vec::new());
        let runner = FakeRunner::new(Vec::new());

        let first = plan.render_dry_run();
        let second = plan.render_dry_run();

        assert_eq!(first, second);
        assert!(first.contains(r#"exec="/Applications/WezTerm Dev.app/Contents/MacOS/wezterm""#));
        assert!(first.contains(r#""check all""#));
        assert!(runner.invocations.is_empty());
        assert!(runner.starts.is_empty());
    }

    #[test]
    fn dry_run_includes_only_explicitly_enabled_editor_and_ollama() {
        let directory = tempfile::tempdir().expect("temp directory");
        let mut plan = plan_with("wezterm", directory.path(), Vec::new());

        let disabled = plan.render_dry_run();
        assert!(!disabled.contains("start configured editor"));
        assert!(!disabled.contains("start explicitly enabled Ollama"));

        plan.editor_enabled = true;
        plan.ollama_enabled = true;
        let enabled = plan.render_dry_run();
        assert!(enabled.contains("start configured editor"));
        assert!(enabled.contains(r#"exec="code""#));
        assert!(enabled.contains("start explicitly enabled Ollama"));
        assert!(enabled.contains(r#"exec="brew" args=["services","start","ollama"]"#));
    }

    #[test]
    fn direction_mapping_is_explicit() {
        assert_eq!(direction_argument(SplitDirection::Left), "--left");
        assert_eq!(direction_argument(SplitDirection::Right), "--right");
        assert_eq!(direction_argument(SplitDirection::Top), "--top");
        assert_eq!(direction_argument(SplitDirection::Bottom), "--bottom");
        assert_eq!(
            crate::plan::direction_name(SplitDirection::Bottom),
            "bottom"
        );
    }
}
