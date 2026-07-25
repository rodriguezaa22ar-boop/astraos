use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::{
    io::{self, stdout},
    time::Duration,
};

use astra_system::command_exists;
use astra_workspaces::astra_root;

pub fn run_dashboard() -> io::Result<()> {
    enable_raw_mode()?;

    let mut output = stdout();
    execute!(output, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(output);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
    loop {
        terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(8),
                    Constraint::Length(3),
                ])
                .split(frame.area());

            let title = Paragraph::new("ASTRA COMMAND CENTER")
                .style(Style::default().add_modifier(Modifier::BOLD))
                .block(Block::default().borders(Borders::ALL));
            frame.render_widget(title, chunks[0]);

            let body = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);

            let tools = [
                "brew", "git", "gh", "node", "python3", "docker", "codex", "ollama",
            ]
            .into_iter()
            .map(|tool| {
                let marker = if command_exists(tool) { "✓" } else { "!" };
                ListItem::new(Line::from(format!("{marker} {tool}")))
            })
            .collect::<Vec<_>>();

            let systems =
                List::new(tools).block(Block::default().title("System").borders(Borders::ALL));
            frame.render_widget(systems, body[0]);

            let root = astra_root();
            let projects = [
                ("Astraeus Omnia", root.join("astraeus-omnia")),
                ("Omnia API Foundry", root.join("omnia-api-foundry")),
                ("Games", root.join("games")),
                ("Cybersecurity", root.join("cybersecurity")),
                ("AI Lab", root.join("ai")),
            ]
            .into_iter()
            .map(|(name, path)| {
                let marker = if path.exists() { "✓" } else { "!" };
                ListItem::new(Line::from(format!("{marker} {name}")))
            })
            .collect::<Vec<_>>();

            let project_list =
                List::new(projects).block(Block::default().title("Projects").borders(Borders::ALL));
            frame.render_widget(project_list, body[1]);

            let footer = Paragraph::new("Press q or Esc to exit")
                .block(Block::default().borders(Borders::ALL));
            frame.render_widget(footer, chunks[2]);
        })?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    return Ok(());
                }
            }
        }
    }
}
