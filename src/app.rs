use chrono::{DateTime, Datelike, Local, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::dates::{DateRange, parse_date};
use crate::grouping::{GroupedEntry, GroupedProject, group_entries};
use crate::models::{Client as TogglClientModel, Project, TimeEntry, Workspace};
use crate::rollups::{DailyTotal, PeriodRollup, Rollups, WeekStart, build_rollups};
use crate::rounding::{RoundingConfig, RoundingMode};
use crate::storage::{self, CacheFile, CachedData, QuotaFile, RollupPreferences, ThemePreference};
use crate::toggl::{TogglClient, TogglError};
use crate::update::{self, UpdateInfo};
use arboard::Clipboard;

const CALL_LIMIT: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Loading,
    Dashboard,
    Rollups,
    RefetchConfirm,
    Login,
    WorkspaceSelect,
    DateInput(DateInputMode),
    Settings,
    Error,
    Updating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardFocus {
    Projects,
    Entries,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollupView {
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollupFocus {
    Periods,
    Days,
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
    Theme,
    TargetHours,
    RollupsIncludeWeekends,
    RollupsWeekStart,
    TimeRoundingToggle,
    RoundingIncrement,
    RoundingMode,
    TogglToken,
}

#[derive(Debug, Clone)]
struct RefetchPlan {
    start: NaiveDate,
    end: NaiveDate,
    scope_label: String,
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
    pub rollups: Rollups,
    pub rollup_view: RollupView,
    pub rollup_focus: RollupFocus,
    pub rollup_week_state: ListState,
    pub rollup_month_state: ListState,
    pub rollup_year_state: ListState,
    pub rollup_day_state: ListState,
    pub rollups_include_weekends: bool,
    pub rollups_week_start: WeekStart,
    non_working_days: HashSet<NaiveDate>,
    pub last_refresh: Option<DateTime<Local>>,
    pub show_help: bool,
    pub theme: ThemePreference,
    pub target_hours: f64,
    pub update_info: Option<UpdateInfo>,
    pub update_error: Option<String>,
    pub update_installable: bool,
    pub rounding: Option<RoundingConfig>,
    token_hash: Option<String>,
    cache: Option<CacheFile>,
    quota: QuotaFile,
    refresh_intent: RefreshIntent,
    refresh_resume_mode: Option<Mode>,
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
    settings_theme_draft: ThemePreference,
    settings_rollups_include_weekends_draft: bool,
    settings_rollups_week_start_draft: WeekStart,
    refetch_plan: Option<RefetchPlan>,
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
        let mode = if token.is_some() {
            Mode::Loading
        } else {
            Mode::Login
        };
        let theme = storage::read_theme().unwrap_or(ThemePreference::Terminal);
        let target_hours = storage::read_target_hours().unwrap_or(8.0);
        let rounding = storage::read_rounding();
        let rollup_preferences = storage::read_rollup_preferences();
        let non_working_days = storage::read_non_working_days();
        let token_hash = token.as_ref().map(|value| storage::hash_token(value));
        let cache = token_hash
            .as_ref()
            .and_then(|hash| storage::read_cache().filter(|cache| cache.token_hash == *hash));
        let quota = storage::read_quota();
        let mut project_state = ListState::default();
        project_state.select(Some(0));
        let mut workspace_state = ListState::default();
        workspace_state.select(Some(0));
        let mut rollup_week_state = ListState::default();
        rollup_week_state.select(Some(0));
        let mut rollup_month_state = ListState::default();
        rollup_month_state.select(Some(0));
        let mut rollup_year_state = ListState::default();
        rollup_year_state.select(Some(0));
        let mut rollup_day_state = ListState::default();
        rollup_day_state.select(Some(0));

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
            rollups: Rollups::default(),
            rollup_view: RollupView::Weekly,
            rollup_focus: RollupFocus::Periods,
            rollup_week_state,
            rollup_month_state,
            rollup_year_state,
            rollup_day_state,
            rollups_include_weekends: rollup_preferences.include_weekends,
            rollups_week_start: rollup_preferences.week_start,
            non_working_days,
            last_refresh: None,
            show_help: false,
            theme,
            target_hours,
            update_info: None,
            update_error: None,
            update_installable: false,
            rounding,
            token_hash,
            cache,
            quota,
            refresh_intent: RefreshIntent::CacheOnly,
            refresh_resume_mode: None,
            update_resume_mode: None,
            needs_update_check,
            needs_update_install: false,
            exit_message: None,
            date_start_input: String::new(),
            date_end_input: String::new(),
            date_active_field: DateField::Start,
            settings_input: String::new(),
            settings_categories: vec![
                "General".to_string(),
                "Rollups".to_string(),
                "Integrations".to_string(),
            ],
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
            settings_theme_draft: theme,
            settings_rollups_include_weekends_draft: rollup_preferences.include_weekends,
            settings_rollups_week_start_draft: rollup_preferences.week_start,
            refetch_plan: None,
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
            Mode::RefetchConfirm => self.handle_refetch_confirm_input(key),
            Mode::Updating => {}
            Mode::Rollups => self.handle_rollups_input(key),
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
        matches!(self.mode, Mode::Updating)
    }

    pub fn take_exit_message(&mut self) -> Option<String> {
        self.exit_message.take()
    }

    pub fn refetch_plan_view(&self) -> Option<RefetchPlanView> {
        let plan = self.refetch_plan.as_ref()?;
        let days = days_between(plan.start, plan.end);
        Some(RefetchPlanView {
            scope_label: plan.scope_label.clone(),
            start: plan.start.format("%Y-%m-%d").to_string(),
            end: plan.end.format("%Y-%m-%d").to_string(),
            days,
            estimated_calls: days as u32,
            remaining_calls: self.quota_remaining(),
        })
    }

    pub fn check_for_update(&mut self) {
        if !self.needs_update_check {
            return;
        }
        self.needs_update_check = false;
        self.update_error = None;

        match update::check_for_update() {
            Ok(Some(info)) => {
                self.update_installable = update::can_self_update();
                let message = if self.update_installable {
                    format!("Update available: v{} (press u to update)", info.latest)
                } else {
                    format!(
                        "Update available: v{} (update via package manager)",
                        info.latest
                    )
                };
                self.update_info = Some(info);
                self.status = Some(message.clone());
                self.set_toast(message, false);
            }
            Ok(None) => {
                self.update_info = None;
                self.update_installable = false;
            }
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
                self.handle_update_failure(format!("Failed to locate current binary: {err}"));
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
                self.exit_message = Some(format!("Updated to v{}. Please relaunch.", info.latest));
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
        let resume_mode = self.refresh_resume_mode.take();

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
        let allow_api = manual_refresh;
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

        let mut projects = match self.resolve_projects(
            &client,
            allow_api,
            workspace.id,
            &mut cache_reason,
            &mut cache_timestamp,
        ) {
            Some(projects) => projects,
            None => return,
        };

        let mut client_names =
            match self.resolve_client_names(&client, allow_api, workspace.id, &projects) {
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

        let missing_project_ids = missing_project_ids(&valid_entries, &projects);
        if allow_api && !missing_project_ids.is_empty() {
            let mut refreshed_projects: HashMap<u64, Project> = projects
                .iter()
                .cloned()
                .map(|project| (project.id, project))
                .collect();
            let mut cache_changed = false;

            match client.fetch_projects(workspace.id) {
                Ok(fresh_projects) => {
                    refreshed_projects = fresh_projects
                        .into_iter()
                        .map(|project| (project.id, project))
                        .collect();
                    cache_changed = true;
                }
                Err(err) => {
                    if matches!(err, TogglError::Unauthorized) {
                        self.handle_error(err);
                        return;
                    }
                }
            }

            let known_ids: HashSet<u64> = refreshed_projects.keys().copied().collect();
            for project_id in missing_project_ids {
                if known_ids.contains(&project_id) {
                    continue;
                }

                match client.fetch_project(workspace.id, project_id) {
                    Ok(project) => {
                        refreshed_projects.insert(project.id, project);
                        cache_changed = true;
                    }
                    Err(TogglError::Unauthorized) => {
                        self.handle_error(TogglError::Unauthorized);
                        return;
                    }
                    Err(_) => {}
                }
            }

            if cache_changed {
                let mut merged_projects: Vec<Project> = refreshed_projects.into_values().collect();
                merged_projects
                    .sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
                self.update_cache_projects(workspace.id, &merged_projects);
                projects = merged_projects;
                client_names =
                    match self.resolve_client_names(&client, allow_api, workspace.id, &projects) {
                        Some(names) => names,
                        None => return,
                    };
            }
        }

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
        self.rebuild_rollups();
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

        self.mode = resume_mode.unwrap_or(Mode::Dashboard);
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

    fn start_update(&mut self) {
        if self.update_resume_mode.is_none() {
            self.update_resume_mode = Some(self.mode);
        }
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
            KeyCode::Char('[') => self.shift_dashboard_date_range(-1),
            KeyCode::Char(']') => self.shift_dashboard_date_range(1),
            KeyCode::Char('h') => self.show_help = true,
            KeyCode::Char('m') | KeyCode::Char('M') => self.toggle_theme(),
            KeyCode::Char('s') => self.enter_settings(),
            KeyCode::Char('o') | KeyCode::Char('O') => self.enter_rollups(),
            KeyCode::Char('d') => self.enter_date_input(DateInputMode::Range),
            KeyCode::Char('u') | KeyCode::Char('U') => {
                if self.update_info.is_some() && self.update_installable {
                    self.start_update();
                } else if self.update_info.is_some() {
                    self.set_toast("Update via package manager.", false);
                } else if update::should_check_updates() {
                    self.set_toast("No update available.", false);
                } else {
                    self.set_toast("Updates are managed by your package manager.", false);
                }
            }
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
            KeyCode::Esc if self.dashboard_focus == DashboardFocus::Entries => {
                self.exit_entries_focus()
            }
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

    fn handle_rollups_input(&mut self, key: KeyEvent) {
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
            KeyCode::Esc => self.exit_rollups(),
            KeyCode::Char('h') => self.show_help = true,
            KeyCode::Char('w') | KeyCode::Char('W') => self.set_rollup_view(RollupView::Weekly),
            KeyCode::Char('m') | KeyCode::Char('M') => self.set_rollup_view(RollupView::Monthly),
            KeyCode::Char('y') | KeyCode::Char('Y') => self.set_rollup_view(RollupView::Yearly),
            KeyCode::Char('z') | KeyCode::Char('Z') => self.toggle_rollup_weekends(),
            KeyCode::Char('R')
                if key.modifiers.contains(KeyModifiers::SHIFT)
                    || key.modifiers == KeyModifiers::NONE =>
            {
                self.open_rollup_refetch_confirm();
            }
            KeyCode::Left => match self.rollup_focus {
                RollupFocus::Periods => self.select_previous_rollup_period(),
                RollupFocus::Days => self.select_previous_rollup_day(),
            },
            KeyCode::Right => match self.rollup_focus {
                RollupFocus::Periods => self.select_next_rollup_period(),
                RollupFocus::Days => self.select_next_rollup_day(),
            },
            KeyCode::Tab => self.toggle_rollup_focus(),
            KeyCode::Up => match self.rollup_focus {
                RollupFocus::Periods => self.select_previous_rollup_period(),
                RollupFocus::Days => {
                    if matches!(self.rollup_view, RollupView::Monthly | RollupView::Yearly) {
                        self.select_previous_rollup_day_by(self.rollup_vertical_step());
                    } else {
                        self.select_previous_rollup_day();
                    }
                }
            },
            KeyCode::Down => match self.rollup_focus {
                RollupFocus::Periods => self.select_next_rollup_period(),
                RollupFocus::Days => {
                    if matches!(self.rollup_view, RollupView::Monthly | RollupView::Yearly) {
                        self.select_next_rollup_day_by(self.rollup_vertical_step());
                    } else {
                        self.select_next_rollup_day();
                    }
                }
            },
            _ => {}
        }
    }

    fn handle_refetch_confirm_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.refetch_plan = None;
                self.mode = Mode::Rollups;
            }
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.execute_rollup_refetch();
            }
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
                    self.cache = self.token_hash.as_ref().and_then(|hash| {
                        storage::read_cache().filter(|cache| cache.token_hash == *hash)
                    });
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

    fn enter_rollups(&mut self) {
        self.rollup_focus = RollupFocus::Periods;
        self.ensure_rollup_selections();
        self.mode = Mode::Rollups;
        self.status = None;
    }

    fn exit_rollups(&mut self) {
        self.mode = Mode::Dashboard;
    }

    fn trigger_refresh(&mut self) {
        self.mode = Mode::Loading;
        self.refresh_intent = RefreshIntent::ForceApi;
        self.needs_refresh = true;
    }

    fn set_date_range(&mut self, range: DateRange) {
        self.set_date_range_with_resume(range, None);
    }

    fn set_date_range_with_resume(&mut self, range: DateRange, resume_mode: Option<Mode>) {
        self.date_range = range;
        self.mode = Mode::Loading;
        self.refresh_intent = RefreshIntent::CacheOnly;
        self.refresh_resume_mode = resume_mode;
        self.needs_refresh = true;
    }

    fn shift_dashboard_date_range(&mut self, direction: i32) {
        let direction = direction.signum();
        if direction == 0 {
            return;
        }
        let start = self.date_range.start_date();
        let end = self.date_range.end_date();
        let span_days = days_between(start, end) as i64;
        if span_days <= 0 {
            return;
        }
        let shift = chrono::Duration::days(span_days * direction as i64);
        let next_start = start + shift;
        let next_end = end + shift;
        self.set_date_range(DateRange::from_bounds(next_start, next_end));
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
                SettingsItem::Theme => {
                    self.cycle_theme(true);
                }
                SettingsItem::RollupsIncludeWeekends => {
                    self.settings_rollups_include_weekends_draft =
                        !self.settings_rollups_include_weekends_draft;
                }
                SettingsItem::RollupsWeekStart => {
                    self.cycle_rollup_week_start(true);
                }
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
                SettingsItem::Theme => {
                    self.cycle_theme(false);
                }
                SettingsItem::RollupsIncludeWeekends => {
                    self.settings_rollups_include_weekends_draft =
                        !self.settings_rollups_include_weekends_draft;
                }
                SettingsItem::RollupsWeekStart => {
                    self.cycle_rollup_week_start(false);
                }
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
                SettingsItem::Theme => {}
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
                SettingsItem::RollupsIncludeWeekends => {}
                SettingsItem::RollupsWeekStart => {}
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

        let parsed: f64 = value
            .parse()
            .map_err(|_| "Invalid number format.".to_string())?;
        if parsed <= 0.0 {
            return Err("Target hours must be greater than 0.".to_string());
        }

        Ok((parsed * 100.0).round() / 100.0)
    }

    fn rebuild_grouped(&mut self) {
        let selected_project_key = self
            .current_project()
            .map(|project| (project.client_name.clone(), project.project_name.clone()));
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
        self.rebuild_rollups();

        if let Some((client_name, project_name)) = selected_project_key {
            if let Some(index) = self.grouped.iter().position(|project| {
                project.client_name == client_name && project.project_name == project_name
            }) {
                self.project_state.select(Some(index));
            }
        }

        self.sync_entry_selection_for_project();

        if let Some(entry_desc) = selected_entry_key {
            if let Some(project) = self.current_project() {
                if let Some(index) = project
                    .entries
                    .iter()
                    .position(|entry| entry.description == entry_desc)
                {
                    self.entry_state.select(Some(index));
                }
            }
        }

        self.sync_entry_selection_for_project();
    }

    fn sync_settings_items_for_category(&mut self) {
        self.settings_items = match self.settings_selected_category() {
            "Integrations" => vec![SettingsItem::TogglToken],
            "Rollups" => vec![
                SettingsItem::RollupsIncludeWeekends,
                SettingsItem::RollupsWeekStart,
            ],
            _ => vec![
                SettingsItem::Theme,
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
            SettingsItem::Theme => {
                self.settings_theme_draft = self.theme;
            }
            SettingsItem::TargetHours => {
                self.settings_input = format!("{:.2}", self.target_hours);
            }
            SettingsItem::RollupsIncludeWeekends => {
                self.settings_rollups_include_weekends_draft = self.rollups_include_weekends;
            }
            SettingsItem::RollupsWeekStart => {
                self.settings_rollups_week_start_draft = self.rollups_week_start;
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
            SettingsItem::Theme => {
                let next = self.settings_theme_draft;
                if let Err(err) = storage::write_theme(next) {
                    self.status = Some(format!("Theme save failed: {err}"));
                    return;
                }
                self.theme = next;
                let label = theme_label(next);
                self.status = Some(format!("Theme set to {label}."));
                self.set_toast(format!("Theme set to {label}."), false);
                self.settings_edit_item = None;
                self.settings_focus = SettingsFocus::Items;
            }
            SettingsItem::RollupsIncludeWeekends | SettingsItem::RollupsWeekStart => {
                let next = RollupPreferences {
                    include_weekends: self.settings_rollups_include_weekends_draft,
                    week_start: self.settings_rollups_week_start_draft,
                };
                if let Err(err) = storage::write_rollup_preferences(next) {
                    self.status = Some(format!("Failed to save: {err}"));
                    return;
                }
                self.rollups_include_weekends = next.include_weekends;
                self.rollups_week_start = next.week_start;
                self.status = Some("Rollup defaults updated.".to_string());
                self.set_toast("Rollup defaults saved.", false);
                self.settings_edit_item = None;
                self.settings_focus = SettingsFocus::Items;
                self.rebuild_rollups();
            }
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
                    .and_then(|hash| {
                        storage::read_cache().filter(|cache| cache.token_hash == *hash)
                    })
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

    fn cycle_theme(&mut self, up: bool) {
        let values = [
            ThemePreference::Terminal,
            ThemePreference::Dark,
            ThemePreference::Light,
        ];
        let current = self.settings_theme_draft;
        let index = values
            .iter()
            .position(|value| *value == current)
            .unwrap_or(0);
        let next_index = if up {
            if index == 0 {
                values.len() - 1
            } else {
                index - 1
            }
        } else {
            if index + 1 >= values.len() {
                0
            } else {
                index + 1
            }
        };
        self.settings_theme_draft = values[next_index];
    }

    fn cycle_rollup_week_start(&mut self, up: bool) {
        self.settings_rollups_week_start_draft = match (self.settings_rollups_week_start_draft, up)
        {
            (WeekStart::Monday, true) => WeekStart::Sunday,
            (WeekStart::Sunday, true) => WeekStart::Monday,
            (WeekStart::Monday, false) => WeekStart::Sunday,
            (WeekStart::Sunday, false) => WeekStart::Monday,
        };
    }

    fn cycle_rounding_increment(&mut self, up: bool) {
        let values = [15u32, 30, 45, 60];
        let current = self.settings_rounding_draft.increment_minutes;
        let index = values
            .iter()
            .position(|value| *value == current)
            .unwrap_or(0);
        let next_index = if up {
            if index == 0 {
                values.len() - 1
            } else {
                index - 1
            }
        } else {
            if index + 1 >= values.len() {
                0
            } else {
                index + 1
            }
        };
        self.settings_rounding_draft.increment_minutes = values[next_index];
    }

    fn cycle_rounding_mode(&mut self, up: bool) {
        let values = [RoundingMode::Closest, RoundingMode::Up, RoundingMode::Down];
        let current = self.settings_rounding_draft.mode;
        let index = values
            .iter()
            .position(|value| *value == current)
            .unwrap_or(0);
        let next_index = if up {
            if index == 0 {
                values.len() - 1
            } else {
                index - 1
            }
        } else {
            if index + 1 >= values.len() {
                0
            } else {
                index + 1
            }
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

    pub fn settings_theme_display(&self) -> ThemePreference {
        if self.settings_focus == SettingsFocus::Edit
            && self.settings_edit_item == Some(SettingsItem::Theme)
        {
            self.settings_theme_draft
        } else {
            self.theme
        }
    }

    pub fn settings_rollups_include_weekends_display(&self) -> bool {
        if self.settings_focus == SettingsFocus::Edit
            && self.settings_edit_item == Some(SettingsItem::RollupsIncludeWeekends)
        {
            self.settings_rollups_include_weekends_draft
        } else {
            self.rollups_include_weekends
        }
    }

    pub fn settings_rollups_week_start_display(&self) -> WeekStart {
        if self.settings_focus == SettingsFocus::Edit
            && self.settings_edit_item == Some(SettingsItem::RollupsWeekStart)
        {
            self.settings_rollups_week_start_draft
        } else {
            self.rollups_week_start
        }
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

    fn rebuild_rollups(&mut self) {
        let (rollup_start, rollup_end) = self.rollup_bounds();
        let Some(workspace_id) = self
            .selected_workspace
            .as_ref()
            .map(|workspace| workspace.id)
        else {
            self.rollups = build_rollups(
                &self.time_entries,
                rollup_start,
                rollup_end,
                self.rounding.as_ref(),
                self.rollups_week_start,
            );
            self.align_rollup_selection_to_active_range();
            self.align_rollup_day_selection_to_active_range();
            self.ensure_rollup_selections();
            return;
        };

        let mut entries_by_id: HashMap<u64, TimeEntry> = HashMap::new();
        for entry in self.collect_cached_entries_for_range(workspace_id, rollup_start, rollup_end) {
            if entry.stop.is_some() {
                entries_by_id.insert(entry.id, entry);
            }
        }
        for entry in &self.time_entries {
            if entry.stop.is_some() {
                entries_by_id.insert(entry.id, entry.clone());
            }
        }

        let mut rollup_entries: Vec<TimeEntry> = entries_by_id.into_values().collect();
        rollup_entries
            .sort_by(|left, right| left.start.cmp(&right.start).then(left.id.cmp(&right.id)));

        self.rollups = build_rollups(
            &rollup_entries,
            rollup_start,
            rollup_end,
            self.rounding.as_ref(),
            self.rollups_week_start,
        );
        self.align_rollup_selection_to_active_range();
        self.align_rollup_day_selection_to_active_range();
        self.ensure_rollup_selections();
    }

    fn rollup_bounds(&self) -> (NaiveDate, NaiveDate) {
        let start = self.date_range.start_date();
        let end = self.date_range.end_date();
        match self.rollup_view {
            RollupView::Yearly => (year_start(start), year_end(end)),
            RollupView::Weekly | RollupView::Monthly => (month_start(start), month_end(end)),
        }
    }

    fn align_rollup_selection_to_active_range(&mut self) {
        let target_day = self.date_range.end_date();

        if let Some(index) = self
            .rollups
            .weekly
            .iter()
            .position(|period| period.start <= target_day && period.end >= target_day)
        {
            self.rollup_week_state.select(Some(index));
        }

        if let Some(index) = self
            .rollups
            .monthly
            .iter()
            .position(|period| period.start <= target_day && period.end >= target_day)
        {
            self.rollup_month_state.select(Some(index));
        }

        if let Some(index) = self
            .rollups
            .yearly
            .iter()
            .position(|period| period.start <= target_day && period.end >= target_day)
        {
            self.rollup_year_state.select(Some(index));
        }
    }

    fn align_rollup_day_selection_to_active_range(&mut self) {
        let target_day = self.date_range.end_date();
        let daily = self.rollup_daily_for_selected_period();
        if daily.is_empty() {
            self.rollup_day_state.select(None);
            return;
        }
        if let Some(index) = daily.iter().position(|day| day.date == target_day) {
            self.rollup_day_state.select(Some(index));
            return;
        }
        let fallback = daily
            .iter()
            .rposition(|day| day.date <= target_day)
            .unwrap_or(0);
        self.rollup_day_state.select(Some(fallback));
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

    fn set_rollup_view(&mut self, view: RollupView) {
        if self.rollup_view == view {
            return;
        }
        self.rollup_view = view;
        self.rebuild_rollups();
        self.ensure_rollup_selections();
    }

    fn toggle_rollup_focus(&mut self) {
        self.rollup_focus = match self.rollup_focus {
            RollupFocus::Periods => RollupFocus::Days,
            RollupFocus::Days => RollupFocus::Periods,
        };
    }

    fn toggle_rollup_weekends(&mut self) {
        self.rollups_include_weekends = !self.rollups_include_weekends;
        self.align_rollup_day_selection_to_active_range();
        self.ensure_rollup_day_selection();
        if self.rollups_include_weekends {
            self.status = Some("Rollups: weekends included.".to_string());
        } else {
            self.status = Some("Rollups: weekends excluded.".to_string());
        }
    }

    fn open_rollup_refetch_confirm(&mut self) {
        let Some(plan) = self.selected_rollup_refetch_plan() else {
            self.status = Some("Select a rollup period/day first.".to_string());
            self.set_toast("Select a rollup period/day first.", true);
            return;
        };
        self.refetch_plan = Some(plan);
        self.mode = Mode::RefetchConfirm;
    }

    fn selected_rollup_refetch_plan(&self) -> Option<RefetchPlan> {
        if self.rollup_focus == RollupFocus::Days {
            let daily = self.rollup_daily_for_selected_period();
            if let Some(index) = self.rollup_day_state.selected() {
                if let Some(day) = daily.get(index) {
                    return Some(RefetchPlan {
                        start: day.date,
                        end: day.date,
                        scope_label: format!("Day {}", day.date.format("%Y-%m-%d")),
                    });
                }
            }
        }

        let period = self.rollup_selected_period()?;
        let scope = match self.rollup_view {
            RollupView::Weekly => "Week",
            RollupView::Monthly => "Month",
            RollupView::Yearly => "Year",
        };
        Some(RefetchPlan {
            start: period.start,
            end: period.end,
            scope_label: format!("{scope} {}", period.label),
        })
    }

    fn execute_rollup_refetch(&mut self) {
        let Some(plan) = self.refetch_plan.clone() else {
            self.mode = Mode::Rollups;
            return;
        };

        let token = match self.token.clone() {
            Some(token) => token,
            None => {
                self.refetch_plan = None;
                self.mode = Mode::Login;
                return;
            }
        };
        let workspace_id = match self
            .selected_workspace
            .as_ref()
            .map(|workspace| workspace.id)
        {
            Some(id) => id,
            None => {
                self.refetch_plan = None;
                self.mode = Mode::WorkspaceSelect;
                return;
            }
        };

        self.ensure_quota_today();
        let client = TogglClient::new(token);
        let dates = date_span(plan.start, plan.end);
        let total_days = dates.len();
        let mut fetched_days: Vec<NaiveDate> = Vec::new();
        let mut stop_reason: Option<String> = None;

        for (index, day) in dates.iter().enumerate() {
            if self.quota_remaining() == 0 {
                stop_reason = Some("local quota reached".to_string());
                if index < total_days {
                    break;
                }
            }

            self.consume_quota();
            let (start_rfc, end_rfc) = DateRange::from_bounds(*day, *day).as_rfc3339();
            match client.fetch_time_entries(&start_rfc, &end_rfc) {
                Ok(entries) => {
                    self.update_cache_time_entries(workspace_id, &start_rfc, &end_rfc, &entries);
                    fetched_days.push(*day);
                }
                Err(TogglError::Unauthorized) => {
                    self.refetch_plan = None;
                    self.handle_error(TogglError::Unauthorized);
                    return;
                }
                Err(TogglError::PaymentRequired) => {
                    stop_reason = Some("Toggl returned 402 Payment Required".to_string());
                    break;
                }
                Err(TogglError::RateLimited) => {
                    stop_reason = Some("Toggl rate limit reached".to_string());
                    break;
                }
                Err(TogglError::ServerError(message)) | Err(TogglError::Network(message)) => {
                    stop_reason = Some(message);
                    break;
                }
            }
        }

        self.refetch_plan = None;
        self.mode = Mode::Loading;
        self.refresh_resume_mode = Some(Mode::Rollups);
        self.refresh_intent = RefreshIntent::CacheOnly;
        self.needs_refresh = true;

        if fetched_days.len() == total_days {
            let message = format!("Refetched {} day(s) for {}.", total_days, plan.scope_label);
            self.status = Some(message.clone());
            self.set_toast(message, false);
            return;
        }

        let skipped_days = if fetched_days.len() < total_days {
            dates
                .iter()
                .skip(fetched_days.len())
                .copied()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let reason = stop_reason.unwrap_or_else(|| "fetch interrupted".to_string());
        let message = format!(
            "Partial refetch {}/{} day(s). Cached: {}. Stopped: {}. Skipped: {}.",
            fetched_days.len(),
            total_days,
            format_day_spans(&fetched_days),
            reason,
            format_day_spans(&skipped_days)
        );
        self.status = Some(message.clone());
        self.set_toast(message, true);
    }

    fn rollup_periods(&self) -> &[PeriodRollup] {
        match self.rollup_view {
            RollupView::Weekly => &self.rollups.weekly,
            RollupView::Monthly => &self.rollups.monthly,
            RollupView::Yearly => &self.rollups.yearly,
        }
    }

    fn rollup_periods_for_view(&self, view: RollupView) -> &[PeriodRollup] {
        match view {
            RollupView::Weekly => &self.rollups.weekly,
            RollupView::Monthly => &self.rollups.monthly,
            RollupView::Yearly => &self.rollups.yearly,
        }
    }

    fn rollup_state_for_view_mut(&mut self, view: RollupView) -> &mut ListState {
        match view {
            RollupView::Weekly => &mut self.rollup_week_state,
            RollupView::Monthly => &mut self.rollup_month_state,
            RollupView::Yearly => &mut self.rollup_year_state,
        }
    }

    fn rollup_state_for_view(&self, view: RollupView) -> &ListState {
        match view {
            RollupView::Weekly => &self.rollup_week_state,
            RollupView::Monthly => &self.rollup_month_state,
            RollupView::Yearly => &self.rollup_year_state,
        }
    }

    pub fn rollup_selected_period(&self) -> Option<&PeriodRollup> {
        let index = self.rollup_state_for_view(self.rollup_view).selected()?;
        self.rollup_periods().get(index)
    }

    pub fn non_working_days(&self) -> &HashSet<NaiveDate> {
        &self.non_working_days
    }

    pub fn is_non_working_day(&self, day: NaiveDate) -> bool {
        self.non_working_days.contains(&day)
    }

    pub fn rollup_daily_for_selected_period(&self) -> Vec<&DailyTotal> {
        let Some(period) = self.rollup_selected_period() else {
            return Vec::new();
        };
        self.rollups
            .daily
            .iter()
            .filter(|day| {
                day.date >= period.start
                    && day.date <= period.end
                    && self.is_rollup_day_included(day.date)
            })
            .collect()
    }

    fn ensure_rollup_state_for_view(&mut self, view: RollupView) {
        let len = self.rollup_periods_for_view(view).len();
        let state = self.rollup_state_for_view_mut(view);
        if len == 0 {
            state.select(None);
            return;
        }
        let selected = state.selected().unwrap_or(0).min(len - 1);
        state.select(Some(selected));
    }

    fn ensure_rollup_day_selection(&mut self) {
        let count = self.rollup_days_len();
        if count == 0 {
            self.rollup_day_state.select(None);
            return;
        }
        let selected = self.rollup_day_state.selected().unwrap_or(0);
        self.rollup_day_state.select(Some(selected.min(count - 1)));
    }

    fn ensure_rollup_selections(&mut self) {
        self.ensure_rollup_state_for_view(RollupView::Weekly);
        self.ensure_rollup_state_for_view(RollupView::Monthly);
        self.ensure_rollup_state_for_view(RollupView::Yearly);
        self.ensure_rollup_day_selection();
    }

    fn reset_rollup_day_selection(&mut self) {
        let count = self.rollup_days_len();
        if count == 0 {
            self.rollup_day_state.select(None);
        } else {
            self.rollup_day_state.select(Some(0));
        }
    }

    fn select_previous_rollup_period(&mut self) {
        let len = self.rollup_periods().len();
        if len == 0 {
            return;
        }
        let state = self.rollup_state_for_view_mut(self.rollup_view);
        let selected = state.selected().unwrap_or(0);
        let new_index = if selected == 0 { len - 1 } else { selected - 1 };
        state.select(Some(new_index));
        self.reset_rollup_day_selection();
    }

    fn select_next_rollup_period(&mut self) {
        let len = self.rollup_periods().len();
        if len == 0 {
            return;
        }
        let state = self.rollup_state_for_view_mut(self.rollup_view);
        let selected = state.selected().unwrap_or(0);
        let new_index = if selected + 1 >= len { 0 } else { selected + 1 };
        state.select(Some(new_index));
        self.reset_rollup_day_selection();
    }

    fn rollup_days_len(&self) -> usize {
        self.rollup_daily_for_selected_period().len()
    }

    fn rollup_vertical_step(&self) -> usize {
        if self.rollups_include_weekends { 7 } else { 5 }
    }

    fn is_rollup_day_included(&self, day: NaiveDate) -> bool {
        if self.rollups_include_weekends {
            true
        } else {
            day.weekday().number_from_monday() <= 5
        }
    }

    fn select_previous_rollup_day(&mut self) {
        let count = self.rollup_days_len();
        if count == 0 {
            self.rollup_day_state.select(None);
            return;
        }
        let selected = self.rollup_day_state.selected().unwrap_or(0);
        let new_index = if selected == 0 {
            count - 1
        } else {
            selected - 1
        };
        self.rollup_day_state.select(Some(new_index));
    }

    fn select_previous_rollup_day_by(&mut self, step: usize) {
        let count = self.rollup_days_len();
        if count == 0 {
            self.rollup_day_state.select(None);
            return;
        }
        let step = step % count;
        if step == 0 {
            return;
        }
        let selected = self.rollup_day_state.selected().unwrap_or(0);
        let new_index = if selected >= step {
            selected - step
        } else {
            count + selected - step
        };
        self.rollup_day_state.select(Some(new_index));
    }

    fn select_next_rollup_day(&mut self) {
        let count = self.rollup_days_len();
        if count == 0 {
            self.rollup_day_state.select(None);
            return;
        }
        let selected = self.rollup_day_state.selected().unwrap_or(0);
        let new_index = if selected + 1 >= count {
            0
        } else {
            selected + 1
        };
        self.rollup_day_state.select(Some(new_index));
    }

    fn select_next_rollup_day_by(&mut self, step: usize) {
        let count = self.rollup_days_len();
        if count == 0 {
            self.rollup_day_state.select(None);
            return;
        }
        let step = step % count;
        if step == 0 {
            return;
        }
        let selected = self.rollup_day_state.selected().unwrap_or(0);
        let new_index = (selected + step) % count;
        self.rollup_day_state.select(Some(new_index));
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

        self.write_clipboard(
            format!("{:.2}", selected.total_hours),
            "Copied entry hours.",
        );
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
                        lines.push(format!(
                            "• {} ({:.2}h)",
                            entry.description, entry.total_hours
                        ));
                    }
                }
            }
        } else {
            for entry in &selected.entries {
                lines.push(format!(
                    "• {} ({:.2}h)",
                    entry.description, entry.total_hours
                ));
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
        match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(text)) {
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
                    lines.push(format!(
                        "• {} ({:.2}h)",
                        entry.description, entry.total_hours
                    ));
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
        self.cache
            .as_ref()
            .and_then(|cache| cache.workspaces.clone())
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

    fn cached_time_entries_for_range(
        &self,
        workspace_id: u64,
        start: &str,
        end: &str,
    ) -> Option<CachedData<Vec<TimeEntry>>> {
        if let Some(cached) = self.cached_time_entries(workspace_id, start, end) {
            return Some(cached);
        }

        let start_date = parse_rfc3339_date(start)?;
        let end_date = parse_rfc3339_date(end)?;
        let cache = self.cache.as_ref()?;

        let mut entries_by_id: HashMap<u64, TimeEntry> = HashMap::new();
        let mut latest_cached_at: Option<DateTime<Local>> = None;
        let mut latest_cached_raw: Option<String> = None;

        for (key, cached) in &cache.time_entries {
            let Some((cached_workspace, cached_start, cached_end)) = parse_cache_key_bounds(key)
            else {
                continue;
            };
            if cached_workspace != workspace_id
                || cached_end < start_date
                || cached_start > end_date
            {
                continue;
            }

            for entry in &cached.data {
                let Some(entry_date) = parse_entry_date(entry) else {
                    continue;
                };
                if entry_date >= start_date && entry_date <= end_date {
                    entries_by_id.insert(entry.id, entry.clone());
                }
            }

            if let Some(cached_at) = parse_cached_time(&cached.fetched_at) {
                match latest_cached_at {
                    Some(current) if cached_at <= current => {}
                    _ => {
                        latest_cached_at = Some(cached_at);
                    }
                }
            } else {
                latest_cached_raw = Some(cached.fetched_at.clone());
            }
        }

        if entries_by_id.is_empty() {
            return None;
        }

        let mut entries: Vec<TimeEntry> = entries_by_id.into_values().collect();
        entries.sort_by(|left, right| left.start.cmp(&right.start).then(left.id.cmp(&right.id)));

        let fetched_at = latest_cached_at
            .map(|dt| dt.to_rfc3339())
            .or(latest_cached_raw)
            .unwrap_or_else(storage::now_rfc3339);

        Some(CachedData {
            data: entries,
            fetched_at,
        })
    }

    fn collect_cached_entries_for_range(
        &self,
        workspace_id: u64,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Vec<TimeEntry> {
        let Some(cache) = self.cache.as_ref() else {
            return Vec::new();
        };

        let mut entries = Vec::new();
        for (key, cached) in &cache.time_entries {
            let Some((cached_workspace, cached_start, cached_end)) = parse_cache_key_bounds(key)
            else {
                continue;
            };
            if cached_workspace != workspace_id || cached_end < start || cached_start > end {
                continue;
            }

            for entry in &cached.data {
                let Some(entry_date) = parse_entry_date(entry) else {
                    continue;
                };
                if entry_date >= start && entry_date <= end {
                    entries.push(entry.clone());
                }
            }
        }

        entries
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

        match client.fetch_workspaces() {
            Ok(workspaces) => {
                self.update_cache_workspaces(&workspaces);
                Some(workspaces)
            }
            Err(err) => {
                self.handle_error(err);
                None
            }
        }
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

        match client.fetch_projects(workspace_id) {
            Ok(projects) => {
                self.update_cache_projects(workspace_id, &projects);
                Some(projects)
            }
            Err(err) => {
                self.handle_error(err);
                None
            }
        }
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

        if let Some(cached) = self.cached_time_entries_for_range(workspace_id, start, end) {
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
        format!("{} No cached data available.", self.quota_message())
    }

    fn cache_status_message(&self, reason: CacheReason, cached_at: Option<&str>) -> String {
        let updated = cached_at
            .and_then(|value| parse_cached_time(value))
            .map(|value| format!(" (last updated {})", value.format("%Y-%m-%d %H:%M")))
            .unwrap_or_default();
        match reason {
            CacheReason::CacheOnly => format!("Using cached data{updated}."),
            CacheReason::Quota => format!("{} Using cached data{updated}.", self.quota_message()),
            CacheReason::ApiError => format!("Using cached data due to API error{updated}."),
        }
    }

    fn quota_message(&self) -> String {
        let remaining = self.quota_remaining();
        if remaining == 0 {
            format!("Quota reached ({}/{}).", self.quota.used_calls, CALL_LIMIT)
        } else {
            format!("Quota low (remaining {}/{}).", remaining, CALL_LIMIT)
        }
    }
}

fn parse_cached_time(value: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
}

fn parse_rfc3339_date(value: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Local).date_naive())
}

fn parse_entry_date(entry: &TimeEntry) -> Option<NaiveDate> {
    parse_rfc3339_date(&entry.start)
}

fn missing_project_ids(entries: &[TimeEntry], projects: &[Project]) -> HashSet<u64> {
    let known_ids: HashSet<u64> = projects.iter().map(|project| project.id).collect();
    entries
        .iter()
        .filter_map(|entry| entry.project_id)
        .filter(|project_id| !known_ids.contains(project_id))
        .collect()
}

fn parse_cache_key_bounds(key: &str) -> Option<(u64, NaiveDate, NaiveDate)> {
    let mut parts = key.splitn(3, '|');
    let workspace_id = parts.next()?.parse::<u64>().ok()?;
    let start = parse_rfc3339_date(parts.next()?)?;
    let end = parse_rfc3339_date(parts.next()?)?;
    Some((workspace_id, start, end))
}

fn month_start(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap_or(date)
}

fn year_start(date: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap_or(date)
}

fn year_end(date: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), 12, 31).unwrap_or(date)
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

fn days_between(start: NaiveDate, end: NaiveDate) -> usize {
    if end < start {
        0
    } else {
        (end - start).num_days() as usize + 1
    }
}

fn date_span(start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
    let mut days = Vec::new();
    let mut current = start;
    while current <= end {
        days.push(current);
        current = current
            .succ_opt()
            .unwrap_or(current + chrono::Duration::days(1));
    }
    days
}

fn format_day_spans(days: &[NaiveDate]) -> String {
    if days.is_empty() {
        return "none".to_string();
    }

    let mut sorted = days.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let mut spans: Vec<String> = Vec::new();
    let mut range_start = sorted[0];
    let mut range_end = sorted[0];

    for day in sorted.iter().skip(1).copied() {
        let expected_next = range_end
            .succ_opt()
            .unwrap_or(range_end + chrono::Duration::days(1));
        if day == expected_next {
            range_end = day;
            continue;
        }
        spans.push(format_date_span(range_start, range_end));
        range_start = day;
        range_end = day;
    }
    spans.push(format_date_span(range_start, range_end));
    spans.join(", ")
}

fn format_date_span(start: NaiveDate, end: NaiveDate) -> String {
    if start == end {
        start.format("%Y-%m-%d").to_string()
    } else {
        format!("{}→{}", start.format("%Y-%m-%d"), end.format("%Y-%m-%d"))
    }
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

pub struct RefetchPlanView {
    pub scope_label: String,
    pub start: String,
    pub end: String,
    pub days: usize,
    pub estimated_calls: u32,
    pub remaining_calls: u32,
}
