#[cfg(feature = "update")]
mod enabled {
    use flate2::read::GzDecoder;
    use reqwest::blocking::Client;
    use semver::Version;
    use serde::Deserialize;
    use std::env;
    use std::fs::{self, File};
    use std::io;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::path::PathBuf;
    use std::time::Duration;
    use tar::Archive;
    use tempfile::Builder;
    use zip::ZipArchive;

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
        pub asset_name: Option<String>,
        pub url: Option<String>,
        pub changelog_url: String,
    }

    impl UpdateInfo {
        pub fn has_download(&self) -> bool {
            self.asset_name.is_some() && self.url.is_some()
        }
    }

    #[derive(Debug)]
    pub enum UpdateError {
        Network(String),
        Parse(String),
        Io(String),
        Unsupported(String),
    }

    impl std::fmt::Display for UpdateError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                UpdateError::Network(message) => write!(f, "Network error: {message}"),
                UpdateError::Parse(message) => write!(f, "Parse error: {message}"),
                UpdateError::Io(message) => write!(f, "IO error: {message}"),
                UpdateError::Unsupported(message) => write!(f, "Unsupported: {message}"),
            }
        }
    }

    impl std::error::Error for UpdateError {}

    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        html_url: String,
        assets: Vec<ReleaseAsset>,
    }

    #[derive(Clone, Deserialize)]
    struct ReleaseAsset {
        name: String,
        browser_download_url: String,
    }

    pub fn current_version() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION"))
            .expect("CARGO_PKG_VERSION should be a valid semantic version")
    }

    pub fn should_check_updates() -> bool {
        if forced_update_mode().is_some() {
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

    pub fn can_self_update() -> bool {
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

    pub fn download_and_extract(info: &UpdateInfo) -> Result<PathBuf, UpdateError> {
        let asset_name = info.asset_name.as_deref().ok_or_else(|| {
            UpdateError::Unsupported(
                "No downloadable update asset found for this release".to_string(),
            )
        })?;
        let url = info.url.as_deref().ok_or_else(|| {
            UpdateError::Unsupported(
                "No downloadable update asset found for this release".to_string(),
            )
        })?;
        let client = build_client()?;
        let response = client
            .get(url)
            .send()
            .map_err(|err| UpdateError::Network(err.to_string()))?;

        if !response.status().is_success() {
            return Err(UpdateError::Network(format!(
                "Download failed: {}",
                response.status()
            )));
        }

        let tempdir = Builder::new()
            .prefix("timeshit-update-")
            .tempdir()
            .map_err(|err| UpdateError::Io(err.to_string()))?;

        let archive_path = tempdir.path().join(asset_name);
        let mut archive_file =
            File::create(&archive_path).map_err(|err| UpdateError::Io(err.to_string()))?;
        let mut reader = response;
        io::copy(&mut reader, &mut archive_file).map_err(|err| UpdateError::Io(err.to_string()))?;

        let archive_file =
            File::open(&archive_path).map_err(|err| UpdateError::Io(err.to_string()))?;
        if asset_name.ends_with(".zip") {
            let mut archive =
                ZipArchive::new(archive_file).map_err(|err| UpdateError::Io(err.to_string()))?;
            archive
                .extract(tempdir.path())
                .map_err(|err| UpdateError::Io(err.to_string()))?;
        } else {
            let decoder = GzDecoder::new(archive_file);
            let mut archive = Archive::new(decoder);
            archive
                .unpack(tempdir.path())
                .map_err(|err| UpdateError::Io(err.to_string()))?;
        }

        let binary_candidates = expected_binary_candidates()?;
        let extracted_path = find_extracted_binary(tempdir.path(), &binary_candidates)?;
        let _persisted_dir = tempdir.keep();

        Ok(extracted_path)
    }

    pub fn install_update(staged_path: &Path, current_exe: &Path) -> Result<(), UpdateError> {
        #[cfg(windows)]
        {
            install_update_windows(staged_path, current_exe)?;
            return Ok(());
        }

        #[cfg(unix)]
        {
            install_update_unix(staged_path, current_exe)?;
            return Ok(());
        }

        #[allow(unreachable_code)]
        Err(UpdateError::Unsupported(
            "Unsupported platform for update install".to_string(),
        ))
    }

    pub fn cleanup_staged(path: &Path) {
        if let Some(parent) = path.parent() {
            if let Some(name) = parent.file_name().and_then(|value| value.to_str()) {
                if name.starts_with("timeshit-update-") {
                    let _ = fs::remove_dir_all(parent);
                }
            }
        }
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
        let (asset_name, url) = match mode {
            ForcedUpdateMode::Installable => mock_download_asset(),
            ForcedUpdateMode::Manual | ForcedUpdateMode::PackageManager => (None, None),
        };

        Ok(Some(UpdateInfo {
            latest,
            asset_name,
            url,
            changelog_url,
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

    fn mock_download_asset() -> (Option<String>, Option<String>) {
        let asset_name = expected_asset_candidates()
            .and_then(|mut values| values.pop())
            .unwrap_or_else(|| "timeshit-update.tar.gz".to_string());
        (
            Some(asset_name),
            Some("https://example.invalid/timeshit-update".to_string()),
        )
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

        let asset = find_matching_release_asset(&release.assets);

        Ok(Some(UpdateInfo {
            latest,
            asset_name: asset.as_ref().map(|value| value.name.clone()),
            url: asset.map(|value| value.browser_download_url),
            changelog_url: release.html_url,
        }))
    }

    fn find_matching_release_asset(assets: &[ReleaseAsset]) -> Option<ReleaseAsset> {
        let candidates = expected_asset_candidates()?;
        let candidate_set: std::collections::HashSet<String> = candidates
            .into_iter()
            .map(|name| name.to_lowercase())
            .collect();

        assets
            .iter()
            .find(|asset| candidate_set.contains(&asset.name.to_lowercase()))
            .cloned()
    }

    fn expected_asset_candidates() -> Option<Vec<String>> {
        let assets = match env::consts::OS {
            "linux" => vec!["timeshit-linux.tar.gz", "timeshit-Linux.tar.gz"],
            "macos" => vec!["timeshit-macos.tar.gz", "timeshit-macOS.tar.gz"],
            "windows" => vec!["timeshit-windows.zip", "timeshit-Windows.zip"],
            _ => return None,
        };
        Some(assets.into_iter().map(|value| value.to_string()).collect())
    }

    fn expected_binary_candidates() -> Result<Vec<String>, UpdateError> {
        let binaries = match env::consts::OS {
            "linux" => vec!["timeshit", "timeshit-Linux"],
            "macos" => vec!["timeshit", "timeshit-macOS"],
            "windows" => vec!["timeshit.exe"],
            other => return Err(UpdateError::Unsupported(format!("Unsupported OS: {other}"))),
        };
        Ok(binaries
            .into_iter()
            .map(|value| value.to_string())
            .collect())
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

    fn find_extracted_binary(dir: &Path, expected: &[String]) -> Result<PathBuf, UpdateError> {
        for name in expected {
            let direct = dir.join(name);
            if direct.exists() {
                return Ok(direct);
            }
        }

        let entries = fs::read_dir(dir).map_err(|err| UpdateError::Io(err.to_string()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
                if expected.iter().any(|candidate| candidate == name) {
                    return Ok(path);
                }
            }
        }

        Err(UpdateError::Parse(format!(
            "Extracted binary {} not found",
            expected.join(" or ")
        )))
    }

    #[cfg(unix)]
    fn install_update_unix(staged_path: &Path, current_exe: &Path) -> Result<(), UpdateError> {
        fs::copy(staged_path, current_exe).map_err(|err| UpdateError::Io(err.to_string()))?;
        fs::set_permissions(current_exe, fs::Permissions::from_mode(0o755))
            .map_err(|err| UpdateError::Io(err.to_string()))?;
        Ok(())
    }

    #[cfg(windows)]
    fn install_update_windows(staged_path: &Path, current_exe: &Path) -> Result<(), UpdateError> {
        let temp_path = current_exe.with_extension("exe.new");
        fs::copy(staged_path, &temp_path).map_err(|err| UpdateError::Io(err.to_string()))?;
        fs::rename(temp_path, current_exe).map_err(|err| UpdateError::Io(err.to_string()))?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn newer_release_still_returns_update_without_matching_asset() {
            let release = Release {
                tag_name: "v1.2.0".to_string(),
                html_url: "https://example.com/releases/v1.2.0".to_string(),
                assets: vec![],
            };

            let info = resolve_update_info(release, &Version::parse("1.1.0").unwrap())
                .unwrap()
                .expect("expected update info");

            assert_eq!(info.latest, Version::parse("1.2.0").unwrap());
            assert!(!info.has_download());
        }

        #[test]
        fn current_release_returns_none() {
            let release = Release {
                tag_name: "v1.2.0".to_string(),
                html_url: "https://example.com/releases/v1.2.0".to_string(),
                assets: vec![],
            };

            let info = resolve_update_info(release, &Version::parse("1.2.0").unwrap()).unwrap();

            assert!(info.is_none());
        }

        #[test]
        fn matching_asset_keeps_download_available() {
            let Some(candidate) = expected_asset_candidates().and_then(|mut values| values.pop())
            else {
                return;
            };
            let release = Release {
                tag_name: "v1.2.0".to_string(),
                html_url: "https://example.com/releases/v1.2.0".to_string(),
                assets: vec![ReleaseAsset {
                    name: candidate,
                    browser_download_url: "https://example.com/download".to_string(),
                }],
            };

            let info = resolve_update_info(release, &Version::parse("1.1.0").unwrap())
                .unwrap()
                .expect("expected update info");

            assert!(info.has_download());
        }
    }
}

#[cfg(not(feature = "update"))]
mod disabled {
    use std::env;
    use std::fmt;
    use std::path::{Path, PathBuf};

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
        pub asset_name: Option<String>,
        pub url: Option<String>,
        pub changelog_url: String,
    }

    impl UpdateInfo {
        pub fn has_download(&self) -> bool {
            self.asset_name.is_some() && self.url.is_some()
        }
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

    pub fn can_self_update() -> bool {
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
        let manual_mode = matches!(
            env::var(FORCE_UPDATE_DIALOG_ENV)
                .ok()
                .map(|value| value.trim().to_ascii_lowercase())
                .as_deref(),
            Some("manual" | "download" | "managed" | "package" | "package-manager")
        );

        Ok(Some(UpdateInfo {
            latest: Version(latest),
            asset_name: (!manual_mode).then(|| "timeshit-update.tar.gz".to_string()),
            url: (!manual_mode).then(|| "https://example.invalid/timeshit-update".to_string()),
            changelog_url,
        }))
    }

    pub fn download_and_extract(_info: &UpdateInfo) -> Result<PathBuf, UpdateError> {
        Err(UpdateError::Unsupported(
            "Updates are disabled in this build".to_string(),
        ))
    }

    pub fn install_update(_staged_path: &Path, _current_exe: &Path) -> Result<(), UpdateError> {
        Err(UpdateError::Unsupported(
            "Updates are disabled in this build".to_string(),
        ))
    }

    pub fn cleanup_staged(_path: &Path) {}
}

#[cfg(feature = "update")]
pub use enabled::*;

#[cfg(not(feature = "update"))]
pub use disabled::*;
