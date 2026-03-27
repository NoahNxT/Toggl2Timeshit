use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use crate::models::{Client as TogglClientModel, Project, TimeEntry, Workspace};
use crate::rollups::WeekStart;
use crate::rounding::RoundingConfig;
use crate::theme::{
    CustomTheme, ThemePalette, ThemePreference, ThemeSelection, find_custom_theme,
    sorted_custom_themes, validate_theme_name,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedData<T> {
    pub data: T,
    pub fetched_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CacheFile {
    pub version: u32,
    pub token_hash: String,
    #[serde(default)]
    pub workspaces: Option<CachedData<Vec<Workspace>>>,
    #[serde(default)]
    pub projects: HashMap<u64, CachedData<Vec<Project>>>,
    #[serde(default)]
    pub clients: HashMap<u64, CachedData<Vec<TogglClientModel>>>,
    #[serde(default)]
    pub time_entries: HashMap<String, CachedData<Vec<TimeEntry>>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QuotaFile {
    #[serde(default)]
    pub version: u32,
    pub date: String,
    pub used_calls: u32,
}

const QUOTA_FILE_VERSION: u32 = 2;

pub fn read_token() -> Option<String> {
    if let Ok(value) = env::var("TOGGL_API_TOKEN") {
        if !value.trim().is_empty() {
            return Some(value);
        }
    }

    let path = token_path()?;
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

pub fn write_token(token: &str) -> Result<(), io::Error> {
    let path = token_path()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    fs::write(path, token)
}

fn token_path() -> Option<PathBuf> {
    let mut path = dirs::home_dir()?;
    path.push(".toggl2tsc");
    Some(path)
}

#[derive(Debug, Clone)]
pub struct ThemeSettings {
    pub active_theme: ThemeSelection,
    pub custom_themes: Vec<CustomTheme>,
}

#[derive(Debug, Clone)]
pub struct ThemeDraft {
    pub id: Option<String>,
    pub name: String,
    pub palette: ThemePalette,
}

#[derive(Debug)]
pub enum ThemeConfigError {
    Io(io::Error),
    Validation(String),
}

impl std::fmt::Display for ThemeConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Validation(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ThemeConfigError {}

impl From<io::Error> for ThemeConfigError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Config {
    theme: Option<ThemePreference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_theme: Option<ThemeSelection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    custom_themes: Vec<CustomTheme>,
    target_hours: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rounding: Option<RoundingConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rollup_preferences: Option<RollupPreferences>,
    // Backward-compatible legacy field; merged into vacation_days on read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    non_working_days: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    vacation_days: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sick_days: Vec<String>,
    // Legacy field; used as fallback for both target and credit hours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vacation_day_hours: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vacation_day_target_hours: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vacation_day_credit_hours: Option<f64>,
    // Legacy field; used as fallback for both target and credit hours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sick_day_hours: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sick_day_target_hours: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sick_day_credit_hours: Option<f64>,
    // Legacy field; used as fallback for both specific toggles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    credit_special_days_as_worked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    credit_vacation_days_as_worked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    credit_sick_days_as_worked: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RollupPreferences {
    #[serde(default = "default_rollup_include_weekends")]
    pub include_weekends: bool,
    #[serde(default)]
    pub week_start: WeekStart,
}

impl Default for RollupPreferences {
    fn default() -> Self {
        Self {
            include_weekends: default_rollup_include_weekends(),
            week_start: WeekStart::Monday,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let mut path = dirs::home_dir()?;
    path.push(".toggl2tsc.json");
    Some(path)
}

pub fn read_theme_settings() -> ThemeSettings {
    read_config()
        .map(|config| theme_settings_from_config(&config))
        .unwrap_or_else(|| ThemeSettings {
            active_theme: ThemeSelection::builtin(ThemePreference::Terminal),
            custom_themes: Vec::new(),
        })
}

pub fn write_theme_selection(
    selection: &ThemeSelection,
) -> Result<ThemeSettings, ThemeConfigError> {
    let mut config = read_config().unwrap_or_default();
    let custom_themes = normalize_custom_themes(&config.custom_themes);
    match selection {
        ThemeSelection::Builtin { theme } => {
            config.theme = Some(*theme);
        }
        ThemeSelection::Custom { id } => {
            if find_custom_theme(&custom_themes, id).is_none() {
                return Err(ThemeConfigError::Validation(
                    "Selected custom theme was not found.".to_string(),
                ));
            }
        }
    }
    config.active_theme = Some(selection.clone());
    config.custom_themes = custom_themes;
    write_config(&config)?;
    Ok(theme_settings_from_config(&config))
}

pub fn save_custom_theme(draft: ThemeDraft) -> Result<ThemeSettings, ThemeConfigError> {
    let mut config = read_config().unwrap_or_default();
    let mut custom_themes = normalize_custom_themes(&config.custom_themes);
    let normalized_name = validate_theme_name(&draft.name).map_err(ThemeConfigError::Validation)?;
    let normalized_palette = draft
        .palette
        .normalized()
        .map_err(ThemeConfigError::Validation)?;
    let now = now_rfc3339();

    if let Some(id) = draft.id.as_deref() {
        let Some(index) = custom_themes.iter().position(|theme| theme.id == id) else {
            return Err(ThemeConfigError::Validation(
                "Custom theme to update was not found.".to_string(),
            ));
        };
        if has_duplicate_custom_name(&custom_themes, &normalized_name, Some(id)) {
            return Err(ThemeConfigError::Validation(
                "Theme name must be unique.".to_string(),
            ));
        }
        let created_at = custom_themes[index].created_at.clone();
        custom_themes[index] = CustomTheme {
            id: id.to_string(),
            name: normalized_name,
            palette: normalized_palette,
            created_at,
            updated_at: now,
        };
    } else {
        if has_duplicate_custom_name(&custom_themes, &normalized_name, None) {
            return Err(ThemeConfigError::Validation(
                "Theme name must be unique.".to_string(),
            ));
        }
        let id = generate_custom_theme_id(&normalized_name, &custom_themes);
        custom_themes.push(CustomTheme {
            id,
            name: normalized_name,
            palette: normalized_palette,
            created_at: now.clone(),
            updated_at: now,
        });
    }

    config.custom_themes = custom_themes;
    write_config(&config)?;
    Ok(theme_settings_from_config(&config))
}

pub fn delete_custom_theme(id: &str) -> Result<ThemeSettings, ThemeConfigError> {
    let mut config = read_config().unwrap_or_default();
    let mut custom_themes = normalize_custom_themes(&config.custom_themes);
    let previous_len = custom_themes.len();
    custom_themes.retain(|theme| theme.id != id);
    if custom_themes.len() == previous_len {
        return Err(ThemeConfigError::Validation(
            "Custom theme to delete was not found.".to_string(),
        ));
    }

    if matches!(config.active_theme, Some(ThemeSelection::Custom { id: ref active_id }) if active_id == id)
    {
        config.active_theme = Some(ThemeSelection::builtin(
            config.theme.unwrap_or(ThemePreference::Terminal),
        ));
    }

    config.custom_themes = custom_themes;
    write_config(&config)?;
    Ok(theme_settings_from_config(&config))
}

fn theme_settings_from_config(config: &Config) -> ThemeSettings {
    let custom_themes = normalize_custom_themes(&config.custom_themes);
    let fallback_theme = config.theme.unwrap_or(ThemePreference::Terminal);
    let active_theme =
        resolve_active_theme(config.active_theme.as_ref(), fallback_theme, &custom_themes);

    ThemeSettings {
        active_theme,
        custom_themes,
    }
}

fn resolve_active_theme(
    active_theme: Option<&ThemeSelection>,
    fallback_theme: ThemePreference,
    custom_themes: &[CustomTheme],
) -> ThemeSelection {
    match active_theme {
        Some(ThemeSelection::Builtin { theme }) => ThemeSelection::builtin(*theme),
        Some(ThemeSelection::Custom { id }) if find_custom_theme(custom_themes, id).is_some() => {
            ThemeSelection::custom(id.to_string())
        }
        _ => ThemeSelection::builtin(fallback_theme),
    }
}

fn normalize_custom_themes(custom_themes: &[CustomTheme]) -> Vec<CustomTheme> {
    let mut normalized = Vec::new();
    let mut names = HashSet::new();

    for theme in custom_themes {
        let Some(next_theme) = normalize_custom_theme(theme) else {
            continue;
        };
        let key = next_theme.name.to_ascii_lowercase();
        if !names.insert(key) {
            continue;
        }
        normalized.push(next_theme);
    }

    sorted_custom_themes(&normalized)
}

fn normalize_custom_theme(theme: &CustomTheme) -> Option<CustomTheme> {
    if theme.id.trim().is_empty() {
        return None;
    }
    let name = validate_theme_name(&theme.name).ok()?;
    let palette = theme.palette.normalized().ok()?;

    Some(CustomTheme {
        id: theme.id.trim().to_string(),
        name,
        palette,
        created_at: theme.created_at.clone(),
        updated_at: theme.updated_at.clone(),
    })
}

fn has_duplicate_custom_name(
    custom_themes: &[CustomTheme],
    name: &str,
    ignore_id: Option<&str>,
) -> bool {
    let needle = name.to_ascii_lowercase();
    custom_themes.iter().any(|theme| {
        if ignore_id.is_some() && ignore_id == Some(theme.id.as_str()) {
            return false;
        }
        theme.name.to_ascii_lowercase() == needle
    })
}

fn generate_custom_theme_id(name: &str, custom_themes: &[CustomTheme]) -> String {
    for attempt in 0..100 {
        let seed = format!("{name}|{}|{attempt}", now_rfc3339());
        let id = format!("theme-{}", &hash_token(&seed)[..12]);
        if find_custom_theme(custom_themes, &id).is_none() {
            return id;
        }
    }

    format!("theme-{}", &hash_token(&format!("{name}|fallback"))[..12])
}

pub fn read_target_hours() -> Option<f64> {
    read_config().and_then(|config| config.target_hours)
}

pub fn write_target_hours(value: f64) -> Result<(), io::Error> {
    let mut config = read_config().unwrap_or_default();
    config.target_hours = Some(value);
    write_config(&config)
}

pub fn read_rounding() -> Option<RoundingConfig> {
    read_config().and_then(|config| config.rounding)
}

pub fn write_rounding(value: Option<RoundingConfig>) -> Result<(), io::Error> {
    let mut config = read_config().unwrap_or_default();
    config.rounding = value;
    write_config(&config)
}

pub fn read_rollup_preferences() -> RollupPreferences {
    read_config()
        .and_then(|config| config.rollup_preferences)
        .unwrap_or_default()
}

pub fn write_rollup_preferences(value: RollupPreferences) -> Result<(), io::Error> {
    let mut config = read_config().unwrap_or_default();
    config.rollup_preferences = Some(value);
    write_config(&config)
}

#[derive(Debug, Clone, Default)]
pub struct SpecialDays {
    pub vacation_days: HashSet<NaiveDate>,
    pub sick_days: HashSet<NaiveDate>,
}

pub fn read_special_days() -> SpecialDays {
    let Some(config) = read_config() else {
        return SpecialDays::default();
    };

    let mut vacation_days = parse_day_list(&config.vacation_days);
    // Migrate legacy "non_working_days" to vacation days.
    vacation_days.extend(parse_day_list(&config.non_working_days));
    let sick_days = parse_day_list(&config.sick_days);

    for day in &sick_days {
        vacation_days.remove(day);
    }

    SpecialDays {
        vacation_days,
        sick_days,
    }
}

pub fn write_special_days(
    vacation_days: &HashSet<NaiveDate>,
    sick_days: &HashSet<NaiveDate>,
) -> Result<(), io::Error> {
    let mut config = read_config().unwrap_or_default();
    config.vacation_days = format_day_list(vacation_days);
    config.sick_days = format_day_list(sick_days);
    // Keep legacy field in sync for backward compatibility.
    config.non_working_days = config.vacation_days.clone();
    write_config(&config)
}

fn config_vacation_day_target_hours(config: &Config) -> Option<f64> {
    config.vacation_day_target_hours.or(config.target_hours)
}

fn config_vacation_day_credit_hours(config: &Config) -> Option<f64> {
    config
        .vacation_day_credit_hours
        .or(config.vacation_day_hours)
}

fn config_sick_day_target_hours(config: &Config) -> Option<f64> {
    config.sick_day_target_hours.or(config.target_hours)
}

fn config_sick_day_credit_hours(config: &Config) -> Option<f64> {
    config.sick_day_credit_hours.or(config.sick_day_hours)
}

pub fn read_vacation_day_target_hours() -> Option<f64> {
    read_config().and_then(|config| config_vacation_day_target_hours(&config))
}

pub fn write_vacation_day_target_hours(value: f64) -> Result<(), io::Error> {
    let mut config = read_config().unwrap_or_default();
    config.vacation_day_target_hours = Some(value);
    write_config(&config)
}

pub fn read_vacation_day_credit_hours() -> Option<f64> {
    read_config().and_then(|config| config_vacation_day_credit_hours(&config))
}

pub fn write_vacation_day_credit_hours(value: f64) -> Result<(), io::Error> {
    let mut config = read_config().unwrap_or_default();
    config.vacation_day_credit_hours = Some(value);
    write_config(&config)
}

pub fn read_sick_day_target_hours() -> Option<f64> {
    read_config().and_then(|config| config_sick_day_target_hours(&config))
}

pub fn write_sick_day_target_hours(value: f64) -> Result<(), io::Error> {
    let mut config = read_config().unwrap_or_default();
    config.sick_day_target_hours = Some(value);
    write_config(&config)
}

pub fn read_sick_day_credit_hours() -> Option<f64> {
    read_config().and_then(|config| config_sick_day_credit_hours(&config))
}

pub fn write_sick_day_credit_hours(value: f64) -> Result<(), io::Error> {
    let mut config = read_config().unwrap_or_default();
    config.sick_day_credit_hours = Some(value);
    write_config(&config)
}

pub fn read_credit_vacation_days_as_worked() -> bool {
    let default = default_credit_special_days_as_worked();
    let Some(config) = read_config() else {
        return default;
    };
    let fallback = config.credit_special_days_as_worked.unwrap_or(default);
    config.credit_vacation_days_as_worked.unwrap_or(fallback)
}

pub fn write_credit_vacation_days_as_worked(value: bool) -> Result<(), io::Error> {
    let mut config = read_config().unwrap_or_default();
    config.credit_vacation_days_as_worked = Some(value);
    write_config(&config)
}

pub fn read_credit_sick_days_as_worked() -> bool {
    let default = default_credit_special_days_as_worked();
    let Some(config) = read_config() else {
        return default;
    };
    let fallback = config.credit_special_days_as_worked.unwrap_or(default);
    config.credit_sick_days_as_worked.unwrap_or(fallback)
}

pub fn write_credit_sick_days_as_worked(value: bool) -> Result<(), io::Error> {
    let mut config = read_config().unwrap_or_default();
    config.credit_sick_days_as_worked = Some(value);
    write_config(&config)
}

fn parse_day_list(values: &[String]) -> HashSet<NaiveDate> {
    values
        .iter()
        .filter_map(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .collect()
}

fn format_day_list(values: &HashSet<NaiveDate>) -> Vec<String> {
    let mut encoded = values
        .iter()
        .map(|day| day.format("%Y-%m-%d").to_string())
        .collect::<Vec<_>>();
    encoded.sort();
    encoded
}

fn read_config() -> Option<Config> {
    let path = config_path()?;
    read_config_from_path(&path)
}

fn write_config(config: &Config) -> Result<(), io::Error> {
    let path = config_path()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    write_config_to_path(&path, config)
}

fn read_config_from_path(path: &Path) -> Option<Config> {
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_config_to_path(path: &Path, config: &Config) -> Result<(), io::Error> {
    let json =
        serde_json::to_string_pretty(config).map_err(|err| io::Error::other(err.to_string()))?;
    fs::write(path, json)
}

pub fn read_cache() -> Option<CacheFile> {
    let path = cache_path()?;
    let contents = fs::read_to_string(path).ok()?;
    let cache: CacheFile = serde_json::from_str(&contents).ok()?;
    if cache.version != 1 {
        return None;
    }
    Some(cache)
}

pub fn write_cache(cache: &CacheFile) -> Result<(), io::Error> {
    let path = cache_path()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    let json =
        serde_json::to_string_pretty(cache).map_err(|err| io::Error::other(err.to_string()))?;
    fs::write(path, json)
}

pub fn new_cache(token_hash: String) -> CacheFile {
    CacheFile {
        version: 1,
        token_hash,
        workspaces: None,
        projects: HashMap::new(),
        clients: HashMap::new(),
        time_entries: HashMap::new(),
    }
}

pub fn read_quota() -> QuotaFile {
    let today = today_string();
    let path = quota_path();
    if let Some(path) = path {
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(mut quota) = serde_json::from_str::<QuotaFile>(&contents) {
                normalize_quota(&mut quota, &today);
                return quota;
            }
        }
    }
    QuotaFile {
        version: QUOTA_FILE_VERSION,
        date: today,
        used_calls: 0,
    }
}

pub fn write_quota(quota: &QuotaFile) -> Result<(), io::Error> {
    let path = quota_path()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    let json =
        serde_json::to_string_pretty(quota).map_err(|err| io::Error::other(err.to_string()))?;
    fs::write(path, json)
}

pub fn cache_key(workspace_id: u64, start: &str, end: &str) -> String {
    format!("{workspace_id}|{start}|{end}")
}

pub fn now_rfc3339() -> String {
    Local::now().to_rfc3339()
}

pub fn today_string() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|byte| format!("{:02x}", byte)).collect()
}

fn cache_path() -> Option<PathBuf> {
    let mut path = dirs::home_dir()?;
    path.push(".toggl2tsc-cache.json");
    Some(path)
}

fn quota_path() -> Option<PathBuf> {
    let mut path = dirs::home_dir()?;
    path.push(".toggl2tsc-quota.json");
    Some(path)
}

fn normalize_quota(quota: &mut QuotaFile, today: &str) {
    if quota.version != QUOTA_FILE_VERSION {
        quota.version = QUOTA_FILE_VERSION;
        quota.used_calls = 0;
    }
    if quota.date != today {
        quota.date = today.to_string();
        quota.used_calls = 0;
    }
}

const fn default_rollup_include_weekends() -> bool {
    false
}

const fn default_credit_special_days_as_worked() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_stable() {
        let key = cache_key(123, "start", "end");
        assert_eq!(key, "123|start|end");
    }

    #[test]
    fn hash_token_is_stable() {
        let first = hash_token("token123");
        let second = hash_token("token123");
        assert_eq!(first, second);
        assert_ne!(first, "token123");
    }

    #[test]
    fn quota_resets_on_new_day() {
        let mut quota = QuotaFile {
            version: QUOTA_FILE_VERSION,
            date: "2026-02-02".to_string(),
            used_calls: 12,
        };
        normalize_quota(&mut quota, "2026-02-03");
        assert_eq!(quota.used_calls, 0);
        assert_eq!(quota.date, "2026-02-03");
    }

    #[test]
    fn parse_day_list_skips_invalid_values() {
        let values = vec![
            "2026-02-10".to_string(),
            "invalid".to_string(),
            "2026-02-11".to_string(),
        ];
        let parsed = parse_day_list(&values);
        assert_eq!(parsed.len(), 2);
        assert!(parsed.contains(&NaiveDate::from_ymd_opt(2026, 2, 10).unwrap()));
        assert!(parsed.contains(&NaiveDate::from_ymd_opt(2026, 2, 11).unwrap()));
    }

    #[test]
    fn format_day_list_is_sorted() {
        let mut values = HashSet::new();
        values.insert(NaiveDate::from_ymd_opt(2026, 2, 12).unwrap());
        values.insert(NaiveDate::from_ymd_opt(2026, 2, 10).unwrap());
        let encoded = format_day_list(&values);
        assert_eq!(encoded, vec!["2026-02-10", "2026-02-12"]);
    }

    #[test]
    fn read_special_days_merges_legacy_non_working() {
        let config = Config {
            vacation_days: vec!["2026-02-10".to_string()],
            sick_days: vec!["2026-02-11".to_string()],
            non_working_days: vec!["2026-02-12".to_string()],
            ..Config::default()
        };
        let mut vacation_days = parse_day_list(&config.vacation_days);
        vacation_days.extend(parse_day_list(&config.non_working_days));
        let sick_days = parse_day_list(&config.sick_days);
        for day in &sick_days {
            vacation_days.remove(day);
        }
        assert!(vacation_days.contains(&NaiveDate::from_ymd_opt(2026, 2, 10).unwrap()));
        assert!(vacation_days.contains(&NaiveDate::from_ymd_opt(2026, 2, 12).unwrap()));
        assert!(sick_days.contains(&NaiveDate::from_ymd_opt(2026, 2, 11).unwrap()));
    }

    #[test]
    fn legacy_special_day_hours_map_to_credit_and_normal_target_defaults() {
        let config = Config {
            target_hours: Some(8.0),
            vacation_day_hours: Some(7.6),
            sick_day_hours: Some(6.8),
            ..Config::default()
        };

        assert_eq!(config_vacation_day_target_hours(&config), Some(8.0));
        assert_eq!(config_vacation_day_credit_hours(&config), Some(7.6));
        assert_eq!(config_sick_day_target_hours(&config), Some(8.0));
        assert_eq!(config_sick_day_credit_hours(&config), Some(6.8));
    }

    #[test]
    fn explicit_special_day_target_and_credit_hours_override_legacy_values() {
        let config = Config {
            vacation_day_hours: Some(7.6),
            vacation_day_target_hours: Some(8.0),
            vacation_day_credit_hours: Some(7.2),
            sick_day_hours: Some(7.6),
            sick_day_target_hours: Some(8.0),
            sick_day_credit_hours: Some(7.4),
            ..Config::default()
        };

        assert_eq!(config_vacation_day_target_hours(&config), Some(8.0));
        assert_eq!(config_vacation_day_credit_hours(&config), Some(7.2));
        assert_eq!(config_sick_day_target_hours(&config), Some(8.0));
        assert_eq!(config_sick_day_credit_hours(&config), Some(7.4));
    }

    #[test]
    fn default_credit_special_days_is_enabled() {
        assert!(default_credit_special_days_as_worked());
    }

    #[test]
    fn legacy_theme_still_loads_without_active_theme() {
        let config = Config {
            theme: Some(ThemePreference::Dark),
            ..Config::default()
        };

        assert_eq!(
            theme_settings_from_config(&config).active_theme,
            ThemeSelection::builtin(ThemePreference::Dark)
        );
    }

    #[test]
    fn invalid_active_custom_theme_falls_back_to_legacy_builtin() {
        let config = Config {
            theme: Some(ThemePreference::TokyoNight),
            active_theme: Some(ThemeSelection::custom("missing")),
            ..Config::default()
        };

        assert_eq!(
            theme_settings_from_config(&config).active_theme,
            ThemeSelection::builtin(ThemePreference::TokyoNight)
        );
    }

    #[test]
    fn duplicate_custom_theme_names_are_rejected_case_insensitively() {
        let custom_themes = vec![CustomTheme {
            id: "theme-1".to_string(),
            name: "Aurora".to_string(),
            palette: ThemePalette {
                panel: "#111111".to_string(),
                border: "#222222".to_string(),
                text: "#333333".to_string(),
                muted: "#444444".to_string(),
                accent: "#555555".to_string(),
                highlight: "#666666".to_string(),
                success: "#777777".to_string(),
                error: "#888888".to_string(),
            },
            created_at: "2026-03-27T11:00:00+01:00".to_string(),
            updated_at: "2026-03-27T11:00:00+01:00".to_string(),
        }];

        assert!(has_duplicate_custom_name(&custom_themes, "aurora", None));
        assert!(!has_duplicate_custom_name(
            &custom_themes,
            "aurora",
            Some("theme-1")
        ));
    }

    #[test]
    fn custom_theme_round_trips_through_config_json() {
        let config = Config {
            theme: Some(ThemePreference::Dark),
            active_theme: Some(ThemeSelection::custom("theme-aurora")),
            custom_themes: vec![CustomTheme {
                id: "theme-aurora".to_string(),
                name: "Aurora".to_string(),
                palette: ThemePalette {
                    panel: "#111111".to_string(),
                    border: "#222222".to_string(),
                    text: "#333333".to_string(),
                    muted: "#444444".to_string(),
                    accent: "#555555".to_string(),
                    highlight: "#666666".to_string(),
                    success: "#777777".to_string(),
                    error: "#888888".to_string(),
                },
                created_at: "2026-03-27T11:00:00+01:00".to_string(),
                updated_at: "2026-03-27T11:00:00+01:00".to_string(),
            }],
            ..Config::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        let decoded: Config = serde_json::from_str(&json).unwrap();
        let settings = theme_settings_from_config(&decoded);

        assert_eq!(settings.custom_themes.len(), 1);
        assert_eq!(
            settings.active_theme,
            ThemeSelection::custom("theme-aurora")
        );
    }
}
