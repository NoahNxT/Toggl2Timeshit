# Toggl2Timeshit (Timeshit TUI)

Toggl2Timeshit is now a Rust-based Terminal UI (TUI) that turns Toggl Track time entries into a clean, navigable timesheet dashboard.

## Features
- TUI dashboard with project summaries and ticket details
- Workspace selection
- Date filters (single day or range)
- Weekly/monthly rollups with per-day totals and overtime/undertime vs target
- Total hours with visual status
- Forced auto-update prompt on launch (GitHub Releases)
- Optional time rounding (increment + mode)
- Secure token storage compatible with previous versions (`~/.toggl2tsc`)
- Persistent cache to minimize Toggl API usage (manual refresh only)
- Built-in settings for target hours and integrations

## Installation
Choose a provider below. GitHub Releases is recommended because the in-app updater also uses GitHub Releases.

### GitHub Releases (Recommended)
Download the asset that matches your OS from the GitHub Releases page.

Asset names:
- macOS: `timeshit-macOS.tar.gz` (binary: `timeshit-macOS`)
- Linux: `timeshit-Linux.tar.gz` (binary: `timeshit-Linux`)
- Windows: `timeshit-Windows.tar.gz` (binary: `timeshit.exe`)

macOS / Linux:
1. Extract the archive.
2. Make the binary executable.
3. Move it into your PATH.

### Homebrew (macOS/Linux)
```bash
brew tap NoahNxT/nxt-solutions-packages
brew install timeshit
```

### Scoop (Windows)
```powershell
scoop bucket add nxt-solutions https://github.com/NoahNxT/nxt-solutions-packages
scoop install timeshit
```

### Build from Source
```bash
tar -xzf <asset>.tar.gz
chmod +x <binary>
sudo mv <binary> /usr/local/bin/timeshit
```

Windows (PowerShell):
1. Extract the archive.
2. Move `timeshit.exe` into a folder on your PATH (or add a new folder to PATH).

```powershell
tar -xf timeshit-Windows.tar.gz
mkdir "C:\\Program Files\\timeshit" -Force
Move-Item timeshit.exe "C:\\Program Files\\timeshit\\timeshit.exe"
```

Verify the install:
```bash
timeshit --version
```

### Build from Source (Cargo)
```bash
cargo build --release
```
The binary will be available at `target/release/timeshit` (or `target/release/timeshit.exe` on Windows).

### Local Cargo Install
```bash
cargo install --path .
```

## Authentication
Run the login flow in the TUI or set an environment variable:
```bash
timeshit login
```
or
```bash
export TOGGL_API_TOKEN="your-token"
```
The token is stored at `~/.toggl2tsc`.

## Usage
Launch the dashboard:
```bash
timeshit
```

On launch, Timeshit checks GitHub Releases for updates. If a newer version is found, you must install it to continue. If the update check or download fails (offline/GitHub down), the app shows a warning and continues. After a successful update, the app exits and should be relaunched.

Optional flags:
```bash
timeshit --date YYYY-MM-DD
timeshit --start-date YYYY-MM-DD --end-date YYYY-MM-DD
```

## Documentation
Full documentation is available via GitHub Pages (see `docs/`).

## Controls
- `h` help (shows all keybinds)
- `c` copy client entries to clipboard
- `v` copy project entries to clipboard
- `x` copy all entries with project and client names to clipboard
- `Enter` browse entries (dashboard)
- `b` copy selected entry title (when browsing entries)
- `n` copy selected entry hours (when browsing entries)
- `Right`/`Tab` switch to entries (dashboard)
- `Left`/`Shift+Tab` switch to projects (dashboard)
- `Esc` back to projects (when browsing entries)
- `d` set date range
- `y` yesterday
- Arrow keys to navigate projects and entries

## License
MIT
