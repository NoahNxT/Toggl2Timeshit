use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, DateInputMode, Mode};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.size();
    draw_dashboard(frame, app, size);

    match app.mode {
        Mode::Loading => draw_overlay(frame, size, "Loading data from Toggl..."),
        Mode::Error => draw_overlay(frame, size, app.status.as_deref().unwrap_or("Unknown error")),
        Mode::Login => draw_login(frame, app, size),
        Mode::WorkspaceSelect => draw_workspace_select(frame, app, size),
        Mode::DateInput(mode) => draw_date_input(frame, app, size, mode),
        Mode::Dashboard => {}
    }

    if matches!(app.mode, Mode::Dashboard) {
        if let Some(toast) = app.active_toast() {
            draw_toast(frame, size, &toast.message, toast.is_error);
        }
    }

    if app.show_help {
        draw_help(frame, size);
    }
}

fn draw_dashboard(frame: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    let header = header_line(app);
    let header_block = Paragraph::new(header)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL).title("Timeshit TUI"));
    frame.render_widget(header_block, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(chunks[1]);

    let project_items: Vec<ListItem> = app
        .grouped
        .iter()
        .map(|group| {
            let line = Line::from(vec![
                Span::styled(&group.name, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("  {:.2}h", group.total_hours)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let project_list = List::new(project_items)
        .block(Block::default().borders(Borders::ALL).title("Projects"))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .highlight_symbol("➤ ");

    frame.render_stateful_widget(project_list, body[0], &mut app.project_state);

    let entry_items: Vec<ListItem> = if let Some(project) = app.current_project() {
        project
            .entries
            .iter()
            .map(|entry| {
                ListItem::new(Line::from(vec![
                    Span::raw(&entry.description),
                    Span::raw(format!("  {:.2}h", entry.total_hours)),
                ]))
            })
            .collect()
    } else {
        vec![ListItem::new(Line::from("No entries"))]
    };

    let entries_block = List::new(entry_items)
        .block(Block::default().borders(Borders::ALL).title("Entries"));

    frame.render_widget(entries_block, body[1]);

    let footer = footer_line(app);
    let footer_block = Paragraph::new(footer)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer_block, chunks[2]);
}

fn header_line(app: &App) -> Line<'static> {
    let workspace = app
        .selected_workspace
        .as_ref()
        .map(|w| w.name.clone())
        .unwrap_or_else(|| "No workspace".to_string());
    let last_refresh = app
        .last_refresh
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "Never".to_string());
    Line::from(vec![
        Span::styled("Workspace: ", Style::default().fg(Color::Gray)),
        Span::styled(workspace, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Date: ", Style::default().fg(Color::Gray)),
        Span::raw(app.date_range.label().to_string()),
        Span::raw("  "),
        Span::styled("Last refresh: ", Style::default().fg(Color::Gray)),
        Span::raw(last_refresh),
    ])
}

fn footer_line(app: &App) -> Line<'static> {
    let total_style = if app.total_hours < 8.0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    };

    let status = app.status.clone().unwrap_or_default();
    Line::from(vec![
        Span::styled(format!("Total: {:.2}h", app.total_hours), total_style),
        Span::raw("  "),
        Span::styled(
            "h help • Enter copy • p copy+project",
            Style::default().fg(Color::Gray),
        ),
        if status.is_empty() {
            Span::raw("")
        } else {
            Span::raw(format!("  |  {}", status))
        },
    ])
}

fn draw_overlay(frame: &mut Frame, area: Rect, message: &str) {
    let block = centered_rect(60, 20, area);
    frame.render_widget(Clear, block);
    let paragraph = Paragraph::new(message)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Status"))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, block);
}

fn draw_login(frame: &mut Frame, app: &App, area: Rect) {
    let block = centered_rect(70, 30, area);
    frame.render_widget(Clear, block);
    let mut lines = vec![
        Line::from("Enter your Toggl API token"),
        Line::from("Find it in https://track.toggl.com/profile"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Token: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&app.input),
        ]),
        Line::from(""),
        Line::from("Press Enter to save, q to quit"),
    ];

    if let Some(status) = &app.status {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(status, Style::default().fg(Color::Red))));
    }

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL).title("Login"))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, block);
}

fn draw_workspace_select(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = centered_rect(60, 60, area);
    frame.render_widget(Clear, block);

    let items: Vec<ListItem> = app
        .workspace_list
        .iter()
        .map(|workspace| ListItem::new(Line::from(workspace.name.clone())))
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Select Workspace"))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .highlight_symbol("➤ ");

    frame.render_stateful_widget(list, block, &mut app.workspace_state);
}

fn draw_date_input(frame: &mut Frame, app: &App, area: Rect, mode: DateInputMode) {
    let block = centered_rect(60, 30, area);
    frame.render_widget(Clear, block);

    let label = match mode {
        DateInputMode::Single => "Enter date (YYYY-MM-DD)",
        DateInputMode::Start => "Enter start date (YYYY-MM-DD)",
        DateInputMode::End => "Enter end date (YYYY-MM-DD)",
    };

    let mut lines = vec![
        Line::from(label),
        Line::from(""),
        Line::from(vec![
            Span::styled("Input: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&app.input),
        ]),
        Line::from(""),
        Line::from("Press Enter to apply, Esc to cancel"),
    ];

    if let Some(status) = &app.status {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(status, Style::default().fg(Color::Red))));
    }

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL).title("Date Filter"))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, block);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    let vertical = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1]);
    vertical[1]
}

fn draw_toast(frame: &mut Frame, area: Rect, message: &str, is_error: bool) {
    let width = (message.len() as u16 + 6).clamp(20, area.width.saturating_sub(2));
    let height = 3;
    let x = area.x + area.width.saturating_sub(width + 1);
    let y = area.y + area.height.saturating_sub(height + 4);
    let rect = Rect::new(x, y, width, height);

    frame.render_widget(Clear, rect);
    let style = if is_error {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    };
    let paragraph = Paragraph::new(Line::from(Span::styled(message, style)))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Copied"));
    frame.render_widget(paragraph, rect);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let block = centered_rect(70, 70, area);
    frame.render_widget(Clear, block);

    let lines = vec![
        Line::from(Span::styled("Navigation", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("Up/Down: Select project"),
        Line::from(""),
        Line::from(Span::styled("Dates", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("t: Today"),
        Line::from("d: Set single date"),
        Line::from("s: Set start date"),
        Line::from("e: Set end date"),
        Line::from(""),
        Line::from(Span::styled("Clipboard", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("Enter: Copy entries"),
        Line::from("Shift+Enter: Copy with project names (if supported)"),
        Line::from("p: Copy with project names"),
        Line::from(""),
        Line::from(Span::styled("General", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("r: Refresh"),
        Line::from("h or Esc: Close help"),
        Line::from("q: Quit"),
    ];

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, block);
}
