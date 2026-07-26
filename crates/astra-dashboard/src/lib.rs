mod app;
mod ui;

use app::{DashboardState, InputAction, RENDER_TICK};
use astra_config::{config_path, Config};
use astra_system::SystemCollector;
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    fs,
    io::{self, stdout},
    time::Instant,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DashboardError {
    #[error("terminal operation failed: {0}")]
    Terminal(#[from] io::Error),
}

pub fn run_dashboard(config: Config) -> Result<(), DashboardError> {
    let mut session = TerminalSession::enter()?;

    let loop_result = {
        let backend = CrosstermBackend::new(stdout());
        let mut terminal = Terminal::new(backend)?;
        run_loop(&mut terminal, config)
    };

    let cleanup_result = session.restore().map_err(DashboardError::from);

    match loop_result {
        Err(error) => Err(error),
        Ok(()) => cleanup_result,
    }
}

fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    config: Config,
) -> Result<(), DashboardError> {
    let mut state = DashboardState::new(&config);
    let mut collector = SystemCollector::new();
    let mut next_render = Instant::now();

    terminal.draw(|frame| ui::render(frame, &state))?;

    loop {
        let now = Instant::now();

        if state.should_refresh(now) {
            refresh_workspaces(&mut state);

            let filesystem_path = state.selected_filesystem_path().to_path_buf();
            let snapshot = if state.force_service_refresh() {
                collector.refresh_now(&filesystem_path)
            } else {
                collector.refresh(&filesystem_path)
            };
            state.complete_refresh(snapshot, now);
        }

        let after_refresh = Instant::now();
        if after_refresh >= next_render {
            terminal.draw(|frame| ui::render(frame, &state))?;
            next_render = after_refresh + RENDER_TICK;
        }

        let poll_timeout = next_render.saturating_duration_since(Instant::now());

        if event::poll(poll_timeout)? {
            match event::read()? {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    if state.handle_key(key.code) == InputAction::Quit {
                        return Ok(());
                    }
                }
                Event::Resize(_, _) => next_render = Instant::now(),
                _ => {}
            }
        }
    }
}

fn refresh_workspaces(state: &mut DashboardState) {
    match load_config_read_only() {
        Ok(config) => {
            state.update_workspaces(&config);
            state.clear_status_message();
        }
        Err(message) => state.set_status_message(message),
    }
}

fn load_config_read_only() -> Result<Config, String> {
    let path = config_path();
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Config refresh unavailable: {error}"))?;

    toml::from_str(&contents).map_err(|error| format!("Config refresh unavailable: {error}"))
}

struct TerminalSession {
    raw_mode_enabled: bool,
    alternate_screen_entered: bool,
    cursor_hidden: bool,
}

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;

        let mut session = Self {
            raw_mode_enabled: true,
            alternate_screen_entered: false,
            cursor_hidden: false,
        };
        let mut output = stdout();

        execute!(output, EnterAlternateScreen)?;
        session.alternate_screen_entered = true;

        execute!(output, Hide)?;
        session.cursor_hidden = true;

        Ok(session)
    }

    fn restore(&mut self) -> io::Result<()> {
        let mut first_error = None;

        if self.raw_mode_enabled {
            match disable_raw_mode() {
                Ok(()) => self.raw_mode_enabled = false,
                Err(error) => first_error = Some(error),
            }
        }

        let mut output = stdout();

        if self.cursor_hidden {
            match execute!(output, Show) {
                Ok(()) => self.cursor_hidden = false,
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }

        if self.alternate_screen_entered {
            match execute!(output, LeaveAlternateScreen) {
                Ok(()) => self.alternate_screen_entered = false,
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
