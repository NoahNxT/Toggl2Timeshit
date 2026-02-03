use chrono::Local;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::models::{Client as TogglClientModel, Project, TimeEntry, Workspace};
use crate::rounding::RoundingConfig;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
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
    pub date: String,
    pub used_calls: u32,
}

pub fn read_token() -> Option<String> {
    if let Ok(value) = env::var("TOGGL_API_TOKEN") {
        if !value.trim().is_empty() {
            return Some(value);
        }
    }

    let path = token_path()?;
    fs::read_to_string(path).ok().map(|value| value.trim().to_string())
}

pub fn write_token(token: &str) -> Result<(), io::Error> {
    let path = token_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
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

fn read_config() -> Option<Config> {
    let path = config_path()?;
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_config(config: &Config) -> Result<(), io::Error> {
    let path = config_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
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
    let path = cache_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
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
        date: today,
        used_calls: 0,
    }
}

pub fn write_quota(quota: &QuotaFile) -> Result<(), io::Error> {
    let path = quota_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
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
    if quota.date != today {
        quota.date = today.to_string();
        quota.used_calls = 0;
    }
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
            date: "2026-02-02".to_string(),
            used_calls: 12,
        };
        normalize_quota(&mut quota, "2026-02-03");
        assert_eq!(quota.used_calls, 0);
        assert_eq!(quota.date, "2026-02-03");
    }
}
