use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Wrap,
};
use ratatui::Frame;

use crate::app::{App, DashboardFocus, DateInputMode, Mode, SettingsFocus, SettingsItem};
use crate::storage::ThemePreference;
use crate::update;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.area();
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
        Mode::Updating => draw_overlay(frame, size, "Installing update...", &theme),
        Mode::Login => draw_login(frame, app, size, &theme),
        Mode::WorkspaceSelect => draw_workspace_select(frame, app, size, &theme),
        Mode::DateInput(mode) => draw_date_input(frame, app, size, mode, &theme),
        Mode::Settings => draw_settings(frame, app, size, &theme),
        Mode::UpdatePrompt => draw_update_prompt(frame, app, size, &theme),
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

fn draw_update_prompt(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let block = centered_rect(70, 35, area);
    frame.render_widget(Clear, block);

    let current = update::current_version();
    let (latest, tag) = app
        .update_info
        .as_ref()
        .map(|info| (format!("v{}", info.latest), info.tag.clone()))
        .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));

    let mut lines = vec![
        Line::from(Span::styled(
            "Update Available",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(theme.accent),
        )),
        Line::from(""),
        Line::from(format!("Current version: v{}", current)),
        Line::from(format!("Latest version:  {}", latest)),
        Line::from(format!("Release tag:     {}", tag)),
        Line::from(""),
        Line::from("This update is required to continue."),
        Line::from("Press u to update now, q to quit."),
    ];

    if let Some(error) = app.update_error.as_ref() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            error,
            Style::default().fg(Color::Red),
        )));
    }

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(panel_block("Update Required", theme))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, block);
}

fn draw_dashboard(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let content = area.inner(Margin {
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

    let active_highlight = Style::default()
        .bg(theme.accent)
        .fg(theme.accent_contrast())
        .add_modifier(Modifier::BOLD);
    let inactive_highlight = Style::default()
        .fg(theme.highlight)
        .add_modifier(Modifier::BOLD);

    let (project_highlight_style, project_highlight_symbol) = match app.dashboard_focus {
        DashboardFocus::Projects => (active_highlight, "▍ "),
        DashboardFocus::Entries => (inactive_highlight, "▏ "),
    };

    let (entry_highlight_style, entry_highlight_symbol) = match app.dashboard_focus {
        DashboardFocus::Entries => (active_highlight, "▍ "),
        DashboardFocus::Projects => (inactive_highlight, "▏ "),
    };

    let project_list = List::new(project_items)
        .block(panel_block("Projects", theme))
        .highlight_style(project_highlight_style)
        .highlight_symbol(project_highlight_symbol);

    frame.render_stateful_widget(project_list, body[0], &mut app.project_state);

    let current_project = app
        .project_state
        .selected()
        .and_then(|index| app.grouped.get(index));
    let entry_items: Vec<ListItem> = if let Some(project) = current_project {
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

    let entries_list = List::new(entry_items)
        .block(panel_block("Entries", theme))
        .highlight_style(entry_highlight_style)
        .highlight_symbol(entry_highlight_symbol);

    frame.render_stateful_widget(entries_list, body[1], &mut app.entry_state);

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
        Row::new(vec![
            Cell::from(Span::styled("Right / Tab", key_style)),
            Cell::from("Switch to entries"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Left / Shift+Tab", key_style)),
            Cell::from("Switch to projects"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Enter", key_style)),
            Cell::from("Browse entries"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Esc", key_style)),
            Cell::from("Back to projects"),
        ]),
        Row::new(vec![Cell::from(""), Cell::from("")]),
        Row::new(vec![
            Cell::from(Span::styled("Entries", header_style)),
            Cell::from(""),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Up/Down", key_style)),
            Cell::from("Select entry"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("b", key_style)),
            Cell::from("Copy entry title"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("n", key_style)),
            Cell::from("Copy entry hours"),
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
    let block = centered_rect(80, 60, area);
    frame.render_widget(Clear, block);

    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(10)])
        .split(block);

    let active_highlight = Style::default()
        .bg(theme.accent)
        .fg(theme.accent_contrast())
        .add_modifier(Modifier::BOLD);
    let inactive_highlight = Style::default().fg(theme.highlight).add_modifier(Modifier::BOLD);

    let (category_highlight, category_symbol) = match app.settings_focus() {
        SettingsFocus::Categories => (active_highlight, "▍ "),
        _ => (inactive_highlight, "▏ "),
    };

    let (items_highlight, items_symbol) = match app.settings_focus() {
        SettingsFocus::Categories => (inactive_highlight, "▏ "),
        SettingsFocus::Items | SettingsFocus::Edit => (active_highlight, "▍ "),
    };

    let categories = List::new(
        app.settings_categories()
            .iter()
            .cloned()
            .map(|category| ListItem::new(Line::from(category)))
            .collect::<Vec<_>>(),
    )
    .block(panel_block("Categories", theme))
    .highlight_style(category_highlight)
    .highlight_symbol(category_symbol);

    frame.render_stateful_widget(categories, sections[0], app.settings_state());

    let right_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(5)])
        .split(sections[1]);

    let rounding_enabled = app.settings_rounding_enabled_display();
    let rounding_cfg = app.settings_rounding_config_display();
    let editing_item = app.settings_edit_item();
    let is_editing = matches!(app.settings_focus(), SettingsFocus::Edit);

    let item_rows: Vec<ListItem> = app
        .settings_items()
        .iter()
        .copied()
        .map(|item| {
            let (label, value, disabled) = match item {
                SettingsItem::Theme => {
                    let theme = app.settings_theme_display();
                    ("Theme", theme_label(theme).to_string(), false)
                }
                SettingsItem::TargetHours => {
                    let value = if is_editing && editing_item == Some(SettingsItem::TargetHours) {
                        app.settings_input_value().to_string()
                    } else {
                        format!("{:.2}h", app.target_hours)
                    };
                    ("Target hours", value, false)
                }
                SettingsItem::TimeRoundingToggle => {
                    let value = if rounding_enabled { "On" } else { "Off" }.to_string();
                    ("Time rounding", value, false)
                }
                SettingsItem::RoundingIncrement => {
                    let value = rounding_cfg
                        .map(|cfg| format!("{:.2}h", cfg.increment_minutes as f64 / 60.0))
                        .unwrap_or_else(|| "—".to_string());
                    ("Rounding increment", value, rounding_cfg.is_none())
                }
                SettingsItem::RoundingMode => {
                    let value = rounding_cfg
                        .map(|cfg| match cfg.mode {
                            crate::rounding::RoundingMode::Closest => "closest".to_string(),
                            crate::rounding::RoundingMode::Up => "up".to_string(),
                            crate::rounding::RoundingMode::Down => "down".to_string(),
                        })
                        .unwrap_or_else(|| "—".to_string());
                    ("Rounding mode", value, rounding_cfg.is_none())
                }
                SettingsItem::TogglToken => {
                    let value = if is_editing && editing_item == Some(SettingsItem::TogglToken) {
                        app.settings_input_value().to_string()
                    } else {
                        app.token
                            .as_deref()
                            .map(mask_token)
                            .unwrap_or_else(|| "Not set".to_string())
                    };
                    ("Toggl token", value, false)
                }
            };

            let value_style = if is_editing && editing_item == Some(item) {
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
            } else if disabled {
                theme.muted_style()
            } else {
                theme.muted_style()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{label}: "), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(value, value_style),
            ]))
            .style(theme.panel_style())
        })
        .collect();

    let items_list = List::new(item_rows)
        .block(panel_block(app.settings_selected_category(), theme))
        .highlight_style(items_highlight)
        .highlight_symbol(items_symbol);

    frame.render_stateful_widget(items_list, right_sections[0], app.settings_item_state());

    let hint_text = match app.settings_focus() {
        SettingsFocus::Categories => "Enter items • Esc close",
        SettingsFocus::Items => "Up/Down select • Enter edit • Esc categories",
        SettingsFocus::Edit => match editing_item {
            Some(SettingsItem::TargetHours) | Some(SettingsItem::TogglToken) => "Enter save • Esc cancel",
            Some(SettingsItem::Theme) => "Up/Down change • Enter save • Esc cancel",
            Some(SettingsItem::TimeRoundingToggle)
            | Some(SettingsItem::RoundingIncrement)
            | Some(SettingsItem::RoundingMode) => "Up/Down change • Enter save • Esc cancel",
            None => "Esc cancel",
        },
    };

    let mut hint_lines = vec![Line::from(hint_text)];
    if let Some(status) = app.visible_status() {
        let is_success = is_success_status(&status);
        let color = if is_success { theme.success } else { theme.error };
        hint_lines.push(Line::from(""));
        hint_lines.push(Line::from(Span::styled(status, Style::default().fg(color))));
    }

    let hint = Paragraph::new(hint_lines)
        .alignment(Alignment::Left)
        .block(panel_block("Hint", theme))
        .wrap(Wrap { trim: true });
    frame.render_widget(hint, right_sections[1]);
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

fn mask_token(token: &str) -> String {
    if token.is_empty() {
        return "Not set".to_string();
    }
    let len = token.chars().count();
    if len <= 4 {
        return "••••".to_string();
    }
    let tail: String = token.chars().skip(len - 4).collect();
    format!("••••{tail}")
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
        ThemePreference::Terminal => Theme {
            bg: Color::Reset,
            panel: Color::Reset,
            border: Color::DarkGray,
            text: Color::Reset,
            muted: Color::DarkGray,
            accent: Color::Blue,
            highlight: Color::Yellow,
            success: Color::Green,
            error: Color::Red,
            accent_dark: Color::Black,
        },
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

fn theme_label(theme: ThemePreference) -> &'static str {
    match theme {
        ThemePreference::Terminal => "Terminal",
        ThemePreference::Dark => "Midnight",
        ThemePreference::Light => "Snow",
    }
}
