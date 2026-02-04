use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::ListState;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::dates::{parse_date, DateRange};
use crate::grouping::{group_entries, GroupedEntry, GroupedProject};
use crate::models::{Client as TogglClientModel, Project, TimeEntry, Workspace};
use crate::rounding::{RoundingConfig, RoundingMode};
use crate::storage::{
    self, CacheFile, CachedData, QuotaFile, ThemePreference,
};
use crate::toggl::{TogglClient, TogglError};
use crate::update::{self, UpdateInfo};
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
    UpdatePrompt,
    Updating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardFocus {
    Projects,
    Entries,
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
pub enum SettingsFocus {
    Categories,
    Items,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsItem {
    TargetHours,
    TimeRoundingToggle,
    RoundingIncrement,
    RoundingMode,
    TogglToken,
}

pub struct App {
    pub should_quit: bool,
    pub needs_refresh: bool,
    pub mode: Mode,
    pub dashboard_focus: DashboardFocus,
    pub status: Option<String>,
    pub input: String,
    pub token: Option<String>,
    pub workspace_list: Vec<Workspace>,
    pub workspace_state: ListState,
    pub selected_workspace: Option<Workspace>,
    pub date_range: DateRange,
    pub projects: Vec<Project>,
    pub time_entries: Vec<TimeEntry>,
    pub client_names: HashMap<u64, String>,
    pub grouped: Vec<GroupedProject>,
    pub total_hours: f64,
    pub project_state: ListState,
    pub entry_state: ListState,
    pub last_refresh: Option<DateTime<Local>>,
    pub show_help: bool,
    pub theme: ThemePreference,
    pub target_hours: f64,
    pub update_info: Option<UpdateInfo>,
    pub update_error: Option<String>,
    pub rounding: Option<RoundingConfig>,
    token_hash: Option<String>,
    cache: Option<CacheFile>,
    quota: QuotaFile,
    refresh_intent: RefreshIntent,
    update_resume_mode: Option<Mode>,
    needs_update_check: bool,
    needs_update_install: bool,
    exit_message: Option<String>,
    date_start_input: String,
    date_end_input: String,
    date_active_field: DateField,
    settings_input: String,
    settings_categories: Vec<String>,
    settings_state: ListState,
    settings_items: Vec<SettingsItem>,
    settings_item_state: ListState,
    settings_focus: SettingsFocus,
    settings_edit_item: Option<SettingsItem>,
    settings_rounding_draft: RoundingConfig,
    settings_rounding_draft_enabled: bool,
    status_created_at: Option<Instant>,
    last_status_snapshot: Option<String>,
    toast: Option<Toast>,
}

impl App {
    pub fn new(date_range: DateRange, force_login: bool, needs_update_check: bool) -> Self {
        let token = if force_login {
            None
        } else {
            storage::read_token()
        };
        let mode = if token.is_some() { Mode::Loading } else { Mode::Login };
        let theme = storage::read_theme().unwrap_or(ThemePreference::Terminal);
        let target_hours = storage::read_target_hours().unwrap_or(8.0);
        let rounding = storage::read_rounding();
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
            dashboard_focus: DashboardFocus::Projects,
            status: None,
            input: String::new(),
            token,
            workspace_list: Vec::new(),
            workspace_state,
            selected_workspace: None,
            date_range,
            projects: Vec::new(),
            time_entries: Vec::new(),
            client_names: HashMap::new(),
            grouped: Vec::new(),
            total_hours: 0.0,
            project_state,
            entry_state: ListState::default(),
            last_refresh: None,
            show_help: false,
            theme,
            target_hours,
            update_info: None,
            update_error: None,
            rounding,
            token_hash,
            cache,
            quota,
            refresh_intent: RefreshIntent::CacheOnly,
            update_resume_mode: None,
            needs_update_check,
            needs_update_install: false,
            exit_message: None,
            date_start_input: String::new(),
            date_end_input: String::new(),
            date_active_field: DateField::Start,
            settings_input: String::new(),
            settings_categories: vec!["General".to_string(), "Integrations".to_string()],
            settings_state: {
                let mut state = ListState::default();
                state.select(Some(0));
                state
            },
            settings_items: Vec::new(),
            settings_item_state: {
                let mut state = ListState::default();
                state.select(Some(0));
                state
            },
            settings_focus: SettingsFocus::Categories,
            settings_edit_item: None,
            settings_rounding_draft: RoundingConfig::default(),
            settings_rounding_draft_enabled: false,
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
            Mode::UpdatePrompt => self.handle_update_prompt_input(key),
            Mode::Updating => {}
            Mode::Dashboard | Mode::Loading | Mode::Error => self.handle_dashboard_input(key),
        }
    }

    pub fn needs_update_check(&self) -> bool {
        self.needs_update_check
    }

    pub fn needs_update_install(&self) -> bool {
        self.needs_update_install
    }

    pub fn is_update_blocking(&self) -> bool {
        matches!(self.mode, Mode::UpdatePrompt | Mode::Updating)
    }

    pub fn take_exit_message(&mut self) -> Option<String> {
        self.exit_message.take()
    }

    pub fn check_for_update(&mut self) {
        if !self.needs_update_check {
            return;
        }
        self.needs_update_check = false;
        self.update_error = None;

        match update::check_for_update() {
            Ok(Some(info)) => {
                self.update_info = Some(info);
                self.update_resume_mode = Some(self.mode);
                self.mode = Mode::UpdatePrompt;
                self.status = None;
            }
            Ok(None) => {}
            Err(err) => {
                let message = format!("Update check failed: {err}");
                self.status = Some(message.clone());
                self.set_toast(message, true);
            }
        }
    }

    pub fn perform_update(&mut self) {
        if !self.needs_update_install {
            return;
        }
        self.needs_update_install = false;

        let info = match self.update_info.clone() {
            Some(info) => info,
            None => {
                self.handle_update_failure("Update info missing.".to_string());
                return;
            }
        };

        let current_exe = match std::env::current_exe() {
            Ok(path) => path,
            Err(err) => {
                self.handle_update_failure(format!(
                    "Failed to locate current binary: {err}"
                ));
                return;
            }
        };

        let staged_path = match update::download_and_extract(&info) {
            Ok(path) => path,
            Err(err) => {
                self.handle_update_failure(err.to_string());
                return;
            }
        };

        let install_result = update::install_update(&staged_path, &current_exe);
        if !cfg!(windows) {
            update::cleanup_staged(&staged_path);
        }

        match install_result {
            Ok(()) => {
                self.exit_message = Some(format!(
                    "Updated to v{}. Please relaunch.",
                    info.latest
                ));
                self.should_quit = true;
            }
            Err(err) => {
                self.handle_update_failure(err.to_string());
            }
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

        let grouped = group_entries(
            &valid_entries,
            &projects,
            &client_names,
            self.rounding.as_ref(),
        );
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
        self.client_names = client_names;
        self.grouped = grouped;
        self.total_hours = total_hours;
        self.last_refresh = if allow_api && cache_reason.is_none() {
            Some(Local::now())
        } else {
            cache_timestamp
                .as_ref()
                .and_then(|value| parse_cached_time(value))
        };
        self.sync_entry_selection_for_project();

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

    fn handle_update_prompt_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('u') | KeyCode::Enter => self.start_update(),
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            _ => {}
        }
    }

    fn start_update(&mut self) {
        self.mode = Mode::Updating;
        self.needs_update_install = true;
        self.update_error = None;
    }

    fn resume_from_update(&mut self) {
        if let Some(mode) = self.update_resume_mode.take() {
            self.mode = mode;
            return;
        }

        if self.token.is_some() {
            self.mode = Mode::Loading;
            self.needs_refresh = true;
        } else {
            self.mode = Mode::Login;
        }
    }

    fn handle_update_failure(&mut self, message: String) {
        self.update_error = Some(message.clone());
        let status = format!("Update failed: {message}");
        self.status = Some(status.clone());
        self.set_toast("Update failed; continuing without update.", true);
        self.resume_from_update();
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
            KeyCode::Right | KeyCode::Tab => self.enter_entries_focus(),
            KeyCode::Left | KeyCode::BackTab if self.dashboard_focus == DashboardFocus::Entries => {
                self.exit_entries_focus();
            }
            KeyCode::Enter if self.dashboard_focus == DashboardFocus::Projects => {
                self.enter_entries_focus();
            }
            KeyCode::Char('b') | KeyCode::Char('B')
                if self.dashboard_focus == DashboardFocus::Entries =>
            {
                self.copy_entry_title_to_clipboard();
            }
            KeyCode::Char('n') | KeyCode::Char('N')
                if self.dashboard_focus == DashboardFocus::Entries =>
            {
                self.copy_entry_hours_to_clipboard();
            }
            KeyCode::Esc if self.dashboard_focus == DashboardFocus::Entries => self.exit_entries_focus(),
            KeyCode::Up => match self.dashboard_focus {
                DashboardFocus::Projects => self.select_previous_project(),
                DashboardFocus::Entries => self.select_previous_entry(),
            },
            KeyCode::Down => match self.dashboard_focus {
                DashboardFocus::Projects => self.select_next_project(),
                DashboardFocus::Entries => self.select_next_entry(),
            },
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
        self.settings_input.clear();
        self.settings_focus = SettingsFocus::Categories;
        self.settings_edit_item = None;
        self.sync_settings_items_for_category();
        self.mode = Mode::Settings;
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
        match self.settings_focus {
            SettingsFocus::Categories => self.handle_settings_category_input(key),
            SettingsFocus::Items => self.handle_settings_items_input(key),
            SettingsFocus::Edit => self.handle_settings_edit_input(key),
        }
    }

    fn handle_settings_category_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => {
                self.settings_input.clear();
                self.settings_edit_item = None;
                self.settings_focus = SettingsFocus::Categories;
                self.mode = Mode::Dashboard;
            }
            KeyCode::Up => self.select_previous_setting_category(),
            KeyCode::Down => self.select_next_setting_category(),
            KeyCode::Enter => {
                self.sync_settings_items_for_category();
                self.settings_focus = SettingsFocus::Items;
            }
            _ => {}
        }
    }

    fn handle_settings_items_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => {
                self.settings_focus = SettingsFocus::Categories;
                self.settings_edit_item = None;
                self.settings_input.clear();
            }
            KeyCode::Up => self.select_previous_setting_item(),
            KeyCode::Down => self.select_next_setting_item(),
            KeyCode::Enter => {
                self.begin_edit_selected_setting_item();
            }
            _ => {}
        }
    }

    fn handle_settings_edit_input(&mut self, key: KeyEvent) {
        let Some(item) = self.settings_edit_item else {
            self.settings_focus = SettingsFocus::Items;
            return;
        };

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => {
                self.settings_input.clear();
                self.settings_edit_item = None;
                self.settings_focus = SettingsFocus::Items;
            }
            KeyCode::Enter => {
                self.save_setting_item(item);
            }
            KeyCode::Up => match item {
                SettingsItem::TimeRoundingToggle => {
                    self.settings_rounding_draft_enabled = !self.settings_rounding_draft_enabled;
                }
                SettingsItem::RoundingIncrement => {
                    self.cycle_rounding_increment(true);
                }
                SettingsItem::RoundingMode => {
                    self.cycle_rounding_mode(true);
                }
                _ => {}
            },
            KeyCode::Down => match item {
                SettingsItem::TimeRoundingToggle => {
                    self.settings_rounding_draft_enabled = !self.settings_rounding_draft_enabled;
                }
                SettingsItem::RoundingIncrement => {
                    self.cycle_rounding_increment(false);
                }
                SettingsItem::RoundingMode => {
                    self.cycle_rounding_mode(false);
                }
                _ => {}
            },
            KeyCode::Backspace => match item {
                SettingsItem::TargetHours | SettingsItem::TogglToken => {
                    self.settings_input.pop();
                }
                _ => {}
            },
            KeyCode::Char(ch) => match item {
                SettingsItem::TogglToken => {
                    if !ch.is_control() {
                        self.settings_input.push(ch);
                    }
                }
                SettingsItem::TargetHours => {
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
                _ => {}
            },
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

    fn rebuild_grouped(&mut self) {
        let selected_project_key = self.current_project().map(|project| {
            (
                project.client_name.clone(),
                project.project_name.clone(),
            )
        });
        let selected_entry_key = self.current_entry().map(|entry| entry.description.clone());

        let grouped = group_entries(
            &self.time_entries,
            &self.projects,
            &self.client_names,
            self.rounding.as_ref(),
        );
        let total_hours: f64 = grouped.iter().map(|group| group.total_hours).sum();

        self.grouped = grouped;
        self.total_hours = total_hours;

        if let Some((client_name, project_name)) = selected_project_key {
            if let Some(index) = self
                .grouped
                .iter()
                .position(|project| project.client_name == client_name && project.project_name == project_name)
            {
                self.project_state.select(Some(index));
            }
        }

        self.sync_entry_selection_for_project();

        if let Some(entry_desc) = selected_entry_key {
            if let Some(project) = self.current_project() {
                if let Some(index) = project.entries.iter().position(|entry| entry.description == entry_desc) {
                    self.entry_state.select(Some(index));
                }
            }
        }

        self.sync_entry_selection_for_project();
    }

    fn sync_settings_items_for_category(&mut self) {
        self.settings_items = match self.settings_selected_category() {
            "Integrations" => vec![SettingsItem::TogglToken],
            _ => vec![
                SettingsItem::TargetHours,
                SettingsItem::TimeRoundingToggle,
                SettingsItem::RoundingIncrement,
                SettingsItem::RoundingMode,
            ],
        };
        if !self.settings_items.is_empty() {
            self.settings_item_state.select(Some(0));
        } else {
            self.settings_item_state.select(None);
        }
    }

    fn begin_edit_selected_setting_item(&mut self) {
        let Some(index) = self.settings_item_state.selected() else {
            return;
        };
        let Some(item) = self.settings_items.get(index).copied() else {
            return;
        };

        match item {
            SettingsItem::TargetHours => {
                self.settings_input = format!("{:.2}", self.target_hours);
            }
            SettingsItem::TogglToken => {
                self.settings_input = self.token.clone().unwrap_or_default();
            }
            SettingsItem::TimeRoundingToggle => {
                self.settings_rounding_draft_enabled = self.rounding.is_some();
                self.settings_rounding_draft = self.rounding.unwrap_or_default();
            }
            SettingsItem::RoundingIncrement | SettingsItem::RoundingMode => {
                let Some(rounding) = self.rounding else {
                    self.status = Some("Enable time rounding first.".to_string());
                    self.set_toast("Enable time rounding first.", true);
                    return;
                };
                self.settings_rounding_draft_enabled = true;
                self.settings_rounding_draft = rounding;
            }
        }

        self.settings_edit_item = Some(item);
        self.settings_focus = SettingsFocus::Edit;
        self.status = None;
    }

    fn save_setting_item(&mut self, item: SettingsItem) {
        match item {
            SettingsItem::TargetHours => {
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
                self.settings_edit_item = None;
                self.settings_focus = SettingsFocus::Items;
            }
            SettingsItem::TogglToken => {
                let token = self.settings_input.trim().to_string();
                if token.is_empty() {
                    self.status = Some("Token is required.".to_string());
                    return;
                }
                if let Err(err) = storage::write_token(&token) {
                    self.status = Some(format!("Failed to save token: {err}"));
                    return;
                }
                self.token = Some(token.clone());
                self.token_hash = Some(storage::hash_token(&token));
                self.cache = self
                    .token_hash
                    .as_ref()
                    .and_then(|hash| storage::read_cache().filter(|cache| cache.token_hash == *hash))
                    .or_else(|| self.token_hash.clone().map(storage::new_cache));
                self.status = Some("Toggl token updated.".to_string());
                self.set_toast("Token updated.", false);
                self.settings_edit_item = None;
                self.settings_focus = SettingsFocus::Items;
                self.mode = Mode::Loading;
                self.refresh_intent = RefreshIntent::CacheOnly;
                self.needs_refresh = true;
            }
            SettingsItem::TimeRoundingToggle => {
                let next = if self.settings_rounding_draft_enabled {
                    Some(self.settings_rounding_draft)
                } else {
                    None
                };
                if let Err(err) = storage::write_rounding(next) {
                    self.status = Some(format!("Failed to save: {err}"));
                    return;
                }
                self.rounding = next;
                self.status = Some(if self.rounding.is_some() {
                    "Time rounding enabled.".to_string()
                } else {
                    "Time rounding disabled.".to_string()
                });
                self.set_toast("Rounding saved.", false);
                self.settings_edit_item = None;
                self.settings_focus = SettingsFocus::Items;
                self.rebuild_grouped();
            }
            SettingsItem::RoundingIncrement | SettingsItem::RoundingMode => {
                if !self.settings_rounding_draft_enabled {
                    self.status = Some("Enable time rounding first.".to_string());
                    self.set_toast("Enable time rounding first.", true);
                    self.settings_edit_item = None;
                    self.settings_focus = SettingsFocus::Items;
                    return;
                }
                let next = Some(self.settings_rounding_draft);
                if let Err(err) = storage::write_rounding(next) {
                    self.status = Some(format!("Failed to save: {err}"));
                    return;
                }
                self.rounding = next;
                self.status = Some("Time rounding updated.".to_string());
                self.set_toast("Rounding saved.", false);
                self.settings_edit_item = None;
                self.settings_focus = SettingsFocus::Items;
                self.rebuild_grouped();
            }
        }
    }

    fn cycle_rounding_increment(&mut self, up: bool) {
        let values = [15u32, 30, 45, 60];
        let current = self.settings_rounding_draft.increment_minutes;
        let index = values.iter().position(|value| *value == current).unwrap_or(0);
        let next_index = if up {
            if index == 0 { values.len() - 1 } else { index - 1 }
        } else {
            if index + 1 >= values.len() { 0 } else { index + 1 }
        };
        self.settings_rounding_draft.increment_minutes = values[next_index];
    }

    fn cycle_rounding_mode(&mut self, up: bool) {
        let values = [RoundingMode::Closest, RoundingMode::Up, RoundingMode::Down];
        let current = self.settings_rounding_draft.mode;
        let index = values.iter().position(|value| *value == current).unwrap_or(0);
        let next_index = if up {
            if index == 0 { values.len() - 1 } else { index - 1 }
        } else {
            if index + 1 >= values.len() { 0 } else { index + 1 }
        };
        self.settings_rounding_draft.mode = values[next_index];
    }

    fn select_previous_setting_item(&mut self) {
        if self.settings_items.is_empty() {
            return;
        }
        let selected = self.settings_item_state.selected().unwrap_or(0);
        let new_index = if selected == 0 {
            self.settings_items.len() - 1
        } else {
            selected - 1
        };
        self.settings_item_state.select(Some(new_index));
    }

    fn select_next_setting_item(&mut self) {
        if self.settings_items.is_empty() {
            return;
        }
        let selected = self.settings_item_state.selected().unwrap_or(0);
        let new_index = if selected + 1 >= self.settings_items.len() {
            0
        } else {
            selected + 1
        };
        self.settings_item_state.select(Some(new_index));
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
        self.sync_settings_items_for_category();
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
        self.sync_settings_items_for_category();
    }

    pub fn settings_categories(&self) -> &[String] {
        &self.settings_categories
    }

    pub fn settings_state(&mut self) -> &mut ListState {
        &mut self.settings_state
    }

    pub fn settings_selected_category(&self) -> &str {
        self.settings_state
            .selected()
            .and_then(|index| self.settings_categories.get(index))
            .map(String::as_str)
            .unwrap_or("General")
    }

    pub fn settings_items(&self) -> &[SettingsItem] {
        &self.settings_items
    }

    pub fn settings_item_state(&mut self) -> &mut ListState {
        &mut self.settings_item_state
    }

    pub fn settings_focus(&self) -> SettingsFocus {
        self.settings_focus
    }

    pub fn settings_edit_item(&self) -> Option<SettingsItem> {
        self.settings_edit_item
    }

    pub fn settings_rounding_enabled_display(&self) -> bool {
        if self.settings_focus == SettingsFocus::Edit
            && self.settings_edit_item == Some(SettingsItem::TimeRoundingToggle)
        {
            return self.settings_rounding_draft_enabled;
        }
        self.rounding.is_some()
    }

    pub fn settings_rounding_config_display(&self) -> Option<RoundingConfig> {
        if self.settings_focus == SettingsFocus::Edit
            && matches!(
                self.settings_edit_item,
                Some(SettingsItem::TimeRoundingToggle)
                    | Some(SettingsItem::RoundingIncrement)
                    | Some(SettingsItem::RoundingMode)
            )
        {
            return if self.settings_rounding_draft_enabled {
                Some(self.settings_rounding_draft)
            } else {
                None
            };
        }
        self.rounding
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
        self.entry_state.select(Some(0));
        self.sync_entry_selection_for_project();
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
        self.entry_state.select(Some(0));
        self.sync_entry_selection_for_project();
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

    pub fn current_entry(&self) -> Option<&GroupedEntry> {
        let project = self.current_project()?;
        let selected = self.entry_state.selected()?;
        project.entries.get(selected)
    }

    fn sync_entry_selection_for_project(&mut self) {
        let Some(project) = self.current_project() else {
            self.entry_state.select(None);
            if self.dashboard_focus == DashboardFocus::Entries {
                self.dashboard_focus = DashboardFocus::Projects;
            }
            return;
        };

        if project.entries.is_empty() {
            self.entry_state.select(None);
            return;
        }

        let selected = self.entry_state.selected().unwrap_or(0);
        let selected = selected.min(project.entries.len().saturating_sub(1));
        self.entry_state.select(Some(selected));
    }

    fn enter_entries_focus(&mut self) {
        self.dashboard_focus = DashboardFocus::Entries;
        self.sync_entry_selection_for_project();
    }

    fn exit_entries_focus(&mut self) {
        self.dashboard_focus = DashboardFocus::Projects;
    }

    fn select_previous_entry(&mut self) {
        let Some(project) = self.current_project() else {
            return;
        };
        if project.entries.is_empty() {
            self.entry_state.select(None);
            return;
        }

        let selected = self.entry_state.selected().unwrap_or(0);
        let new_index = if selected == 0 {
            project.entries.len() - 1
        } else {
            selected - 1
        };
        self.entry_state.select(Some(new_index));
    }

    fn select_next_entry(&mut self) {
        let Some(project) = self.current_project() else {
            return;
        };
        if project.entries.is_empty() {
            self.entry_state.select(None);
            return;
        }

        let selected = self.entry_state.selected().unwrap_or(0);
        let new_index = if selected + 1 >= project.entries.len() {
            0
        } else {
            selected + 1
        };
        self.entry_state.select(Some(new_index));
    }

    fn copy_entry_title_to_clipboard(&mut self) {
        let selected = match self.current_entry() {
            Some(entry) => entry,
            None => {
                self.status = Some("Select an entry first.".to_string());
                self.set_toast("Select an entry first.", true);
                return;
            }
        };

        self.write_clipboard(selected.description.clone(), "Copied entry title.");
    }

    fn copy_entry_hours_to_clipboard(&mut self) {
        let selected = match self.current_entry() {
            Some(entry) => entry,
            None => {
                self.status = Some("Select an entry first.".to_string());
                self.set_toast("Select an entry first.", true);
                return;
            }
        };

        self.write_clipboard(format!("{:.2}", selected.total_hours), "Copied entry hours.");
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
            ThemePreference::Light | ThemePreference::Terminal => ThemePreference::Dark,
        };
        if let Err(err) = storage::write_theme(self.theme) {
            self.status = Some(format!("Theme save failed: {err}"));
            self.set_toast("Theme save failed.", true);
        } else {
            let label = theme_label(self.theme);
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
        self.quota.used_calls = self.quota.used_calls.saturating_add(1).min(CALL_LIMIT);
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
        _cache_timestamp: &mut Option<String>,
    ) -> Option<Vec<Workspace>> {
        if let Some(cached) = self.cached_workspaces() {
            return Some(cached.data);
        }

        if !allow_api {
            if cache_reason.is_none() {
                *cache_reason = Some(CacheReason::CacheOnly);
            }
            self.mode = Mode::Error;
            self.status = Some(self.no_cache_message());
            return None;
        }

        let mut api_error: Option<TogglError> = None;
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
            }
        }

        if let Some(err) = api_error {
            self.handle_error(err);
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
        _cache_timestamp: &mut Option<String>,
    ) -> Option<Vec<Project>> {
        if let Some(cached) = self.cached_projects(workspace_id) {
            return Some(cached.data);
        }

        if !allow_api {
            if cache_reason.is_none() {
                *cache_reason = Some(CacheReason::CacheOnly);
            }
            self.mode = Mode::Error;
            self.status = Some(self.no_cache_message());
            return None;
        }

        let mut api_error: Option<TogglError> = None;
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
            }
        }

        if let Some(err) = api_error {
            self.handle_error(err);
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

        if !allow_api {
            return Some(client_names);
        }

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

fn theme_label(theme: ThemePreference) -> &'static str {
    match theme {
        ThemePreference::Terminal => "Terminal",
        ThemePreference::Dark => "Midnight",
        ThemePreference::Light => "Snow",
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
