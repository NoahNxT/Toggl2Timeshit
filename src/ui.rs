use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Wrap,
};
use ratatui::Frame;

use crate::app::{App, DateInputMode, Mode};
use crate::storage::ThemePreference;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.size();
    let theme = theme_from(app.theme);
    draw_background(frame, size, &theme);
    draw_dashboard(frame, app, size, &theme);

    match app.mode {
        Mode::Loading => draw_overlay(frame, size, "Loading data from Toggl...", &theme),
        Mode::Error => draw_overlay(
            frame,
            size,
            app.status.as_deref().unwrap_or("Unknown error"),
            &theme,
        ),
        Mode::Login => draw_login(frame, app, size, &theme),
        Mode::WorkspaceSelect => draw_workspace_select(frame, app, size, &theme),
        Mode::DateInput(mode) => draw_date_input(frame, app, size, mode, &theme),
        Mode::Settings => draw_settings(frame, app, size, &theme),
        Mode::Dashboard => {}
    }

    if matches!(app.mode, Mode::Dashboard) && !app.show_help {
        if let Some(toast) = app.active_toast() {
            draw_toast(frame, size, &toast.message, toast.is_error, &theme);
        }
    }

    if app.show_help {
        draw_help(frame, size, &theme);
    }
}

fn draw_dashboard(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let content = area.inner(&Margin {
        vertical: 1,
        horizontal: 2,
    });

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(2)])
        .split(content);

    let header = header_line(app, theme);
    let header_block = Paragraph::new(header)
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(theme.border_style())
                .style(theme.panel_style()),
        );
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
                Span::styled(&group.display_name, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("  {:.2}h", group.total_hours),
                    theme.muted_style(),
                ),
            ]);
            ListItem::new(line).style(theme.panel_style())
        })
        .collect();

    let project_list = List::new(project_items)
        .block(panel_block("Projects", theme))
        .highlight_style(
            Style::default()
                .bg(theme.accent)
                .fg(theme.accent_contrast())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▍ ");

    frame.render_stateful_widget(project_list, body[0], &mut app.project_state);

    let entry_items: Vec<ListItem> = if let Some(project) = app.current_project() {
        project
            .entries
            .iter()
            .map(|entry| {
                ListItem::new(Line::from(vec![
                    Span::raw(&entry.description),
                    Span::styled(
                        format!("  {:.2}h", entry.total_hours),
                        theme.muted_style(),
                    ),
                ]))
                .style(theme.panel_style())
            })
            .collect()
    } else {
        vec![ListItem::new(Line::from("No entries")).style(theme.panel_style())]
    };

    let entries_block = List::new(entry_items).block(panel_block("Entries", theme));

    frame.render_widget(entries_block, body[1]);

    let footer = footer_line(app, theme);
    let footer_block = Paragraph::new(footer)
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(theme.border_style())
                .style(theme.panel_style()),
        );
    frame.render_widget(footer_block, chunks[2]);
}

fn header_line(app: &App, theme: &Theme) -> Line<'static> {
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
        Span::styled("Timeshit", theme.title_style()),
        Span::raw("  "),
        Span::styled("Workspace", theme.muted_style()),
        Span::raw(": "),
        Span::styled(workspace, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Date", theme.muted_style()),
        Span::raw(": "),
        Span::raw(app.date_range.label().to_string()),
        Span::raw("  "),
        Span::styled("Last refresh", theme.muted_style()),
        Span::raw(": "),
        Span::raw(last_refresh),
    ])
}

fn footer_line(app: &mut App, theme: &Theme) -> Line<'static> {
    let total_style = if app.total_hours < app.target_hours {
        Style::default().fg(theme.error).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.success).add_modifier(Modifier::BOLD)
    };

    let status = app.visible_status().unwrap_or_default();
    Line::from(vec![
        Span::styled(format!("Total {:.2}h", app.total_hours), total_style),
        Span::raw("   "),
        Span::styled("h help", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("s settings", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("q quit", theme.muted_style()),
        if status.is_empty() {
            Span::raw("")
        } else {
            Span::raw(format!("   |   {}", status))
        },
    ])
}

fn draw_overlay(frame: &mut Frame, area: Rect, message: &str, theme: &Theme) {
    let block = centered_rect(60, 20, area);
    frame.render_widget(Clear, block);
    let paragraph = Paragraph::new(message)
        .alignment(Alignment::Center)
        .block(panel_block("Status", theme))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, block);
}

fn draw_login(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
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
        .block(panel_block("Login", theme))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, block);
}

fn draw_workspace_select(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let block = centered_rect(60, 60, area);
    frame.render_widget(Clear, block);

    let items: Vec<ListItem> = app
        .workspace_list
        .iter()
        .map(|workspace| ListItem::new(Line::from(workspace.name.clone())))
        .collect();

    let list = List::new(items)
        .block(panel_block("Select Workspace", theme))
        .highlight_style(
            Style::default()
                .bg(theme.accent)
                .fg(theme.accent_contrast())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▍ ");

    frame.render_stateful_widget(list, block, &mut app.workspace_state);
}

fn draw_date_input(frame: &mut Frame, app: &App, area: Rect, mode: DateInputMode, theme: &Theme) {
    let block = centered_rect(60, 30, area);
    frame.render_widget(Clear, block);

    let label = match mode {
        DateInputMode::Range => "Select date range (YYYY-MM-DD)",
    };

    let start_value = if app.is_date_start_active() {
        Span::styled(app.date_start_input_value(), Style::default().fg(theme.accent))
    } else {
        Span::raw(app.date_start_input_value())
    };
    let end_value = if app.is_date_start_active() {
        Span::raw(app.date_end_input_value())
    } else {
        Span::styled(app.date_end_input_value(), Style::default().fg(theme.accent))
    };

    let mut lines = vec![
        Line::from(label),
        Line::from(""),
        Line::from(vec![
            Span::styled("Start: ", Style::default().add_modifier(Modifier::BOLD)),
            start_value,
        ]),
        Line::from(vec![
            Span::styled("End:   ", Style::default().add_modifier(Modifier::BOLD)),
            end_value,
        ]),
        Line::from(""),
        Line::from("Tab to switch field • Enter apply • Esc cancel"),
    ];

    if let Some(status) = &app.status {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(status, Style::default().fg(Color::Red))));
    }

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(panel_block("Date Filter", theme))
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

fn draw_toast(frame: &mut Frame, area: Rect, message: &str, is_error: bool, theme: &Theme) {
    let width = (message.len() as u16 + 6).clamp(20, area.width.saturating_sub(2));
    let height = 3;
    let x = area.x + area.width.saturating_sub(width + 1);
    let y = area.y + area.height.saturating_sub(height + 4);
    let rect = Rect::new(x, y, width, height);

    frame.render_widget(Clear, rect);
    let style = if is_error {
        Style::default().fg(theme.error).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.success).add_modifier(Modifier::BOLD)
    };
    let paragraph = Paragraph::new(Line::from(Span::styled(message, style)))
        .alignment(Alignment::Center)
        .block(panel_block("Copied", theme));
    frame.render_widget(paragraph, rect);
}

fn draw_help(frame: &mut Frame, area: Rect, theme: &Theme) {
    let block = centered_rect(70, 70, area);
    frame.render_widget(Clear, block);

    let header_style = Style::default().add_modifier(Modifier::BOLD).fg(theme.accent);
    let key_style = Style::default().fg(theme.highlight);

    let rows = vec![
        Row::new(vec![
            Cell::from(Span::styled("Navigation", header_style)),
            Cell::from(""),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Up/Down", key_style)),
            Cell::from("Select project"),
        ]),
        Row::new(vec![Cell::from(""), Cell::from("")]),
        Row::new(vec![
            Cell::from(Span::styled("Dates", header_style)),
            Cell::from(""),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("t", key_style)),
            Cell::from("Today"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("y", key_style)),
            Cell::from("Yesterday"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("d", key_style)),
            Cell::from("Set date range"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Tab", key_style)),
            Cell::from("Switch range field"),
        ]),
        Row::new(vec![Cell::from(""), Cell::from("")]),
        Row::new(vec![
            Cell::from(Span::styled("Clipboard", header_style)),
            Cell::from(""),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("c", key_style)),
            Cell::from("Copy client entries on current date"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("v", key_style)),
            Cell::from("Copy project entries on current date"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("x", key_style)),
            Cell::from("Copy all entries on current date with project names"),
        ]),
        Row::new(vec![Cell::from(""), Cell::from("")]),
        Row::new(vec![
            Cell::from(Span::styled("General", header_style)),
            Cell::from(""),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("r", key_style)),
            Cell::from("Refresh"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("s", key_style)),
            Cell::from("Settings"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("m", key_style)),
            Cell::from("Toggle theme"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("h / Esc", key_style)),
            Cell::from("Close help"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("q", key_style)),
            Cell::from("Quit"),
        ]),
    ];

    let table = Table::new(rows, [Constraint::Length(20), Constraint::Min(10)])
        .block(panel_block("Help", theme))
        .column_spacing(2);

    frame.render_widget(table, block);
}

fn draw_settings(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let block = centered_rect(70, 40, area);
    frame.render_widget(Clear, block);

    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(18), Constraint::Min(10)])
        .split(block);

    let selected_style = Style::default()
        .bg(theme.accent)
        .fg(theme.accent_contrast())
        .add_modifier(Modifier::BOLD);

    let categories = List::new(
        app.settings_categories()
            .iter()
            .cloned()
            .map(|category| ListItem::new(Line::from(category)))
            .collect::<Vec<_>>(),
    )
    .block(panel_block("Categories", theme))
    .highlight_style(selected_style)
    .highlight_symbol("▍ ");

    frame.render_stateful_widget(categories, sections[0], app.settings_state());

    let selected_category = app.settings_selected_category().to_string();
    let settings_input = app.settings_input_value().to_string();
    let is_editing = app.settings_is_editing();

    let hint = if is_editing {
        "Enter to save • Esc back"
    } else {
        "Enter to edit • Esc close"
    };

    let mut lines = vec![
        Line::from(Span::styled(
            selected_category,
            Style::default().add_modifier(Modifier::BOLD).fg(theme.accent),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Target hours: ", Style::default().add_modifier(Modifier::BOLD)),
            if is_editing {
                Span::styled(settings_input, Style::default().fg(theme.accent))
            } else {
                Span::raw(settings_input)
            },
            Span::raw(" h"),
        ]),
        Line::from(""),
        Line::from(hint),
    ];

    if let Some(status) = app.visible_status() {
        let is_success = is_success_status(&status);
        let color = if is_success { theme.success } else { theme.error };
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            status,
            Style::default().fg(color),
        )));
    }

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(panel_block("General", theme))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, sections[1]);
}

fn draw_background(frame: &mut Frame, area: Rect, theme: &Theme) {
    let block = Block::default().style(Style::default().bg(theme.bg).fg(theme.text));
    frame.render_widget(block, area);
}

fn panel_block(title: &str, theme: &Theme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style())
        .style(theme.panel_style())
        .title(Line::from(Span::styled(
            format!(" {} ", title),
            theme.title_style(),
        )))
}

fn is_success_status(status: &str) -> bool {
    let lower = status.to_lowercase();
    lower.contains("updated")
        || lower.contains("saved")
        || lower.contains("success")
        || lower.contains("copied")
        || lower.contains("set to")
}

#[derive(Clone, Copy)]
struct Theme {
    bg: Color,
    panel: Color,
    border: Color,
    text: Color,
    muted: Color,
    accent: Color,
    highlight: Color,
    success: Color,
    error: Color,
    accent_dark: Color,
}

impl Theme {
    fn panel_style(&self) -> Style {
        Style::default().bg(self.panel).fg(self.text)
    }

    fn border_style(&self) -> Style {
        Style::default().fg(self.border)
    }

    fn title_style(&self) -> Style {
        Style::default().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    fn muted_style(&self) -> Style {
        Style::default().fg(self.muted)
    }

    fn accent_contrast(&self) -> Color {
        if matches!(self.bg, Color::Rgb(242, 244, 248)) {
            self.accent_dark
        } else {
            Color::Black
        }
    }
}

fn theme_from(pref: ThemePreference) -> Theme {
    match pref {
        ThemePreference::Dark => Theme {
            bg: Color::Rgb(12, 18, 36),
            panel: Color::Rgb(18, 28, 52),
            border: Color::Rgb(44, 72, 112),
            text: Color::Rgb(220, 230, 255),
            muted: Color::Rgb(150, 170, 200),
            accent: Color::Rgb(90, 180, 255),
            highlight: Color::Rgb(255, 210, 120),
            success: Color::Rgb(120, 220, 140),
            error: Color::Rgb(255, 120, 120),
            accent_dark: Color::Rgb(26, 60, 110),
        },
        ThemePreference::Light => Theme {
            bg: Color::Rgb(242, 244, 248),
            panel: Color::Rgb(255, 255, 255),
            border: Color::Rgb(210, 220, 235),
            text: Color::Rgb(26, 32, 44),
            muted: Color::Rgb(90, 110, 140),
            accent: Color::Rgb(70, 130, 235),
            highlight: Color::Rgb(255, 165, 80),
            success: Color::Rgb(36, 150, 90),
            error: Color::Rgb(220, 60, 80),
            accent_dark: Color::Rgb(18, 34, 64),
        },
    }
}
