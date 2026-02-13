use chrono::{Datelike, Duration, Local, NaiveDate};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Wrap,
};
use std::collections::{HashMap, HashSet};

use crate::app::{
    App, DashboardFocus, DateInputMode, Mode, RollupFocus, RollupView, SettingsFocus, SettingsItem,
};
use crate::rollups::WeekStart;
use crate::rollups::{DailyTotal, PeriodRollup};
use crate::storage::ThemePreference;
use crate::update;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.area();
    let theme = theme_from(app.theme);
    draw_background(frame, size, &theme);
    if matches!(app.mode, Mode::Rollups | Mode::RefetchConfirm) {
        draw_rollups(frame, app, size, &theme);
    } else {
        draw_dashboard(frame, app, size, &theme);
    }

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
        Mode::RefetchConfirm => draw_refetch_confirm(frame, app, size, &theme),
        Mode::Dashboard | Mode::Rollups => {}
    }

    if matches!(app.mode, Mode::Dashboard | Mode::Rollups) && !app.show_help {
        if let Some(toast) = app.active_toast() {
            draw_toast(frame, size, &toast.message, toast.is_error, &theme);
        }
    }

    if app.show_help {
        draw_help(frame, app, size, &theme);
    }
}

fn draw_dashboard(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let content = area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(content);

    let header = header_line(app, theme);
    let header_block = Paragraph::new(header).alignment(Alignment::Left).block(
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
                Span::styled(
                    &group.display_name,
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {:.2}h", group.total_hours), theme.muted_style()),
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
                    Span::styled(format!("  {:.2}h", entry.total_hours), theme.muted_style()),
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
    let footer_block = Paragraph::new(footer).alignment(Alignment::Left).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(theme.border_style())
            .style(theme.panel_style()),
    );
    frame.render_widget(footer_block, chunks[2]);
}

fn draw_rollups(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let content = area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(content);

    let header = rollups_header_line(app, theme);
    let header_block = Paragraph::new(header).alignment(Alignment::Left).block(
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

    let periods = match app.rollup_view {
        RollupView::Weekly => &app.rollups.weekly,
        RollupView::Monthly => &app.rollups.monthly,
        RollupView::Yearly => &app.rollups.yearly,
    };

    let period_items: Vec<ListItem> = if periods.is_empty() {
        vec![ListItem::new(Line::from("No rollup data")).style(theme.panel_style())]
    } else {
        periods
            .iter()
            .map(|period| {
                let hours = hours_from_seconds(period.seconds);
                let (target, _) =
                    period_target_hours(period, app.target_hours, app.rollups_include_weekends);
                let delta = normalize_delta(hours - target);
                let delta_style = delta_style(delta, theme);
                let line = Line::from(vec![
                    Span::styled(&period.label, Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw("  "),
                    Span::styled(format!("{:.2}h", hours), theme.muted_style()),
                    Span::styled(format!("  {:+.2}h", delta), delta_style),
                ]);
                ListItem::new(line).style(theme.panel_style())
            })
            .collect()
    };

    let active_highlight = Style::default()
        .bg(theme.accent)
        .fg(theme.accent_contrast())
        .add_modifier(Modifier::BOLD);
    let inactive_highlight = Style::default()
        .fg(theme.highlight)
        .add_modifier(Modifier::BOLD);

    let (period_highlight_style, period_highlight_symbol) = match app.rollup_focus {
        RollupFocus::Periods => (active_highlight, "▍ "),
        RollupFocus::Days => (inactive_highlight, "▏ "),
    };

    let period_title = match app.rollup_view {
        RollupView::Weekly => "Weeks",
        RollupView::Monthly => "Months",
        RollupView::Yearly => "Years",
    };

    let period_list = List::new(period_items)
        .block(panel_block(period_title, theme))
        .highlight_style(period_highlight_style)
        .highlight_symbol(period_highlight_symbol);

    match app.rollup_view {
        RollupView::Weekly => {
            frame.render_stateful_widget(period_list, body[0], &mut app.rollup_week_state)
        }
        RollupView::Monthly => {
            frame.render_stateful_widget(period_list, body[0], &mut app.rollup_month_state)
        }
        RollupView::Yearly => {
            frame.render_stateful_widget(period_list, body[0], &mut app.rollup_year_state)
        }
    };

    let right_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(0)])
        .split(body[1]);

    let daily = app.rollup_daily_for_selected_period();
    let selected_day = app
        .rollup_day_state
        .selected()
        .and_then(|index| daily.get(index).copied());

    let summary_lines = if let Some(period) = app.rollup_selected_period() {
        let total_hours = hours_from_seconds(period.seconds);
        let (target_hours, target_days) =
            period_target_hours(period, app.target_hours, app.rollups_include_weekends);
        let delta = normalize_delta(total_hours - target_hours);
        let overtime = period_overtime_left_hours(
            period,
            &app.rollups.daily,
            app.target_hours,
            app.rollups_include_weekends,
            app.date_range.end_date(),
        );
        let avg = if target_days > 0 {
            total_hours / target_days as f64
        } else {
            0.0
        };
        let mut lines = vec![
            Line::from(Span::styled(
                period.label.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(format!("Total: {:.2}h", total_hours)),
            Line::from(format!(
                "Target: {:.2}h ({} days × {:.2})",
                target_hours, target_days, app.target_hours
            )),
            Line::from(vec![
                Span::raw("Delta: "),
                Span::styled(format!("{:+.2}h", delta), delta_style(delta, theme)),
            ]),
            Line::from(vec![
                Span::raw("Overtime: "),
                Span::styled(format!("{:.2}h", overtime), delta_style(overtime, theme)),
            ]),
            Line::from(format!("Avg/day: {:.2}h", avg)),
        ];

        if let Some(day) = selected_day {
            let hours = hours_from_seconds(day.seconds);
            let day_target =
                target_hours_for_day(day.date, app.target_hours, app.rollups_include_weekends);
            let day_delta = normalize_delta(hours - day_target);
            let label = day.date.format("%a %Y-%m-%d").to_string();
            lines.push(Line::from(vec![
                Span::raw(format!("Selected: {label} ")),
                Span::styled(format!("{:.2}h", hours), theme.muted_style()),
                Span::raw(" "),
                Span::styled(format!("{:+.2}h", day_delta), delta_style(day_delta, theme)),
            ]));
        }

        lines
    } else {
        vec![Line::from("No rollup data.")]
    };

    let summary = Paragraph::new(summary_lines)
        .alignment(Alignment::Left)
        .block(panel_block("Summary", theme))
        .wrap(Wrap { trim: true });
    frame.render_widget(summary, right_sections[0]);

    if let Some(period) = app.rollup_selected_period() {
        let calendar = build_calendar_lines(
            &daily,
            period,
            selected_day.map(|day| day.date),
            app.rollup_focus,
            app.rollup_view,
            app.target_hours,
            app.rollups_include_weekends,
            app.rollups_week_start,
            theme,
        );
        let viewport_height = right_sections[1].height.saturating_sub(2) as usize;
        let scroll_y = calendar_scroll_offset(
            calendar.selected_line,
            calendar.lines.len(),
            viewport_height,
        );
        let calendar = Paragraph::new(calendar.lines)
            .alignment(Alignment::Left)
            .block(panel_block("Calendar", theme))
            .scroll((scroll_y, 0))
            .wrap(Wrap { trim: false });
        frame.render_widget(calendar, right_sections[1]);
    } else {
        let empty = Paragraph::new(vec![Line::from("No days")])
            .alignment(Alignment::Left)
            .block(panel_block("Calendar", theme))
            .wrap(Wrap { trim: false });
        frame.render_widget(empty, right_sections[1]);
    }

    let footer = rollups_footer_line(app, theme);
    let footer_block = Paragraph::new(footer).alignment(Alignment::Left).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(theme.border_style())
            .style(theme.panel_style()),
    );
    frame.render_widget(footer_block, chunks[2]);
}

fn rollups_header_line(app: &App, theme: &Theme) -> Line<'static> {
    let workspace = app
        .selected_workspace
        .as_ref()
        .map(|w| w.name.clone())
        .unwrap_or_else(|| "No workspace".to_string());
    let view_label = match app.rollup_view {
        RollupView::Weekly => "Weekly",
        RollupView::Monthly => "Monthly",
        RollupView::Yearly => "Yearly",
    };
    let weekends = if app.rollups_include_weekends {
        "On"
    } else {
        "Off"
    };
    let week_start = match app.rollups_week_start {
        WeekStart::Monday => "Mon",
        WeekStart::Sunday => "Sun",
    };
    Line::from(vec![
        Span::styled("Rollups", theme.title_style()),
        Span::raw("  "),
        Span::styled("Workspace", theme.muted_style()),
        Span::raw(": "),
        Span::styled(workspace, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("View", theme.muted_style()),
        Span::raw(": "),
        Span::styled(view_label, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Weekends", theme.muted_style()),
        Span::raw(": "),
        Span::styled(weekends, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Week start", theme.muted_style()),
        Span::raw(": "),
        Span::styled(week_start, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Date", theme.muted_style()),
        Span::raw(": "),
        Span::raw(app.date_range.label().to_string()),
    ])
}

fn rollups_footer_line(app: &mut App, theme: &Theme) -> Line<'static> {
    let status = app.visible_status().unwrap_or_default();
    Line::from(vec![
        Span::styled("Tab focus", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("←/→ move", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("↑/↓ move", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("w weekly", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("m monthly", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("y yearly", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("z weekends", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("R refetch scope", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("h help", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("Esc back", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("q quit", theme.muted_style()),
        if status.is_empty() {
            Span::raw("")
        } else {
            Span::raw(format!("   |   {}", status))
        },
    ])
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
        Span::styled(
            format!("Timeshit v{}", update::current_version()),
            theme.title_style(),
        ),
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
        Style::default()
            .fg(theme.error)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.success)
            .add_modifier(Modifier::BOLD)
    };

    let status = app.visible_status().unwrap_or_default();
    Line::from(vec![
        Span::styled(format!("Total {:.2}h", app.total_hours), total_style),
        Span::raw("   "),
        Span::styled("h help", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("o rollups", theme.muted_style()),
        Span::raw(" · "),
        Span::styled("[/]", theme.muted_style()),
        Span::raw(" "),
        Span::styled("period", theme.muted_style()),
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
        lines.push(Line::from(Span::styled(
            status,
            Style::default().fg(Color::Red),
        )));
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
        Span::styled(
            app.date_start_input_value(),
            Style::default().fg(theme.accent),
        )
    } else {
        Span::raw(app.date_start_input_value())
    };
    let end_value = if app.is_date_start_active() {
        Span::raw(app.date_end_input_value())
    } else {
        Span::styled(
            app.date_end_input_value(),
            Style::default().fg(theme.accent),
        )
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
        lines.push(Line::from(Span::styled(
            status,
            Style::default().fg(Color::Red),
        )));
    }

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(panel_block("Date Filter", theme))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, block);
}

fn draw_refetch_confirm(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let block = centered_rect(72, 42, area);
    frame.render_widget(Clear, block);

    let lines = if let Some(plan) = app.refetch_plan_view() {
        let warning = if plan.estimated_calls > plan.remaining_calls {
            format!(
                "Warning: needs ~{} call(s), only {} local calls remain.",
                plan.estimated_calls, plan.remaining_calls
            )
        } else {
            format!(
                "This may use up to {} API call(s). Remaining local budget: {}.",
                plan.estimated_calls, plan.remaining_calls
            )
        };

        vec![
            Line::from(Span::styled(
                "Refetch from Toggl API",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(format!("Scope: {}", plan.scope_label)),
            Line::from(format!("Range: {} → {}", plan.start, plan.end)),
            Line::from(format!("Days: {}", plan.days)),
            Line::from(""),
            Line::from(Span::styled(warning, Style::default().fg(theme.error))),
            Line::from(Span::styled(
                "Free Toggl accounts usually have a hard daily limit (about 30 calls).",
                Style::default().fg(theme.error),
            )),
            Line::from(Span::styled(
                "If quota is exhausted (402/429), refetch stops and only fetched days are cached.",
                Style::default().fg(theme.error),
            )),
            Line::from(""),
            Line::from("Press Enter or y to continue • n or Esc to cancel"),
        ]
    } else {
        vec![
            Line::from("No refetch scope selected."),
            Line::from("Press Esc to close."),
        ]
    };

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(panel_block("Confirm Refetch", theme))
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
        Style::default()
            .fg(theme.error)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.success)
            .add_modifier(Modifier::BOLD)
    };
    let paragraph = Paragraph::new(Line::from(Span::styled(message, style)))
        .alignment(Alignment::Center)
        .block(panel_block("Copied", theme));
    frame.render_widget(paragraph, rect);
}

fn hours_from_seconds(seconds: i64) -> f64 {
    seconds as f64 / 3600.0
}

fn normalize_delta(value: f64) -> f64 {
    if value.abs() < 0.005 { 0.0 } else { value }
}

fn delta_style(value: f64, theme: &Theme) -> Style {
    if value > 0.0 {
        Style::default().fg(theme.success)
    } else if value < 0.0 {
        Style::default().fg(theme.error)
    } else {
        Style::default().fg(theme.muted)
    }
}

fn period_target_hours(
    period: &PeriodRollup,
    target_hours: f64,
    include_weekends: bool,
    non_working_days: &HashSet<NaiveDate>,
) -> (f64, usize) {
    let mut days = 0usize;
    let mut total = 0.0;
    let mut current = period.start;
    while current <= period.end {
        let target =
            target_hours_for_day(current, target_hours, include_weekends, non_working_days);
        if target > 0.0 {
            days += 1;
        }
        total += target;
        current = current.succ_opt().unwrap_or(current + Duration::days(1));
    }
    (total, days)
}

fn period_overtime_left_hours(
    period: &PeriodRollup,
    daily: &[DailyTotal],
    target_hours: f64,
    include_weekends: bool,
    non_working_days: &HashSet<NaiveDate>,
    active_end: NaiveDate,
) -> f64 {
    let cutoff = period.end.min(active_end);
    if cutoff < period.start {
        return 0.0;
    }

    let (worked_seconds, target_total) = daily
        .iter()
        .filter(|day| day.date >= period.start && day.date <= cutoff)
        .fold((0i64, 0.0f64), |(worked, target), day| {
            (
                worked + day.seconds,
                target
                    + target_hours_for_day(
                        day.date,
                        target_hours,
                        include_weekends,
                        non_working_days,
                    ),
            )
        });

    let overtime = normalize_delta(hours_from_seconds(worked_seconds) - target_total);
    if overtime > 0.0 { overtime } else { 0.0 }
}

fn period_worked_totals_until(
    period: &PeriodRollup,
    daily: &[DailyTotal],
    active_end: NaiveDate,
) -> (i64, usize) {
    let cutoff = period.end.min(active_end);
    if cutoff < period.start {
        return (0, 0);
    }

    daily
        .iter()
        .filter(|day| day.date >= period.start && day.date <= cutoff)
        .fold((0i64, 0usize), |(worked_seconds, worked_days), day| {
            if day.seconds > 0 {
                (worked_seconds + day.seconds, worked_days + 1)
            } else {
                (worked_seconds, worked_days)
            }
        })
}

fn target_hours_for_day(
    day: NaiveDate,
    target_hours: f64,
    include_weekends: bool,
    non_working_days: &HashSet<NaiveDate>,
) -> f64 {
    if non_working_days.contains(&day) {
        return 0.0;
    }
    if include_weekends || day.weekday().number_from_monday() <= 5 {
        target_hours
    } else {
        0.0
    }
}

struct CalendarRender {
    lines: Vec<Line<'static>>,
    selected_line: Option<usize>,
}

fn build_calendar_lines(
    daily: &[&DailyTotal],
    period: &PeriodRollup,
    selected_date: Option<NaiveDate>,
    focus: RollupFocus,
    rollup_view: RollupView,
    target_hours: f64,
    include_weekends: bool,
    week_start: WeekStart,
    theme: &Theme,
) -> CalendarRender {
    if matches!(rollup_view, RollupView::Yearly) {
        build_yearly_calendar_lines(
            daily,
            period,
            selected_date,
            focus,
            target_hours,
            include_weekends,
            week_start,
            theme,
        )
    } else {
        build_period_calendar_grid_lines(
            daily,
            period,
            selected_date,
            focus,
            target_hours,
            include_weekends,
            week_start,
            theme,
        )
    }
}

fn build_period_calendar_grid_lines(
    daily: &[&DailyTotal],
    period: &PeriodRollup,
    selected_date: Option<NaiveDate>,
    focus: RollupFocus,
    target_hours: f64,
    include_weekends: bool,
    week_start: WeekStart,
    theme: &Theme,
) -> CalendarRender {
    let mut map: HashMap<NaiveDate, i64> = HashMap::new();
    for day in daily {
        map.insert(day.date, day.seconds);
    }

    let header_labels: Vec<&str> = if include_weekends {
        match week_start {
            WeekStart::Monday => vec!["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
            WeekStart::Sunday => vec!["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
        }
    } else {
        vec!["Mon", "Tue", "Wed", "Thu", "Fri"]
    };
    let column_count = header_labels.len();
    let cell_width = 10usize;
    let mut week_cells: Vec<Option<NaiveDate>> = Vec::new();
    let mut week_rows: Vec<Vec<Option<NaiveDate>>> = Vec::new();
    let mut current = period.start;
    let offset = if include_weekends {
        match week_start {
            WeekStart::Monday => current.weekday().num_days_from_monday() as usize,
            WeekStart::Sunday => current.weekday().num_days_from_sunday() as usize,
        }
    } else {
        match current.weekday().number_from_monday() {
            6 | 7 => 0,
            weekday => (weekday - 1) as usize,
        }
    };
    for _ in 0..offset {
        week_cells.push(None);
    }

    while current <= period.end {
        if include_weekends || current.weekday().number_from_monday() <= 5 {
            week_cells.push(Some(current));
            if week_cells.len() == column_count {
                if week_cells.iter().any(Option::is_some) {
                    week_rows.push(week_cells.clone());
                }
                week_cells.clear();
            }
        }
        current = current.succ_opt().unwrap_or(current + Duration::days(1));
    }

    if !week_cells.is_empty() && week_cells.iter().any(Option::is_some) {
        while week_cells.len() < column_count {
            week_cells.push(None);
        }
        week_rows.push(week_cells);
    }

    if week_rows.is_empty() {
        return CalendarRender {
            lines: vec![Line::from("No days")],
            selected_line: None,
        };
    }

    let border_style = theme.border_style();
    let today = Local::now().date_naive();
    let horizontal = "─".repeat(cell_width);
    let mut lines = Vec::new();
    let mut selected_line = None;

    let build_border =
        |left: &'static str, join: &'static str, right: &'static str| -> Line<'static> {
            let mut spans = Vec::new();
            spans.push(Span::styled(left, border_style));
            for column in 0..column_count {
                spans.push(Span::styled(horizontal.clone(), border_style));
                if column + 1 < column_count {
                    spans.push(Span::styled(join, border_style));
                }
            }
            spans.push(Span::styled(right, border_style));
            Line::from(spans)
        };

    let build_text_row = |values: Vec<(String, Style)>| -> Line<'static> {
        let mut spans = Vec::new();
        spans.push(Span::styled("│", border_style));
        for (index, (value, style)) in values.into_iter().enumerate() {
            spans.push(Span::styled(value, style));
            if index + 1 < column_count {
                spans.push(Span::styled("│", border_style));
            }
        }
        spans.push(Span::styled("│", border_style));
        Line::from(spans)
    };

    lines.push(build_border("┌", "┬", "┐"));
    let header_values = header_labels
        .iter()
        .map(|label| {
            (
                format!("{:^width$}", label, width = cell_width),
                theme.muted_style().add_modifier(Modifier::BOLD),
            )
        })
        .collect::<Vec<_>>();
    lines.push(build_text_row(header_values));
    lines.push(build_border("├", "┼", "┤"));

    for (week_index, week) in week_rows.iter().enumerate() {
        let mut day_values = Vec::new();
        let mut hour_values = Vec::new();
        let has_selected = selected_date
            .map(|date| week.iter().any(|cell| *cell == Some(date)))
            .unwrap_or(false);
        for cell in week {
            match cell {
                Some(date) => {
                    let seconds = *map.get(date).unwrap_or(&0);
                    let hours = hours_from_seconds(seconds);
                    let delta = normalize_delta(
                        hours - target_hours_for_day(*date, target_hours, include_weekends),
                    );
                    let mut style = delta_style(delta, theme).add_modifier(Modifier::BOLD);
                    if *date == today {
                        style = style.fg(Color::Yellow).add_modifier(Modifier::BOLD);
                    }
                    if Some(*date) == selected_date {
                        if matches!(focus, RollupFocus::Days) {
                            style = style.bg(theme.accent);
                            if *date == today {
                                style = style.fg(Color::Yellow).add_modifier(Modifier::BOLD);
                            } else {
                                style = style.fg(theme.text).add_modifier(Modifier::BOLD);
                            }
                        }
                    }
                    if *date == today {
                        style = style.add_modifier(Modifier::UNDERLINED);
                    }
                    day_values.push((
                        format!(
                            "{:^width$}",
                            format!("{:02}", date.day()),
                            width = cell_width
                        ),
                        style,
                    ));
                    hour_values.push((
                        format!("{:^width$}", format!("{:.2}h", hours), width = cell_width),
                        style,
                    ));
                }
                None => {
                    day_values.push((
                        format!("{:width$}", "", width = cell_width),
                        theme.muted_style(),
                    ));
                    hour_values.push((
                        format!("{:width$}", "", width = cell_width),
                        theme.muted_style(),
                    ));
                }
            }
        }

        if has_selected && selected_line.is_none() {
            selected_line = Some(lines.len());
        }
        lines.push(build_text_row(day_values));
        lines.push(build_text_row(hour_values));
        if week_index + 1 < week_rows.len() {
            lines.push(build_border("├", "┼", "┤"));
        } else {
            lines.push(build_border("└", "┴", "┘"));
        }
    }
    CalendarRender {
        lines,
        selected_line,
    }
}

fn build_yearly_calendar_lines(
    daily: &[&DailyTotal],
    period: &PeriodRollup,
    selected_date: Option<NaiveDate>,
    focus: RollupFocus,
    target_hours: f64,
    include_weekends: bool,
    week_start: WeekStart,
    theme: &Theme,
) -> CalendarRender {
    let mut all_lines = Vec::new();
    let mut selected_line = None;

    let mut current = NaiveDate::from_ymd_opt(period.start.year(), 1, 1).unwrap_or(period.start);
    let year_end = NaiveDate::from_ymd_opt(period.end.year(), 12, 31).unwrap_or(period.end);

    while current <= year_end {
        let month_start =
            NaiveDate::from_ymd_opt(current.year(), current.month(), 1).unwrap_or(current);
        let month_end = month_end(month_start);
        let clamped_start = if month_start < period.start {
            period.start
        } else {
            month_start
        };
        let clamped_end = if month_end > period.end {
            period.end
        } else {
            month_end
        };

        if clamped_start <= clamped_end {
            all_lines.push(Line::from(Span::styled(
                month_start.format("%B %Y").to_string(),
                theme.title_style(),
            )));
            let month_period = PeriodRollup {
                label: month_start.format("%B %Y").to_string(),
                start: clamped_start,
                end: clamped_end,
                days: 0,
                seconds: 0,
            };
            let month_render = build_period_calendar_grid_lines(
                daily,
                &month_period,
                selected_date,
                focus,
                target_hours,
                include_weekends,
                week_start,
                theme,
            );
            let start_line = all_lines.len();
            if selected_line.is_none() {
                selected_line = month_render.selected_line.map(|line| start_line + line);
            }
            all_lines.extend(month_render.lines);
            all_lines.push(Line::from(""));
        }

        let (next_year, next_month) = if current.month() == 12 {
            (current.year() + 1, 1)
        } else {
            (current.year(), current.month() + 1)
        };
        current = NaiveDate::from_ymd_opt(next_year, next_month, 1)
            .unwrap_or(year_end + Duration::days(1));
    }

    while matches!(all_lines.last(), Some(line) if line.spans.is_empty()) {
        all_lines.pop();
    }

    CalendarRender {
        lines: if all_lines.is_empty() {
            vec![Line::from("No days")]
        } else {
            all_lines
        },
        selected_line,
    }
}

fn calendar_scroll_offset(
    selected_line: Option<usize>,
    total_lines: usize,
    viewport_height: usize,
) -> u16 {
    if viewport_height == 0 || total_lines <= viewport_height {
        return 0;
    }
    let max_offset = total_lines.saturating_sub(viewport_height);
    let target_line = selected_line.unwrap_or(0);
    let offset = target_line
        .saturating_sub(viewport_height / 2)
        .min(max_offset);
    offset as u16
}

fn month_end(date: NaiveDate) -> NaiveDate {
    let (year, month) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    NaiveDate::from_ymd_opt(year, month, 1)
        .and_then(|next| next.pred_opt())
        .unwrap_or(date)
}

fn draw_help(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let block = centered_rect(70, 70, area);
    frame.render_widget(Clear, block);

    let header_style = Style::default()
        .add_modifier(Modifier::BOLD)
        .fg(theme.accent);
    let key_style = Style::default().fg(theme.highlight);

    let mut rows = vec![
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
            Cell::from(Span::styled("[ / ]", key_style)),
            Cell::from("Previous / next active date range"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Tab", key_style)),
            Cell::from("Switch range field"),
        ]),
        Row::new(vec![Cell::from(""), Cell::from("")]),
        Row::new(vec![
            Cell::from(Span::styled("Rollups", header_style)),
            Cell::from(""),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("o", key_style)),
            Cell::from("Open rollups view"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("w / m / y", key_style)),
            Cell::from("Weekly / monthly / yearly view"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("z", key_style)),
            Cell::from("Toggle weekends in rollups"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Shift+R", key_style)),
            Cell::from("Refetch selected day/week/month/year"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Left/Right", key_style)),
            Cell::from("Move period/day (1 step)"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Up/Down", key_style)),
            Cell::from("Move period; in month/year day view jump week"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Tab", key_style)),
            Cell::from("Switch focus"),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("Esc", key_style)),
            Cell::from("Back to dashboard"),
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

    if app.update_installable {
        rows.insert(
            rows.len() - 3,
            Row::new(vec![
                Cell::from(Span::styled("u", key_style)),
                Cell::from("Install update (when available)"),
            ]),
        );
    }

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
    let inactive_highlight = Style::default()
        .fg(theme.highlight)
        .add_modifier(Modifier::BOLD);

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
                SettingsItem::RollupsIncludeWeekends => {
                    let enabled = app.settings_rollups_include_weekends_display();
                    (
                        "Include weekends",
                        if enabled {
                            "On".to_string()
                        } else {
                            "Off".to_string()
                        },
                        false,
                    )
                }
                SettingsItem::RollupsWeekStart => {
                    let value = match app.settings_rollups_week_start_display() {
                        WeekStart::Monday => "Monday",
                        WeekStart::Sunday => "Sunday",
                    };
                    ("Week start", value.to_string(), false)
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
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else if disabled {
                theme.muted_style()
            } else {
                theme.muted_style()
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{label}: "),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
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
            Some(SettingsItem::TargetHours) | Some(SettingsItem::TogglToken) => {
                "Enter save • Esc cancel"
            }
            Some(SettingsItem::Theme)
            | Some(SettingsItem::RollupsIncludeWeekends)
            | Some(SettingsItem::RollupsWeekStart) => "Up/Down change • Enter save • Esc cancel",
            Some(SettingsItem::TimeRoundingToggle)
            | Some(SettingsItem::RoundingIncrement)
            | Some(SettingsItem::RoundingMode) => "Up/Down change • Enter save • Esc cancel",
            None => "Esc cancel",
        },
    };

    let mut hint_lines = vec![Line::from(hint_text)];
    if let Some(status) = app.visible_status() {
        let is_success = is_success_status(&status);
        let color = if is_success {
            theme.success
        } else {
            theme.error
        };
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
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
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
