use chrono::{DateTime, Local, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::time::{Duration, Instant};

use crate::dates::{parse_date, DateRange};
use crate::grouping::{group_entries, GroupedProject};
use crate::models::{Project, TimeEntry, Workspace};
use crate::storage;
use crate::toggl::{TogglClient, TogglError};
use arboard::Clipboard;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Loading,
    Dashboard,
    Login,
    WorkspaceSelect,
    DateInput(DateInputMode),
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateInputMode {
    Single,
    Start,
    End,
}

pub struct App {
    pub should_quit: bool,
    pub needs_refresh: bool,
    pub mode: Mode,
    pub status: Option<String>,
    pub input: String,
    pub token: Option<String>,
    pub workspace_list: Vec<Workspace>,
    pub workspace_state: ListState,
    pub selected_workspace: Option<Workspace>,
    pub date_range: DateRange,
    pub projects: Vec<Project>,
    pub time_entries: Vec<TimeEntry>,
    pub grouped: Vec<GroupedProject>,
    pub total_hours: f64,
    pub project_state: ListState,
    pub last_refresh: Option<DateTime<Local>>,
    pub show_help: bool,
    toast: Option<Toast>,
}

impl App {
    pub fn new(date_range: DateRange, force_login: bool) -> Self {
        let token = if force_login {
            None
        } else {
            storage::read_token()
        };
        let mode = if token.is_some() {
            Mode::Loading
        } else {
            Mode::Login
        };
        let mut project_state = ListState::default();
        project_state.select(Some(0));
        let mut workspace_state = ListState::default();
        workspace_state.select(Some(0));

        App {
            should_quit: false,
            needs_refresh: token.is_some(),
            mode,
            status: None,
            input: String::new(),
            token,
            workspace_list: Vec::new(),
            workspace_state,
            selected_workspace: None,
            date_range,
            projects: Vec::new(),
            time_entries: Vec::new(),
            grouped: Vec::new(),
            total_hours: 0.0,
            project_state,
            last_refresh: None,
            show_help: false,
            toast: None,
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Login => self.handle_login_input(key),
            Mode::WorkspaceSelect => self.handle_workspace_input(key),
            Mode::DateInput(mode) => self.handle_date_input(mode, key),
            Mode::Dashboard | Mode::Loading | Mode::Error => self.handle_dashboard_input(key),
        }
    }

    pub fn refresh_data(&mut self) {
        self.needs_refresh = false;
        self.status = None;

        let token = match self.token.clone() {
            Some(token) => token,
            None => {
                self.mode = Mode::Login;
                return;
            }
        };

        let client = TogglClient::new(token);

        let workspaces = match client.fetch_workspaces() {
            Ok(workspaces) => workspaces,
            Err(err) => {
                self.handle_error(err);
                return;
            }
        };

        if workspaces.is_empty() {
            self.mode = Mode::Error;
            self.status = Some("No workspaces found.".to_string());
            return;
        }

        self.workspace_list = workspaces;
        if self.workspace_state.selected().is_none() {
            self.workspace_state.select(Some(0));
        } else if let Some(selected) = self.workspace_state.selected() {
            if selected >= self.workspace_list.len() {
                self.workspace_state.select(Some(0));
            }
        }

        if self.selected_workspace.is_none() {
            if self.workspace_list.len() == 1 {
                self.selected_workspace = Some(self.workspace_list[0].clone());
            } else {
                self.mode = Mode::WorkspaceSelect;
                return;
            }
        }

        let workspace = match &self.selected_workspace {
            Some(workspace) => workspace.clone(),
            None => {
                self.mode = Mode::WorkspaceSelect;
                return;
            }
        };

        let projects = match client.fetch_projects(workspace.id) {
            Ok(projects) => projects,
            Err(err) => {
                self.handle_error(err);
                return;
            }
        };

        let (start, end) = self.date_range.as_rfc3339();
        let time_entries = match client.fetch_time_entries(&start, &end) {
            Ok(entries) => entries,
            Err(err) => {
                self.handle_error(err);
                return;
            }
        };

        let valid_entries: Vec<TimeEntry> = time_entries
            .into_iter()
            .filter(|entry| entry.stop.is_some())
            .collect();

        let grouped = group_entries(&valid_entries, &projects);
        let total_hours = grouped.iter().map(|group| group.total_hours).sum();

        if self.project_state.selected().is_none() {
            self.project_state.select(Some(0));
        } else if let Some(selected) = self.project_state.selected() {
            if selected >= grouped.len() && !grouped.is_empty() {
                self.project_state.select(Some(0));
            }
        }

        self.projects = projects;
        self.time_entries = valid_entries;
        self.grouped = grouped;
        self.total_hours = total_hours;
        self.last_refresh = Some(Local::now());
        self.mode = Mode::Dashboard;
    }

    fn handle_error(&mut self, err: TogglError) {
        match err {
            TogglError::Unauthorized => {
                self.token = None;
                self.mode = Mode::Login;
                self.status = Some("Invalid token. Please login.".to_string());
            }
            TogglError::Network(message) => {
                self.mode = Mode::Error;
                self.status = Some(message);
            }
        }
    }

    fn handle_dashboard_input(&mut self, key: KeyEvent) {
        if self.show_help {
            match key.code {
                KeyCode::Char('h') | KeyCode::Esc => {
                    self.show_help = false;
                }
                KeyCode::Char('q') => self.should_quit = true,
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('r') => self.trigger_refresh(),
            KeyCode::Char('t') => {
                self.date_range = DateRange::today();
                self.trigger_refresh();
            }
            KeyCode::Char('h') => self.show_help = true,
            KeyCode::Char('d') => self.enter_date_input(DateInputMode::Single),
            KeyCode::Char('s') => self.enter_date_input(DateInputMode::Start),
            KeyCode::Char('e') => self.enter_date_input(DateInputMode::End),
            KeyCode::Enter => {
                let include_project = key.modifiers.contains(KeyModifiers::SHIFT);
                self.copy_entries_to_clipboard(include_project);
            }
            KeyCode::Char('p') | KeyCode::Char('P') => self.copy_entries_to_clipboard(true),
            KeyCode::Up => self.select_previous_project(),
            KeyCode::Down => self.select_next_project(),
            _ => {}
        }
    }

    fn handle_workspace_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Up => self.select_previous_workspace(),
            KeyCode::Down => self.select_next_workspace(),
            KeyCode::Enter => {
                if let Some(index) = self.workspace_state.selected() {
                    if let Some(workspace) = self.workspace_list.get(index) {
                        self.selected_workspace = Some(workspace.clone());
                        self.mode = Mode::Loading;
                        self.needs_refresh = true;
                    }
                }
            }
            KeyCode::Esc => {
                if self.selected_workspace.is_some() {
                    self.mode = Mode::Dashboard;
                } else {
                    self.should_quit = true;
                }
            }
            _ => {}
        }
    }

    fn handle_login_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Enter => {
                if !self.input.trim().is_empty() {
                    if let Err(err) = storage::write_token(self.input.trim()) {
                        self.status = Some(format!("Failed to save token: {err}"));
                        return;
                    }
                    self.token = Some(self.input.trim().to_string());
                    self.input.clear();
                    self.mode = Mode::Loading;
                    self.needs_refresh = true;
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(ch) => {
                if !ch.is_control() {
                    self.input.push(ch);
                }
            }
            KeyCode::Esc => self.should_quit = true,
            _ => {}
        }
    }

    fn handle_date_input(&mut self, mode: DateInputMode, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Enter => {
                let date_value = match parse_date(self.input.trim()) {
                    Ok(date) => date,
                    Err(err) => {
                        self.status = Some(err);
                        return;
                    }
                };

                let updated = match mode {
                    DateInputMode::Single => Ok(DateRange::from_single(date_value)),
                    DateInputMode::Start => self.update_start_date(date_value),
                    DateInputMode::End => self.update_end_date(date_value),
                };

                match updated {
                    Ok(range) => {
                        self.date_range = range;
                        self.input.clear();
                        self.mode = Mode::Loading;
                        self.needs_refresh = true;
                        self.status = None;
                    }
                    Err(err) => {
                        self.status = Some(err);
                    }
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(ch) => {
                if !ch.is_control() {
                    self.input.push(ch);
                }
            }
            KeyCode::Esc => {
                self.input.clear();
                self.mode = Mode::Dashboard;
            }
            _ => {}
        }
    }

    fn update_start_date(&self, date: NaiveDate) -> Result<DateRange, String> {
        let current_end = self.date_range.end_date();
        if date > current_end {
            return Err("Start date cannot be after end date.".to_string());
        }
        Ok(DateRange::from_bounds(date, current_end))
    }

    fn update_end_date(&self, date: NaiveDate) -> Result<DateRange, String> {
        let current_start = self.date_range.start_date();
        if date < current_start {
            return Err("End date cannot be before start date.".to_string());
        }
        Ok(DateRange::from_bounds(current_start, date))
    }

    fn enter_date_input(&mut self, mode: DateInputMode) {
        self.input.clear();
        self.mode = Mode::DateInput(mode);
        self.status = None;
    }

    fn trigger_refresh(&mut self) {
        self.mode = Mode::Loading;
        self.needs_refresh = true;
    }

    fn select_previous_project(&mut self) {
        if self.grouped.is_empty() {
            return;
        }
        let selected = self.project_state.selected().unwrap_or(0);
        let new_index = if selected == 0 {
            self.grouped.len() - 1
        } else {
            selected - 1
        };
        self.project_state.select(Some(new_index));
    }

    fn select_next_project(&mut self) {
        if self.grouped.is_empty() {
            return;
        }
        let selected = self.project_state.selected().unwrap_or(0);
        let new_index = if selected + 1 >= self.grouped.len() {
            0
        } else {
            selected + 1
        };
        self.project_state.select(Some(new_index));
    }

    fn select_previous_workspace(&mut self) {
        if self.workspace_list.is_empty() {
            return;
        }
        let selected = self.workspace_state.selected().unwrap_or(0);
        let new_index = if selected == 0 {
            self.workspace_list.len() - 1
        } else {
            selected - 1
        };
        self.workspace_state.select(Some(new_index));
    }

    fn select_next_workspace(&mut self) {
        if self.workspace_list.is_empty() {
            return;
        }
        let selected = self.workspace_state.selected().unwrap_or(0);
        let new_index = if selected + 1 >= self.workspace_list.len() {
            0
        } else {
            selected + 1
        };
        self.workspace_state.select(Some(new_index));
    }

    pub fn current_project(&self) -> Option<&GroupedProject> {
        self.project_state
            .selected()
            .and_then(|index| self.grouped.get(index))
    }

    fn copy_entries_to_clipboard(&mut self, include_project: bool) {
        if self.grouped.is_empty() {
            self.status = Some("No entries to copy.".to_string());
            self.set_toast("No entries to copy.", true);
            return;
        }

        let text = self.format_entries_for_clipboard(include_project);
        match Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(text))
        {
            Ok(_) => {
                let message = if include_project {
                    "Copied entries with project names."
                } else {
                    "Copied entries to clipboard."
                };
                self.status = Some(message.to_string());
                self.set_toast(message, false);
            }
            Err(err) => {
                let message = format!("Clipboard error: {err}");
                self.status = Some(message.clone());
                self.set_toast(message, true);
            }
        }
    }

    fn format_entries_for_clipboard(&self, include_project: bool) -> String {
        let mut lines: Vec<String> = Vec::new();
        for project in &self.grouped {
            for entry in &project.entries {
                if include_project {
                    lines.push(format!(
                        "• {} — {} ({:.2}h)",
                        project.name, entry.description, entry.total_hours
                    ));
                } else {
                    lines.push(format!("• {} ({:.2}h)", entry.description, entry.total_hours));
                }
            }
        }

        while matches!(lines.last(), Some(last) if last.is_empty()) {
            lines.pop();
        }

        lines.join("\n")
    }

    pub fn active_toast(&mut self) -> Option<ToastView> {
        let toast = self.toast.as_ref()?;
        if toast.created_at.elapsed() > Duration::from_secs(2) {
            self.toast = None;
            return None;
        }
        Some(ToastView {
            message: toast.message.clone(),
            is_error: toast.is_error,
        })
    }

    fn set_toast(&mut self, message: impl Into<String>, is_error: bool) {
        self.toast = Some(Toast {
            message: message.into(),
            created_at: Instant::now(),
            is_error,
        });
    }
}

struct Toast {
    message: String,
    created_at: Instant,
    is_error: bool,
}

pub struct ToastView {
    pub message: String,
    pub is_error: bool,
}
