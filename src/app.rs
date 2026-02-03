use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::ListState;
use std::time::{Duration, Instant};

use crate::dates::{parse_date, DateRange};
use crate::grouping::{group_entries, GroupedProject};
use crate::models::{Project, TimeEntry, Workspace};
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
    token_hash: Option<String>,
    cache: Option<CacheFile>,
    quota: QuotaFile,
    refresh_intent: RefreshIntent,
    date_start_input: String,
    date_end_input: String,
    date_active_field: DateField,
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
            token_hash,
            cache,
            quota,
            refresh_intent: RefreshIntent::CacheOnly,
            date_start_input: String::new(),
            date_end_input: String::new(),
            date_active_field: DateField::Start,
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
                self.date_range = DateRange::today();
                self.trigger_refresh();
            }
            KeyCode::Char('h') => self.show_help = true,
            KeyCode::Char('m') | KeyCode::Char('M') => self.toggle_theme(),
            KeyCode::Char('d') => self.enter_date_input(DateInputMode::Range),
            KeyCode::Char('c') | KeyCode::Char('C') => self.copy_entries_to_clipboard(false),
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

    fn trigger_refresh(&mut self) {
        self.mode = Mode::Loading;
        self.refresh_intent = RefreshIntent::ForceApi;
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
