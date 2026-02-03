use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::ListState;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::dates::{parse_date, DateRange};
use crate::grouping::{group_entries, GroupedProject};
use crate::models::{Client as TogglClientModel, Project, TimeEntry, Workspace};
use crate::storage::{
    self, CacheFile, CachedData, QuotaFile, ThemePreference,
};
use crate::toggl::{TogglClient, TogglError};
use arboard::Clipboard;

const CALL_LIMIT: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Loading,
    Dashboard,
    Login,
    WorkspaceSelect,
    DateInput(DateInputMode),
    Settings,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateInputMode {
    Range,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateField {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefreshIntent {
    CacheOnly,
    ForceApi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheReason {
    CacheOnly,
    Quota,
    ApiError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsMode {
    SelectCategory,
    EditValue,
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
    pub theme: ThemePreference,
    pub target_hours: f64,
    token_hash: Option<String>,
    cache: Option<CacheFile>,
    quota: QuotaFile,
    refresh_intent: RefreshIntent,
    date_start_input: String,
    date_end_input: String,
    date_active_field: DateField,
    settings_input: String,
    settings_categories: Vec<String>,
    settings_state: ListState,
    settings_mode: SettingsMode,
    status_created_at: Option<Instant>,
    last_status_snapshot: Option<String>,
    toast: Option<Toast>,
}

impl App {
    pub fn new(date_range: DateRange, force_login: bool) -> Self {
        let token = if force_login {
            None
        } else {
            storage::read_token()
        };
        let mode = if token.is_some() { Mode::Loading } else { Mode::Login };
        let theme = storage::read_theme().unwrap_or(ThemePreference::Dark);
        let target_hours = storage::read_target_hours().unwrap_or(8.0);
        let token_hash = token.as_ref().map(|value| storage::hash_token(value));
        let cache = token_hash
            .as_ref()
            .and_then(|hash| storage::read_cache().filter(|cache| cache.token_hash == *hash));
        let quota = storage::read_quota();
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
            theme,
            target_hours,
            token_hash,
            cache,
            quota,
            refresh_intent: RefreshIntent::CacheOnly,
            date_start_input: String::new(),
            date_end_input: String::new(),
            date_active_field: DateField::Start,
            settings_input: String::new(),
            settings_categories: vec!["General".to_string()],
            settings_state: {
                let mut state = ListState::default();
                state.select(Some(0));
                state
            },
            settings_mode: SettingsMode::SelectCategory,
            status_created_at: None,
            last_status_snapshot: None,
            toast: None,
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Login => self.handle_login_input(key),
            Mode::WorkspaceSelect => self.handle_workspace_input(key),
            Mode::DateInput(mode) => self.handle_date_input(mode, key),
            Mode::Settings => self.handle_settings_input(key),
            Mode::Dashboard | Mode::Loading | Mode::Error => self.handle_dashboard_input(key),
        }
    }

    pub fn refresh_data(&mut self) {
        self.needs_refresh = false;
        self.status = None;
        self.ensure_quota_today();

        let token = match self.token.clone() {
            Some(token) => token,
            None => {
                self.mode = Mode::Login;
                return;
            }
        };

        let token_hash = match self.token_hash.clone() {
            Some(hash) => hash,
            None => {
                let hash = storage::hash_token(&token);
                self.token_hash = Some(hash.clone());
                hash
            }
        };

        self.ensure_cache_loaded(&token_hash);
        let manual_refresh = matches!(self.refresh_intent, RefreshIntent::ForceApi);
        let mut allow_api = manual_refresh;
        self.refresh_intent = RefreshIntent::CacheOnly;

        let client = TogglClient::new(token);
        let mut cache_reason: Option<CacheReason> = None;
        let mut cache_timestamp: Option<String> = None;

        if allow_api && self.quota_remaining() < 3 {
            allow_api = false;
            cache_reason = Some(CacheReason::Quota);
        }

        let workspaces = match self.resolve_workspaces(
            &client,
            allow_api,
            &mut cache_reason,
            &mut cache_timestamp,
        ) {
            Some(workspaces) => workspaces,
            None => return,
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
                if manual_refresh {
                    self.refresh_intent = RefreshIntent::ForceApi;
                }
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

        let projects = match self.resolve_projects(
            &client,
            allow_api,
            workspace.id,
            &mut cache_reason,
            &mut cache_timestamp,
        ) {
            Some(projects) => projects,
            None => return,
        };

        let client_names = match self.resolve_client_names(&client, allow_api, workspace.id, &projects) {
            Some(names) => names,
            None => return,
        };

        let (start, end) = self.date_range.as_rfc3339();
        let time_entries = match self.resolve_time_entries(
            &client,
            allow_api,
            workspace.id,
            &start,
            &end,
            &mut cache_reason,
            &mut cache_timestamp,
        ) {
            Some(entries) => entries,
            None => return,
        };

        let valid_entries: Vec<TimeEntry> = time_entries
            .into_iter()
            .filter(|entry| entry.stop.is_some())
            .collect();

        let grouped = group_entries(&valid_entries, &projects, &client_names);
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
        self.last_refresh = if allow_api && cache_reason.is_none() {
            Some(Local::now())
        } else {
            cache_timestamp
                .as_ref()
                .and_then(|value| parse_cached_time(value))
        };

        if let Some(reason) = cache_reason {
            let message = self.cache_status_message(reason, cache_timestamp.as_deref());
            if reason == CacheReason::Quota && manual_refresh {
                self.set_toast(message.clone(), true);
            }
            self.status = Some(message);
        }

        self.mode = Mode::Dashboard;
    }

    fn handle_error(&mut self, err: TogglError) {
        match err {
            TogglError::Unauthorized => {
                self.token = None;
                self.token_hash = None;
                self.cache = None;
                self.mode = Mode::Login;
                self.status = Some("Invalid token. Please login.".to_string());
            }
            TogglError::PaymentRequired => {
                self.mode = Mode::Error;
                self.status = Some("Toggl API error: 402 Payment Required".to_string());
            }
            TogglError::RateLimited => {
                self.mode = Mode::Error;
                self.status = Some("Toggl API rate limit reached.".to_string());
            }
            TogglError::ServerError(message) | TogglError::Network(message) => {
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
                self.set_date_range(DateRange::today());
            }
            KeyCode::Char('y') => self.set_date_range(DateRange::yesterday()),
            KeyCode::Char('h') => self.show_help = true,
            KeyCode::Char('m') | KeyCode::Char('M') => self.toggle_theme(),
            KeyCode::Char('s') => self.enter_settings(),
            KeyCode::Char('d') => self.enter_date_input(DateInputMode::Range),
            KeyCode::Char('c') | KeyCode::Char('C') => self.copy_client_entries_to_clipboard(),
            KeyCode::Char('v') | KeyCode::Char('V') => self.copy_project_entries_to_clipboard(),
            KeyCode::Char('x') | KeyCode::Char('X') => self.copy_entries_to_clipboard(true),
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
                        if self.refresh_intent != RefreshIntent::ForceApi {
                            self.refresh_intent = RefreshIntent::CacheOnly;
                        }
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
                    self.token_hash = Some(storage::hash_token(self.input.trim()));
                    self.cache = self
                        .token_hash
                        .as_ref()
                        .and_then(|hash| storage::read_cache().filter(|cache| cache.token_hash == *hash));
                    self.input.clear();
                    self.mode = Mode::Loading;
                    self.refresh_intent = RefreshIntent::CacheOnly;
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
                let updated = match mode {
                    DateInputMode::Range => self.update_range_from_input(),
                };

                match updated {
                    Ok(range) => {
                        self.date_range = range;
                        self.date_start_input.clear();
                        self.date_end_input.clear();
                        self.mode = Mode::Loading;
                        self.refresh_intent = RefreshIntent::CacheOnly;
                        self.needs_refresh = true;
                        self.status = None;
                    }
                    Err(err) => {
                        self.status = Some(err);
                    }
                }
            }
            KeyCode::Tab => {
                self.date_active_field = match self.date_active_field {
                    DateField::Start => DateField::End,
                    DateField::End => DateField::Start,
                };
            }
            KeyCode::Backspace => {
                self.active_date_input_mut().pop();
            }
            KeyCode::Char(ch) => {
                if !ch.is_control() {
                    self.active_date_input_mut().push(ch);
                }
            }
            KeyCode::Esc => {
                self.date_start_input.clear();
                self.date_end_input.clear();
                self.mode = Mode::Dashboard;
            }
            _ => {}
        }
    }

    fn enter_date_input(&mut self, mode: DateInputMode) {
        let start = self.date_range.start_date().format("%Y-%m-%d").to_string();
        let end = self.date_range.end_date().format("%Y-%m-%d").to_string();
        self.date_start_input = start;
        self.date_end_input = end;
        self.date_active_field = DateField::Start;
        self.mode = Mode::DateInput(mode);
        self.status = None;
    }

    fn enter_settings(&mut self) {
        self.settings_input = format!("{:.2}", self.target_hours);
        self.mode = Mode::Settings;
        self.settings_mode = SettingsMode::SelectCategory;
        self.status = None;
    }

    fn trigger_refresh(&mut self) {
        self.mode = Mode::Loading;
        self.refresh_intent = RefreshIntent::ForceApi;
        self.needs_refresh = true;
    }

    fn set_date_range(&mut self, range: DateRange) {
        self.date_range = range;
        self.mode = Mode::Loading;
        self.refresh_intent = RefreshIntent::CacheOnly;
        self.needs_refresh = true;
    }

    fn handle_settings_input(&mut self, key: KeyEvent) {
        match self.settings_mode {
            SettingsMode::SelectCategory => self.handle_settings_category_input(key),
            SettingsMode::EditValue => self.handle_settings_value_input(key),
        }
    }

    fn handle_settings_category_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => {
                self.settings_input.clear();
                self.mode = Mode::Dashboard;
            }
            KeyCode::Up => self.select_previous_setting_category(),
            KeyCode::Down => self.select_next_setting_category(),
            KeyCode::Enter => {
                self.sync_settings_input_for_category();
                self.settings_mode = SettingsMode::EditValue;
            }
            _ => {}
        }
    }

    fn handle_settings_value_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Enter => {
                let parsed = match self.parse_target_hours() {
                    Ok(value) => value,
                    Err(message) => {
                        self.status = Some(message);
                        return;
                    }
                };
                if let Err(err) = storage::write_target_hours(parsed) {
                    self.status = Some(format!("Failed to save: {err}"));
                    return;
                }
                self.target_hours = parsed;
                self.settings_input = format!("{:.2}", parsed);
                self.status = Some("Target hours updated.".to_string());
                self.set_toast("Target hours saved.", false);
                self.settings_mode = SettingsMode::SelectCategory;
            }
            KeyCode::Backspace => {
                self.settings_input.pop();
            }
            KeyCode::Char(ch) => {
                if ch.is_ascii_digit() {
                    self.settings_input.push(ch);
                    return;
                }
                if ch == '.' || ch == ',' {
                    if self.settings_input.is_empty() {
                        return;
                    }
                    if self.settings_input.contains('.') || self.settings_input.contains(',') {
                        return;
                    }
                    self.settings_input.push(ch);
                }
            }
            KeyCode::Esc => {
                self.settings_input.clear();
                self.settings_mode = SettingsMode::SelectCategory;
            }
            _ => {}
        }
    }

    fn parse_target_hours(&self) -> Result<f64, String> {
        let mut value = self.settings_input.trim().to_string();
        if value.is_empty() {
            return Err("Target hours is required.".to_string());
        }

        if value.contains(',') && !value.contains('.') {
            value = value.replace(',', ".");
        }

        if value.ends_with('.') {
            return Err("Use a complete number (e.g. 8 or 8.50).".to_string());
        }

        let parts: Vec<&str> = value.split('.').collect();
        if parts.len() > 2 {
            return Err("Invalid number format.".to_string());
        }

        if parts.len() == 2 {
            let decimals = parts[1];
            if decimals.is_empty() || decimals.len() > 2 {
                return Err("Use up to 2 decimals (e.g. 8.50).".to_string());
            }
        }

        let parsed: f64 = value.parse().map_err(|_| "Invalid number format.".to_string())?;
        if parsed <= 0.0 {
            return Err("Target hours must be greater than 0.".to_string());
        }

        Ok((parsed * 100.0).round() / 100.0)
    }

    fn select_previous_setting_category(&mut self) {
        if self.settings_categories.is_empty() {
            return;
        }
        let selected = self.settings_state.selected().unwrap_or(0);
        let new_index = if selected == 0 {
            self.settings_categories.len() - 1
        } else {
            selected - 1
        };
        self.settings_state.select(Some(new_index));
        self.sync_settings_input_for_category();
    }

    fn select_next_setting_category(&mut self) {
        if self.settings_categories.is_empty() {
            return;
        }
        let selected = self.settings_state.selected().unwrap_or(0);
        let new_index = if selected + 1 >= self.settings_categories.len() {
            0
        } else {
            selected + 1
        };
        self.settings_state.select(Some(new_index));
        self.sync_settings_input_for_category();
    }

    fn sync_settings_input_for_category(&mut self) {
        let category = self.settings_selected_category();
        if category == "General" {
            self.settings_input = format!("{:.2}", self.target_hours);
        }
    }

    pub fn settings_categories(&self) -> &[String] {
        &self.settings_categories
    }

    pub fn settings_state(&mut self) -> &mut ListState {
        &mut self.settings_state
    }

    pub fn settings_is_editing(&self) -> bool {
        matches!(self.settings_mode, SettingsMode::EditValue)
    }

    pub fn settings_selected_category(&self) -> &str {
        self.settings_state
            .selected()
            .and_then(|index| self.settings_categories.get(index))
            .map(String::as_str)
            .unwrap_or("General")
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
        let message = if include_project {
            "Copied entries with project names."
        } else {
            "Copied entries to clipboard."
        };
        self.write_clipboard(text, message);
    }

    fn copy_client_entries_to_clipboard(&mut self) {
        let selected = match self.current_project() {
            Some(project) => project,
            None => {
                self.status = Some("Select a project first.".to_string());
                self.set_toast("Select a project first.", true);
                return;
            }
        };

        let mut lines: Vec<String> = Vec::new();
        if let Some(client_name) = selected.client_name.as_ref() {
            for project in &self.grouped {
                if project.client_name.as_deref() == Some(client_name.as_str()) {
                    for entry in &project.entries {
                        lines.push(format!("• {} ({:.2}h)", entry.description, entry.total_hours));
                    }
                }
            }
        } else {
            for entry in &selected.entries {
                lines.push(format!("• {} ({:.2}h)", entry.description, entry.total_hours));
            }
        }

        if lines.is_empty() {
            self.status = Some("No entries to copy.".to_string());
            self.set_toast("No entries to copy.", true);
            return;
        }

        let text = lines.join("\n");
        let message = if selected.client_name.is_some() {
            "Copied client entries."
        } else {
            "Copied project entries."
        };
        self.write_clipboard(text, message);
    }

    fn copy_project_entries_to_clipboard(&mut self) {
        let selected = match self.current_project() {
            Some(project) => project,
            None => {
                self.status = Some("Select a project first.".to_string());
                self.set_toast("Select a project first.", true);
                return;
            }
        };

        if selected.entries.is_empty() {
            self.status = Some("No entries to copy.".to_string());
            self.set_toast("No entries to copy.", true);
            return;
        }

        let text = selected
            .entries
            .iter()
            .map(|entry| format!("• {} ({:.2}h)", entry.description, entry.total_hours))
            .collect::<Vec<_>>()
            .join("\n");
        self.write_clipboard(text, "Copied project entries.");
    }

    fn write_clipboard(&mut self, text: String, success_message: &str) {
        match Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(text))
        {
            Ok(_) => {
                self.status = Some(success_message.to_string());
                self.set_toast(success_message, false);
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
        if include_project {
            let mut items: Vec<(Option<String>, String, String, f64)> = Vec::new();
            for project in &self.grouped {
                let client_name = project.client_name.clone();
                for entry in &project.entries {
                    items.push((
                        client_name.clone(),
                        project.project_name.clone(),
                        entry.description.clone(),
                        entry.total_hours,
                    ));
                }
            }
            items.sort_by(|a, b| {
                match (&a.0, &b.0) {
                    (Some(a), Some(b)) => a.cmp(b),
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, None) => std::cmp::Ordering::Equal,
                }
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
            });

            for (client, project, entry, hours) in items {
                if let Some(client) = client {
                    lines.push(format!(
                        "• {} — {} — {} ({:.2}h)",
                        client, project, entry, hours
                    ));
                } else {
                    lines.push(format!("• {} — {} ({:.2}h)", project, entry, hours));
                }
            }
            lines.push(String::new());
            lines.push(format!("Total hours: {:.2}h", self.total_hours));
        } else {
            for project in &self.grouped {
                for entry in &project.entries {
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

    pub fn visible_status(&mut self) -> Option<String> {
        if self.status.is_none() {
            self.status_created_at = None;
            self.last_status_snapshot = None;
            return None;
        }

        if self.last_status_snapshot.as_ref() != self.status.as_ref() {
            self.status_created_at = Some(Instant::now());
            self.last_status_snapshot = self.status.clone();
        }

        if matches!(self.mode, Mode::Error) {
            return self.status.clone();
        }

        if let Some(created_at) = self.status_created_at {
            if created_at.elapsed() > Duration::from_secs(4) {
                self.status = None;
                self.status_created_at = None;
                self.last_status_snapshot = None;
                return None;
            }
        }

        self.status.clone()
    }

    fn set_toast(&mut self, message: impl Into<String>, is_error: bool) {
        self.toast = Some(Toast {
            message: message.into(),
            created_at: Instant::now(),
            is_error,
        });
    }

    fn toggle_theme(&mut self) {
        self.theme = match self.theme {
            ThemePreference::Dark => ThemePreference::Light,
            ThemePreference::Light => ThemePreference::Dark,
        };
        if let Err(err) = storage::write_theme(self.theme) {
            self.status = Some(format!("Theme save failed: {err}"));
            self.set_toast("Theme save failed.", true);
        } else {
            let label = match self.theme {
                ThemePreference::Dark => "Dark mode",
                ThemePreference::Light => "Light mode",
            };
            self.status = Some(format!("Theme set to {label}."));
            self.set_toast(format!("{label} enabled."), false);
        }
    }

    pub fn settings_input_value(&self) -> &str {
        &self.settings_input
    }

    pub fn date_start_input_value(&self) -> &str {
        &self.date_start_input
    }

    pub fn date_end_input_value(&self) -> &str {
        &self.date_end_input
    }

    pub fn is_date_start_active(&self) -> bool {
        matches!(self.date_active_field, DateField::Start)
    }

    fn update_range_from_input(&self) -> Result<DateRange, String> {
        let start_text = self.date_start_input.trim();
        let end_text = self.date_end_input.trim();
        if start_text.is_empty() || end_text.is_empty() {
            return Err("Start and end date are required.".to_string());
        }
        let start_date = parse_date(start_text)?;
        let end_date = parse_date(end_text)?;
        if start_date > end_date {
            return Err("Start date cannot be after end date.".to_string());
        }
        Ok(DateRange::from_bounds(start_date, end_date))
    }

    fn active_date_input_mut(&mut self) -> &mut String {
        match self.date_active_field {
            DateField::Start => &mut self.date_start_input,
            DateField::End => &mut self.date_end_input,
        }
    }

    fn ensure_quota_today(&mut self) {
        let today = storage::today_string();
        if self.quota.date != today {
            self.quota.date = today;
            self.quota.used_calls = 0;
            let _ = storage::write_quota(&self.quota);
        }
    }

    fn ensure_cache_loaded(&mut self, token_hash: &str) {
        if self.cache.is_none() {
            if let Some(cache) = storage::read_cache() {
                if cache.token_hash == token_hash {
                    self.cache = Some(cache);
                }
            }
        }
    }

    fn quota_remaining(&self) -> u32 {
        CALL_LIMIT.saturating_sub(self.quota.used_calls)
    }

    fn consume_quota(&mut self) {
        self.quota.used_calls = self.quota.used_calls.saturating_add(1);
        let _ = storage::write_quota(&self.quota);
    }

    fn cache_mut(&mut self) -> &mut CacheFile {
        if self.cache.is_none() {
            let token_hash = self.token_hash.clone().unwrap_or_default();
            self.cache = Some(storage::new_cache(token_hash));
        }
        self.cache.as_mut().unwrap()
    }

    fn cached_workspaces(&self) -> Option<CachedData<Vec<Workspace>>> {
        self.cache.as_ref().and_then(|cache| cache.workspaces.clone())
    }

    fn cached_projects(&self, workspace_id: u64) -> Option<CachedData<Vec<Project>>> {
        self.cache
            .as_ref()
            .and_then(|cache| cache.projects.get(&workspace_id).cloned())
    }

    fn cached_clients(&self, workspace_id: u64) -> Option<CachedData<Vec<TogglClientModel>>> {
        self.cache
            .as_ref()
            .and_then(|cache| cache.clients.get(&workspace_id).cloned())
    }

    fn cached_time_entries(
        &self,
        workspace_id: u64,
        start: &str,
        end: &str,
    ) -> Option<CachedData<Vec<TimeEntry>>> {
        let key = storage::cache_key(workspace_id, start, end);
        self.cache
            .as_ref()
            .and_then(|cache| cache.time_entries.get(&key).cloned())
    }

    fn update_cache_workspaces(&mut self, workspaces: &[Workspace]) {
        let cached = CachedData {
            data: workspaces.to_vec(),
            fetched_at: storage::now_rfc3339(),
        };
        let cache = self.cache_mut();
        cache.workspaces = Some(cached);
        let _ = storage::write_cache(cache);
    }

    fn update_cache_projects(&mut self, workspace_id: u64, projects: &[Project]) {
        let cached = CachedData {
            data: projects.to_vec(),
            fetched_at: storage::now_rfc3339(),
        };
        let cache = self.cache_mut();
        cache.projects.insert(workspace_id, cached);
        let _ = storage::write_cache(cache);
    }

    fn update_cache_clients(&mut self, workspace_id: u64, clients: &[TogglClientModel]) {
        let cached = CachedData {
            data: clients.to_vec(),
            fetched_at: storage::now_rfc3339(),
        };
        let cache = self.cache_mut();
        cache.clients.insert(workspace_id, cached);
        let _ = storage::write_cache(cache);
    }

    fn update_cache_time_entries(
        &mut self,
        workspace_id: u64,
        start: &str,
        end: &str,
        entries: &[TimeEntry],
    ) {
        let cached = CachedData {
            data: entries.to_vec(),
            fetched_at: storage::now_rfc3339(),
        };
        let key = storage::cache_key(workspace_id, start, end);
        let cache = self.cache_mut();
        cache.time_entries.insert(key, cached);
        let _ = storage::write_cache(cache);
    }

    fn resolve_workspaces(
        &mut self,
        client: &TogglClient,
        allow_api: bool,
        cache_reason: &mut Option<CacheReason>,
        cache_timestamp: &mut Option<String>,
    ) -> Option<Vec<Workspace>> {
        let mut api_error: Option<TogglError> = None;
        if allow_api {
            if self.quota_remaining() == 0 {
                if cache_reason.is_none() {
                    *cache_reason = Some(CacheReason::Quota);
                }
            } else {
                self.consume_quota();
                match client.fetch_workspaces() {
                    Ok(workspaces) => {
                        self.update_cache_workspaces(&workspaces);
                        return Some(workspaces);
                    }
                    Err(err) => {
                        if matches!(err, TogglError::Unauthorized) {
                            self.handle_error(err);
                            return None;
                        }
                        api_error = Some(err);
                        if cache_reason.is_none() {
                            *cache_reason = Some(CacheReason::ApiError);
                        }
                    }
                }
            }
        } else if cache_reason.is_none() {
            *cache_reason = Some(CacheReason::CacheOnly);
        }

        if let Some(cached) = self.cached_workspaces() {
            if cache_timestamp.is_none() {
                *cache_timestamp = Some(cached.fetched_at.clone());
            }
            return Some(cached.data);
        }

        if let Some(err) = api_error {
            self.handle_error(err);
        } else if matches!(cache_reason, Some(CacheReason::Quota)) {
            self.mode = Mode::Error;
            self.status = Some(self.quota_exhausted_message());
        } else {
            self.mode = Mode::Error;
            self.status = Some(self.no_cache_message());
        }
        None
    }

    fn resolve_projects(
        &mut self,
        client: &TogglClient,
        allow_api: bool,
        workspace_id: u64,
        cache_reason: &mut Option<CacheReason>,
        cache_timestamp: &mut Option<String>,
    ) -> Option<Vec<Project>> {
        let mut api_error: Option<TogglError> = None;
        if allow_api {
            if self.quota_remaining() == 0 {
                if cache_reason.is_none() {
                    *cache_reason = Some(CacheReason::Quota);
                }
            } else {
                self.consume_quota();
                match client.fetch_projects(workspace_id) {
                    Ok(projects) => {
                        self.update_cache_projects(workspace_id, &projects);
                        return Some(projects);
                    }
                    Err(err) => {
                        if matches!(err, TogglError::Unauthorized) {
                            self.handle_error(err);
                            return None;
                        }
                        api_error = Some(err);
                        if cache_reason.is_none() {
                            *cache_reason = Some(CacheReason::ApiError);
                        }
                    }
                }
            }
        } else if cache_reason.is_none() {
            *cache_reason = Some(CacheReason::CacheOnly);
        }

        if let Some(cached) = self.cached_projects(workspace_id) {
            if cache_timestamp.is_none() {
                *cache_timestamp = Some(cached.fetched_at.clone());
            }
            return Some(cached.data);
        }

        if let Some(err) = api_error {
            self.handle_error(err);
        } else if matches!(cache_reason, Some(CacheReason::Quota)) {
            self.mode = Mode::Error;
            self.status = Some(self.quota_exhausted_message());
        } else {
            self.mode = Mode::Error;
            self.status = Some(self.no_cache_message());
        }
        None
    }

    fn resolve_client_names(
        &mut self,
        client: &TogglClient,
        allow_api: bool,
        workspace_id: u64,
        projects: &[Project],
    ) -> Option<HashMap<u64, String>> {
        let mut client_names = HashMap::new();
        let mut missing = HashSet::new();

        for project in projects {
            if let Some(client_id) = project.client_id {
                if let Some(name) = project.client_name.clone() {
                    client_names.insert(client_id, name);
                } else {
                    missing.insert(client_id);
                }
            }
        }

        if missing.is_empty() {
            return Some(client_names);
        }

        if let Some(cached) = self.cached_clients(workspace_id) {
            for client in cached.data {
                if missing.contains(&client.id) {
                    client_names.insert(client.id, client.name);
                }
            }
            return Some(client_names);
        }

        if !allow_api || self.quota_remaining() == 0 {
            return Some(client_names);
        }

        self.consume_quota();
        match client.fetch_clients(workspace_id) {
            Ok(clients) => {
                self.update_cache_clients(workspace_id, &clients);
                for client in clients {
                    if missing.contains(&client.id) {
                        client_names.insert(client.id, client.name);
                    }
                }
                Some(client_names)
            }
            Err(err) => {
                if matches!(err, TogglError::Unauthorized) {
                    self.handle_error(err);
                    return None;
                }
                Some(client_names)
            }
        }
    }

    fn resolve_time_entries(
        &mut self,
        client: &TogglClient,
        allow_api: bool,
        workspace_id: u64,
        start: &str,
        end: &str,
        cache_reason: &mut Option<CacheReason>,
        cache_timestamp: &mut Option<String>,
    ) -> Option<Vec<TimeEntry>> {
        let mut api_error: Option<TogglError> = None;
        if allow_api {
            if self.quota_remaining() == 0 {
                if cache_reason.is_none() {
                    *cache_reason = Some(CacheReason::Quota);
                }
            } else {
                self.consume_quota();
                match client.fetch_time_entries(start, end) {
                    Ok(entries) => {
                        self.update_cache_time_entries(workspace_id, start, end, &entries);
                        return Some(entries);
                    }
                    Err(err) => {
                        if matches!(err, TogglError::Unauthorized) {
                            self.handle_error(err);
                            return None;
                        }
                        api_error = Some(err);
                        if cache_reason.is_none() {
                            *cache_reason = Some(CacheReason::ApiError);
                        }
                    }
                }
            }
        } else if cache_reason.is_none() {
            *cache_reason = Some(CacheReason::CacheOnly);
        }

        if let Some(cached) = self.cached_time_entries(workspace_id, start, end) {
            *cache_timestamp = Some(cached.fetched_at.clone());
            return Some(cached.data);
        }

        if let Some(err) = api_error {
            self.handle_error(err);
        } else if matches!(cache_reason, Some(CacheReason::Quota)) {
            self.mode = Mode::Error;
            self.status = Some(self.quota_exhausted_message());
        } else {
            self.mode = Mode::Error;
            self.status = Some(self.no_cache_message());
        }
        None
    }

    fn no_cache_message(&self) -> String {
        "No cached data available. Press r to fetch.".to_string()
    }

    fn quota_exhausted_message(&self) -> String {
        format!(
            "{} No cached data available.",
            self.quota_message()
        )
    }

    fn cache_status_message(&self, reason: CacheReason, cached_at: Option<&str>) -> String {
        let updated = cached_at
            .and_then(|value| parse_cached_time(value))
            .map(|value| format!(" (last updated {})", value.format("%Y-%m-%d %H:%M")))
            .unwrap_or_default();
        match reason {
            CacheReason::CacheOnly => format!("Using cached data{updated}."),
            CacheReason::Quota => format!(
                "{} Using cached data{updated}.",
                self.quota_message()
            ),
            CacheReason::ApiError => format!("Using cached data due to API error{updated}."),
        }
    }

    fn quota_message(&self) -> String {
        let remaining = self.quota_remaining();
        if remaining == 0 {
            format!("Quota reached ({}/{}).", self.quota.used_calls, CALL_LIMIT)
        } else {
            format!(
                "Quota low (remaining {}/{}).",
                remaining, CALL_LIMIT
            )
        }
    }
}

fn parse_cached_time(value: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
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
