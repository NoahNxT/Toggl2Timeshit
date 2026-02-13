use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::models::{Client as TogglClientModel, Project, TimeEntry, Workspace};
use crate::rollups::WeekStart;
use crate::rounding::RoundingConfig;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    Terminal,
    Light,
    Dark,
}

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

pub fn read_theme() -> Option<ThemePreference> {
    read_config().and_then(|config| config.theme)
}

pub fn write_theme(theme: ThemePreference) -> Result<(), io::Error> {
    let mut config = read_config().unwrap_or_default();
    config.theme = Some(theme);
    write_config(&config)
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Config {
    theme: Option<ThemePreference>,
    target_hours: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rounding: Option<RoundingConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rollup_preferences: Option<RollupPreferences>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    non_working_days: Vec<String>,
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

pub fn read_non_working_days() -> HashSet<NaiveDate> {
    read_config()
        .map(|config| parse_non_working_days(&config.non_working_days))
        .unwrap_or_default()
}

pub fn write_non_working_days(value: &HashSet<NaiveDate>) -> Result<(), io::Error> {
    let mut config = read_config().unwrap_or_default();
    config.non_working_days = format_non_working_days(value);
    write_config(&config)
}

fn parse_non_working_days(values: &[String]) -> HashSet<NaiveDate> {
    values
        .iter()
        .filter_map(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .collect()
}

fn format_non_working_days(values: &HashSet<NaiveDate>) -> Vec<String> {
    let mut encoded = values
        .iter()
        .map(|day| day.format("%Y-%m-%d").to_string())
        .collect::<Vec<_>>();
    encoded.sort();
    encoded
}

fn read_config() -> Option<Config> {
    let path = config_path()?;
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_config(config: &Config) -> Result<(), io::Error> {
    let path = config_path()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    let json = serde_json::to_string_pretty(config)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
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
    let json = serde_json::to_string_pretty(cache)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
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
    let json = serde_json::to_string_pretty(quota)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
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
    fn parse_non_working_days_skips_invalid_values() {
        let values = vec![
            "2026-02-10".to_string(),
            "invalid".to_string(),
            "2026-02-11".to_string(),
        ];
        let parsed = parse_non_working_days(&values);
        assert_eq!(parsed.len(), 2);
        assert!(parsed.contains(&NaiveDate::from_ymd_opt(2026, 2, 10).unwrap()));
        assert!(parsed.contains(&NaiveDate::from_ymd_opt(2026, 2, 11).unwrap()));
    }

    #[test]
    fn format_non_working_days_is_sorted() {
        let mut values = HashSet::new();
        values.insert(NaiveDate::from_ymd_opt(2026, 2, 12).unwrap());
        values.insert(NaiveDate::from_ymd_opt(2026, 2, 10).unwrap());
        let encoded = format_non_working_days(&values);
        assert_eq!(encoded, vec!["2026-02-10", "2026-02-12"]);
    }
}
