#[cfg(feature = "update")]
mod enabled {
    use flate2::read::GzDecoder;
    use reqwest::blocking::Client;
    use semver::Version;
    use serde::Deserialize;
    use std::env;
    use std::fs::{self, File};
    use std::io;
    use std::path::PathBuf;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::Duration;
    use tar::Archive;
    use tempfile::Builder;
    use zip::ZipArchive;

    const RELEASES_URL: &str =
        "https://api.github.com/repos/NoahNxT/Toggl2Timeshit/releases/latest";

    #[derive(Debug, Clone)]
    pub struct UpdateInfo {
        pub latest: Version,
        pub tag: String,
        pub asset_name: String,
        pub url: String,
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
        assets: Vec<ReleaseAsset>,
    }

    #[derive(Deserialize)]
    struct ReleaseAsset {
        name: String,
        browser_download_url: String,
    }

    pub fn current_version() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION"))
            .expect("CARGO_PKG_VERSION should be a valid semantic version")
    }

    pub fn should_check_updates() -> bool {
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

    pub fn check_for_update() -> Result<Option<UpdateInfo>, UpdateError> {
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

        let tag = release.tag_name.trim().to_string();
        let trimmed = tag.trim_start_matches('v');
        let latest =
            Version::parse(trimmed).map_err(|err| UpdateError::Parse(err.to_string()))?;
        let current = current_version();

        if latest <= current {
            return Ok(None);
        }

        let asset_name = expected_asset_name()?;
        let asset = release
            .assets
            .into_iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| UpdateError::Parse(format!("Release asset {asset_name} not found")))?;

        Ok(Some(UpdateInfo {
            latest,
            tag,
            asset_name: asset.name,
            url: asset.browser_download_url,
        }))
    }

    pub fn download_and_extract(info: &UpdateInfo) -> Result<PathBuf, UpdateError> {
        let client = build_client()?;
        let response = client
            .get(&info.url)
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

        let archive_path = tempdir.path().join(&info.asset_name);
        let mut archive_file =
            File::create(&archive_path).map_err(|err| UpdateError::Io(err.to_string()))?;
        let mut reader = response;
        io::copy(&mut reader, &mut archive_file)
            .map_err(|err| UpdateError::Io(err.to_string()))?;

        let archive_file =
            File::open(&archive_path).map_err(|err| UpdateError::Io(err.to_string()))?;
        if info.asset_name.ends_with(".zip") {
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

        let binary_name = expected_binary_name()?;
        let extracted_path = find_extracted_binary(tempdir.path(), &binary_name)?;
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

    fn expected_asset_name() -> Result<String, UpdateError> {
        let asset = match env::consts::OS {
            "linux" => "timeshit-linux.tar.gz",
            "macos" => "timeshit-macos.tar.gz",
            "windows" => "timeshit-windows.zip",
            other => {
                return Err(UpdateError::Unsupported(format!(
                    "Unsupported OS: {other}"
                )))
            }
        };
        Ok(asset.to_string())
    }

    fn expected_binary_name() -> Result<String, UpdateError> {
        let binary = match env::consts::OS {
            "linux" => "timeshit",
            "macos" => "timeshit",
            "windows" => "timeshit.exe",
            other => {
                return Err(UpdateError::Unsupported(format!(
                    "Unsupported OS: {other}"
                )))
            }
        };
        Ok(binary.to_string())
    }

    fn find_extracted_binary(dir: &Path, expected: &str) -> Result<PathBuf, UpdateError> {
        let direct = dir.join(expected);
        if direct.exists() {
            return Ok(direct);
        }

        let entries = fs::read_dir(dir).map_err(|err| UpdateError::Io(err.to_string()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == expected)
            {
                return Ok(path);
            }
        }

        Err(UpdateError::Parse(format!(
            "Extracted binary {expected} not found"
        )))
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

    #[cfg(unix)]
    fn install_update_unix(staged_path: &Path, current_exe: &Path) -> Result<(), UpdateError> {
        fs::copy(staged_path, current_exe).map_err(|err| UpdateError::Io(err.to_string()))?;
        fs::set_permissions(
            current_exe,
            fs::Permissions::from_mode(0o755),
        )
        .map_err(|err| UpdateError::Io(err.to_string()))?;
        Ok(())
    }

    #[cfg(windows)]
    fn install_update_windows(staged_path: &Path, current_exe: &Path) -> Result<(), UpdateError> {
        let temp_path = current_exe
            .with_extension("exe.new");
        fs::copy(staged_path, &temp_path)
            .map_err(|err| UpdateError::Io(err.to_string()))?;
        fs::rename(temp_path, current_exe)
            .map_err(|err| UpdateError::Io(err.to_string()))?;
        Ok(())
    }
}

#[cfg(not(feature = "update"))]
mod disabled {
    use std::env;
    use std::fmt;
    use std::path::{Path, PathBuf};

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
        pub tag: String,
        pub asset_name: String,
        pub url: String,
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
        false
    }

    pub fn check_for_update() -> Result<Option<UpdateInfo>, UpdateError> {
        Ok(None)
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
