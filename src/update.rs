#[cfg(feature = "update")]
mod enabled {
    use reqwest::blocking::Client;
    use semver::Version;
    use serde::Deserialize;
    use std::env;
    use std::fs;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    const RELEASES_URL: &str =
        "https://api.github.com/repos/NoahNxT/Toggl2Timeshit/releases/latest";
    const FORCE_UPDATE_DIALOG_ENV: &str = "TIMESHIT_FORCE_UPDATE_DIALOG";
    const FORCE_UPDATE_VERSION_ENV: &str = "TIMESHIT_FORCE_UPDATE_VERSION";
    const FORCE_UPDATE_URL_ENV: &str = "TIMESHIT_FORCE_UPDATE_URL";

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ForcedUpdateMode {
        Installable,
        Manual,
        PackageManager,
    }

    #[derive(Debug, Clone)]
    pub struct UpdateInfo {
        pub latest: Version,
        pub changelog_url: String,
        pub release_notes: Vec<String>,
    }

    #[derive(Debug)]
    pub enum UpdateError {
        Network(String),
        Parse(String),
        Io(String),
    }

    impl std::fmt::Display for UpdateError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                UpdateError::Network(message) => write!(f, "Network error: {message}"),
                UpdateError::Parse(message) => write!(f, "Parse error: {message}"),
                UpdateError::Io(message) => write!(f, "IO error: {message}"),
            }
        }
    }

    impl std::error::Error for UpdateError {}

    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        html_url: String,
        #[serde(default)]
        body: String,
    }

    pub fn current_version() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION"))
            .expect("CARGO_PKG_VERSION should be a valid semantic version")
    }

    pub fn should_check_updates() -> bool {
        if is_forced_update_dialog() {
            return true;
        }

        if cfg!(debug_assertions) {
            return false;
        }

        if let Ok(path) = std::env::current_exe() {
            let path = path.to_string_lossy();
            if path.contains("/target/") || path.contains("\\target\\") {
                return false;
            }
        }

        true
    }

    pub fn is_forced_update_dialog() -> bool {
        forced_update_mode().is_some()
    }

    pub fn is_direct_install() -> bool {
        if matches!(forced_update_mode(), Some(ForcedUpdateMode::PackageManager)) {
            return false;
        }

        !is_managed_install()
    }

    pub fn check_for_update() -> Result<Option<UpdateInfo>, UpdateError> {
        if let Some(info) = forced_update_info()? {
            return Ok(Some(info));
        }

        let client = build_client()?;
        let response = client
            .get(RELEASES_URL)
            .send()
            .map_err(|err| UpdateError::Network(err.to_string()))?;

        if !response.status().is_success() {
            return Err(UpdateError::Network(format!(
                "GitHub API error: {}",
                response.status()
            )));
        }

        let release: Release = response
            .json()
            .map_err(|err| UpdateError::Parse(err.to_string()))?;

        resolve_update_info(release, &current_version())
    }

    pub fn open_release_page(url: &str) -> Result<(), UpdateError> {
        let mut command = match env::consts::OS {
            "macos" => {
                let mut command = Command::new("open");
                command.arg(url);
                command
            }
            "windows" => {
                let mut command = Command::new("cmd");
                command.args(["/C", "start", "", url]);
                command
            }
            _ => {
                let mut command = Command::new("xdg-open");
                command.arg(url);
                command
            }
        };

        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| UpdateError::Io(err.to_string()))?;

        Ok(())
    }

    fn build_client() -> Result<Client, UpdateError> {
        Client::builder()
            .user_agent("timeshit-tui")
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|err| UpdateError::Network(err.to_string()))
    }

    fn forced_update_info() -> Result<Option<UpdateInfo>, UpdateError> {
        let Some(mode) = forced_update_mode() else {
            return Ok(None);
        };

        let current = current_version();
        let latest = forced_update_version(&current)?;
        let changelog_url = env::var(FORCE_UPDATE_URL_ENV).unwrap_or_else(|_| {
            "https://github.com/NoahNxT/Toggl2Timeshit/releases/latest".to_string()
        });
        let release_notes = forced_release_notes(mode, &latest);

        Ok(Some(UpdateInfo {
            latest,
            changelog_url,
            release_notes,
        }))
    }

    fn forced_update_mode() -> Option<ForcedUpdateMode> {
        let value = env::var(FORCE_UPDATE_DIALOG_ENV).ok()?;
        let value = value.trim().to_ascii_lowercase();
        let mode = match value.as_str() {
            "" | "1" | "true" | "yes" | "self" | "installable" => ForcedUpdateMode::Installable,
            "manual" | "download" => ForcedUpdateMode::Manual,
            "managed" | "package" | "package-manager" => ForcedUpdateMode::PackageManager,
            _ => ForcedUpdateMode::Installable,
        };
        Some(mode)
    }

    fn forced_update_version(current: &Version) -> Result<Version, UpdateError> {
        let version = match env::var(FORCE_UPDATE_VERSION_ENV) {
            Ok(value) => Version::parse(value.trim().trim_start_matches('v'))
                .map_err(|err| UpdateError::Parse(err.to_string()))?,
            Err(_) => next_patch_version(current),
        };

        if version > *current {
            Ok(version)
        } else {
            Ok(next_patch_version(current))
        }
    }

    fn next_patch_version(current: &Version) -> Version {
        Version::new(current.major, current.minor, current.patch + 1)
    }

    fn forced_release_notes(mode: ForcedUpdateMode, latest: &Version) -> Vec<String> {
        let mut notes = vec![format!("Previewing update dialog content for v{latest}.")];
        if matches!(mode, ForcedUpdateMode::PackageManager) {
            notes.push(
                "This preview simulates an install that should be updated outside the release page flow."
                    .to_string(),
            );
        } else {
            notes.push(
                "This preview simulates a direct GitHub install where pressing u opens the latest release page."
                    .to_string(),
            );
        }
        notes.push(
            "Open the release page in your browser for the full GitHub changelog layout."
                .to_string(),
        );
        notes
    }

    fn resolve_update_info(
        release: Release,
        current: &Version,
    ) -> Result<Option<UpdateInfo>, UpdateError> {
        let trimmed = release.tag_name.trim().trim_start_matches('v');
        let latest = Version::parse(trimmed).map_err(|err| UpdateError::Parse(err.to_string()))?;
        if latest <= *current {
            return Ok(None);
        }

        Ok(Some(UpdateInfo {
            latest,
            changelog_url: release.html_url,
            release_notes: release_notes_from_markdown(&release.body),
        }))
    }

    fn release_notes_from_markdown(markdown: &str) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();

        for raw_line in markdown.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                if !matches!(lines.last(), Some(last) if last.is_empty()) {
                    lines.push(String::new());
                }
                continue;
            }

            let mut line = trimmed
                .trim_start_matches('#')
                .trim()
                .replace("**", "")
                .replace("__", "")
                .replace('`', "");

            if line.eq_ignore_ascii_case("What's Changed") {
                continue;
            }

            if let Some(stripped) = line.strip_prefix("* ").or_else(|| line.strip_prefix("- ")) {
                line = format!("• {}", stripped.trim());
            }

            lines.push(line);
        }

        while matches!(lines.last(), Some(last) if last.is_empty()) {
            lines.pop();
        }

        lines
    }

    fn is_managed_install() -> bool {
        let exe = match std::env::current_exe() {
            Ok(path) => path,
            Err(_) => return false,
        };
        let canonical = fs::canonicalize(&exe).unwrap_or(exe);
        let path_str = canonical.to_string_lossy().to_lowercase();

        if path_str.contains("/cellar/") || path_str.contains("\\cellar\\") {
            return true;
        }

        if path_str.contains("\\chocolatey\\lib\\")
            || path_str.contains("\\chocolatey\\bin\\")
            || path_str.contains("\\scoop\\apps\\")
            || path_str.contains("\\windowsapps\\")
        {
            return true;
        }

        if cfg!(target_os = "linux") {
            if Path::new("/var/lib/dpkg/info/timeshit.list").exists()
                || Path::new("/usr/share/doc/timeshit").exists()
            {
                return true;
            }
        }

        false
    }
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn newer_release_still_returns_update_info() {
            let release = Release {
                tag_name: "v1.2.0".to_string(),
                html_url: "https://example.com/releases/v1.2.0".to_string(),
                body: String::new(),
            };

            let info = resolve_update_info(release, &Version::parse("1.1.0").unwrap())
                .unwrap()
                .expect("expected update info");

            assert_eq!(info.latest, Version::parse("1.2.0").unwrap());
        }

        #[test]
        fn current_release_returns_none() {
            let release = Release {
                tag_name: "v1.2.0".to_string(),
                html_url: "https://example.com/releases/v1.2.0".to_string(),
                body: String::new(),
            };

            let info = resolve_update_info(release, &Version::parse("1.2.0").unwrap()).unwrap();

            assert!(info.is_none());
        }

        #[test]
        fn release_notes_strip_github_markdown() {
            let notes = release_notes_from_markdown(
                "## What's Changed\n* Fix updater popup\n\n**Full Changelog**: https://example.com",
            );

            assert_eq!(
                notes,
                vec![
                    "• Fix updater popup".to_string(),
                    String::new(),
                    "Full Changelog: https://example.com".to_string()
                ]
            );
        }
    }
}

#[cfg(not(feature = "update"))]
mod disabled {
    use std::env;
    use std::fmt;
    const FORCE_UPDATE_DIALOG_ENV: &str = "TIMESHIT_FORCE_UPDATE_DIALOG";
    const FORCE_UPDATE_VERSION_ENV: &str = "TIMESHIT_FORCE_UPDATE_VERSION";
    const FORCE_UPDATE_URL_ENV: &str = "TIMESHIT_FORCE_UPDATE_URL";

    #[derive(Debug, Clone)]
    pub struct Version(String);

    impl fmt::Display for Version {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    #[derive(Debug, Clone)]
    pub struct UpdateInfo {
        pub latest: Version,
        pub changelog_url: String,
        pub release_notes: Vec<String>,
    }

    #[derive(Debug)]
    pub enum UpdateError {
        Unsupported(String),
    }

    impl fmt::Display for UpdateError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                UpdateError::Unsupported(message) => write!(f, "Unsupported: {message}"),
            }
        }
    }

    impl std::error::Error for UpdateError {}

    pub fn current_version() -> Version {
        Version(env!("CARGO_PKG_VERSION").to_string())
    }

    pub fn should_check_updates() -> bool {
        env::var(FORCE_UPDATE_DIALOG_ENV).is_ok()
    }

    pub fn is_forced_update_dialog() -> bool {
        env::var(FORCE_UPDATE_DIALOG_ENV).is_ok()
    }

    pub fn is_direct_install() -> bool {
        !matches!(
            env::var(FORCE_UPDATE_DIALOG_ENV)
                .ok()
                .map(|value| value.trim().to_ascii_lowercase())
                .as_deref(),
            Some("managed" | "package" | "package-manager")
        )
    }

    pub fn check_for_update() -> Result<Option<UpdateInfo>, UpdateError> {
        if env::var(FORCE_UPDATE_DIALOG_ENV).is_err() {
            return Ok(None);
        }

        let latest = env::var(FORCE_UPDATE_VERSION_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("{}.forced", env!("CARGO_PKG_VERSION")));
        let changelog_url = env::var(FORCE_UPDATE_URL_ENV).unwrap_or_else(|_| {
            "https://github.com/NoahNxT/Toggl2Timeshit/releases/latest".to_string()
        });
        Ok(Some(UpdateInfo {
            latest: Version(latest),
            changelog_url,
            release_notes: vec![
                "Previewing forced update dialog content.".to_string(),
                "Open the release page in your browser for the full GitHub changelog layout."
                    .to_string(),
            ],
        }))
    }

    pub fn open_release_page(_url: &str) -> Result<(), UpdateError> {
        Err(UpdateError::Unsupported(
            "Opening the release page is disabled in this build".to_string(),
        ))
    }
}

#[cfg(feature = "update")]
pub use enabled::*;

#[cfg(not(feature = "update"))]
pub use disabled::*;
