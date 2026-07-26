use crate::app::DashboardState;
use astra_system::{
    BatterySnapshot, BatteryState, DeveloperServicesSnapshot, ServiceStatus, SystemSnapshot,
};
use chrono::Local;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use std::time::Duration;

const NARROW_LAYOUT_WIDTH: u16 = 88;

pub(crate) fn render(frame: &mut Frame<'_>, state: &DashboardState) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(frame.area());

    render_header(frame, sections[0]);
    render_content(frame, sections[1], state);
    render_footer(frame, sections[2], state.status_message());
}

fn render_header(frame: &mut Frame<'_>, area: Rect) {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S");
    let line = Line::from(vec![
        Span::styled(
            "AstraOS",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "  v{}  •  Overview  •  {now}",
            env!("CARGO_PKG_VERSION")
        )),
    ]);

    frame.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn render_content(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    if area.width < NARROW_LAYOUT_WIDTH {
        let panels = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(10),
                Constraint::Length(5),
                Constraint::Min(4),
            ])
            .split(area);

        render_system(frame, panels[0], state.snapshot());
        render_services(frame, panels[1], state.snapshot().services);
        render_workspaces(frame, panels[2], state);
    } else {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
            .split(area);
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(4)])
            .split(columns[1]);

        render_system(frame, columns[0], state.snapshot());
        render_services(frame, right[0], state.snapshot().services);
        render_workspaces(frame, right[1], state);
    }
}

fn render_system(frame: &mut Frame<'_>, area: Rect, snapshot: &SystemSnapshot) {
    let lines = vec![
        metric_line(
            "OS",
            snapshot
                .operating_system
                .as_deref()
                .unwrap_or("unavailable"),
        ),
        metric_line(
            "Hostname",
            snapshot.hostname.as_deref().unwrap_or("unavailable"),
        ),
        metric_line(
            "CPU",
            &snapshot
                .cpu
                .map(|cpu| format!("{:.1}%", cpu.usage_percent))
                .unwrap_or_else(|| "unavailable".to_string()),
        ),
        metric_line(
            "Memory",
            &snapshot
                .memory
                .map(|memory| format_usage(memory.used_bytes, memory.total_bytes))
                .unwrap_or_else(|| "unavailable".to_string()),
        ),
        metric_line(
            "Disk",
            &snapshot
                .disk
                .as_ref()
                .map(|disk| format_usage(disk.used_bytes, disk.total_bytes))
                .unwrap_or_else(|| "unavailable".to_string()),
        ),
        metric_line("Battery", &format_battery(snapshot.battery)),
        metric_line(
            "Uptime",
            &snapshot
                .uptime
                .map(format_duration)
                .unwrap_or_else(|| "unavailable".to_string()),
        ),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(" System ").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn metric_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_string()),
    ])
}

fn render_services(frame: &mut Frame<'_>, area: Rect, services: DeveloperServicesSnapshot) {
    let lines = vec![
        service_line("Docker", services.docker),
        service_line("Ollama", services.ollama),
    ];

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Developer Services ")
                .borders(Borders::ALL),
        ),
        area,
    );
}

fn service_line(name: &str, status: ServiceStatus) -> Line<'static> {
    let (label, color) = match status {
        ServiceStatus::Running => ("Running", Color::Green),
        ServiceStatus::Stopped => ("Stopped", Color::Yellow),
        ServiceStatus::Unavailable => ("Unavailable", Color::DarkGray),
        ServiceStatus::Unknown => ("Unknown", Color::Magenta),
    };

    Line::from(vec![
        Span::styled(
            format!("{name}: "),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(label, Style::default().fg(color)),
    ])
}

fn render_workspaces(frame: &mut Frame<'_>, area: Rect, state: &DashboardState) {
    let title = format!(" Workspaces ({}) ", state.workspaces().len());
    let block = Block::default().title(title).borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.workspaces().is_empty() {
        frame.render_widget(
            Paragraph::new("No configured workspaces").style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let path_width = usize::from(inner.width.saturating_sub(4));
    let items = state
        .workspaces()
        .iter()
        .map(|workspace| {
            let path = workspace.path.to_string_lossy();
            ListItem::new(vec![
                Line::from(Span::styled(
                    workspace.name.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!("  {}", truncate_text(&path, path_width)),
                    Style::default().fg(Color::DarkGray),
                )),
            ])
        })
        .collect::<Vec<_>>();
    let mut list_state = ListState::default().with_selected(state.selected_workspace());
    let list = List::new(items)
        .highlight_symbol("› ")
        .highlight_style(Style::default().fg(Color::Cyan));

    frame.render_stateful_widget(list, inner, &mut list_state);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, status_message: Option<&str>) {
    let controls = "q/Esc: quit  •  r: refresh now  •  ↑/↓ or j/k: select workspace";
    let content = match status_message {
        Some(message) => format!("{controls}  •  {message}"),
        None => controls.to_string(),
    };

    frame.render_widget(
        Paragraph::new(content).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn format_usage(used_bytes: u64, total_bytes: u64) -> String {
    format!(
        "{} / {}",
        format_bytes(used_bytes),
        format_bytes(total_bytes)
    )
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let mut value = bytes as f64;
    let mut unit_index = 0;

    while value >= 1024.0 && unit_index < UNITS.len().saturating_sub(1) {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit_index])
    }
}

fn format_battery(battery: Option<BatterySnapshot>) -> String {
    let Some(battery) = battery else {
        return "unavailable".to_string();
    };
    let state = match battery.state {
        BatteryState::Charging => "charging",
        BatteryState::Discharging => "discharging",
        BatteryState::Empty => "empty",
        BatteryState::Full => "full",
        BatteryState::Unknown => "unknown",
    };

    format!("{:.0}% — {state}", battery.charge_percent)
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;

    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

fn truncate_text(value: &str, max_characters: usize) -> String {
    let character_count = value.chars().count();

    if character_count <= max_characters {
        return value.to_string();
    }
    if max_characters == 0 {
        return String::new();
    }
    if max_characters == 1 {
        return "…".to_string();
    }

    let visible = value
        .chars()
        .take(max_characters.saturating_sub(1))
        .collect::<String>();
    format!("{visible}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_human_readable_bytes_and_usage() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1_610_612_736), "1.5 GB");
        assert_eq!(
            format_usage(6 * 1024 * 1024 * 1024, 16 * 1024 * 1024 * 1024),
            "6.0 GB / 16.0 GB"
        );
    }

    #[test]
    fn formats_uptime_without_overflowing_units() {
        assert_eq!(format_duration(Duration::from_secs(90)), "1m");
        assert_eq!(
            format_duration(Duration::from_secs(2 * 86_400 + 3 * 3_600 + 4 * 60)),
            "2d 3h 4m"
        );
    }

    #[test]
    fn formats_available_and_unavailable_battery_states() {
        assert_eq!(format_battery(None), "unavailable");
        assert_eq!(
            format_battery(Some(BatterySnapshot {
                charge_percent: 74.2,
                state: BatteryState::Charging,
            })),
            "74% — charging"
        );
    }

    #[test]
    fn truncates_unicode_without_slicing_utf8_bytes() {
        assert_eq!(truncate_text("workspace", 6), "works…");
        assert_eq!(truncate_text("Astra 🚀 workspace", 8), "Astra 🚀…");
        assert_eq!(truncate_text("🚀", 1), "🚀");
        assert_eq!(truncate_text("🚀x", 1), "…");
        assert_eq!(truncate_text("anything", 0), "");
    }
}
