# Timeshit (Toggl2Timeshit)

Timeshit is a Rust-based Terminal UI (TUI) that turns Toggl Track time entries into a clean, navigable timesheet dashboard. It is optimized for free-tier API limits with a persistent cache and manual refresh.

## Highlights
- Modern TUI dashboard with grouped summaries and entry details
- Workspace selection and fast date range filtering
- Clipboard export for timesheet tools
- Cache-first design to reduce Toggl API usage
- Built-in settings (target hours, integrations, theme)

## Installation

### macOS
**Homebrew (recommended)**
```bash
brew tap NoahNxT/nxt-solutions-packages
brew install timeshit
```
Upgrade:
```bash
brew upgrade timeshit
```

### Windows
**Chocolatey**
```powershell
choco install timeshit
```
```powershell
choco install timeshit --version=1.3.6
```

**Scoop**
```powershell
scoop bucket add nxt-solutions https://github.com/NoahNxT/homebrew-nxt-solutions-packages
scoop install timeshit
```

**Winget**
```powershell
winget install NxTSolutions.Timeshit
```

### Linux
**APT (Debian/Ubuntu via Cloudsmith)**
```bash
curl -1sLf 'https://dl.cloudsmith.io/public/nxt-solutions/timeshit/setup.deb.sh' | sudo -E bash
sudo apt-get update
sudo apt-get install timeshit
```

### GitHub Releases (any OS)
Download the latest asset from GitHub Releases and place it in your PATH.

Release assets:
- Linux: `timeshit-linux.tar.gz`
- macOS: `timeshit-macos.tar.gz`
- Windows: `timeshit-windows.zip`

### Build from Source
```bash
cargo build --release
```
Binary: `target/release/timeshit`

## Updates
- **GitHub Releases installs** use the in-app updater.
- **Package manager installs** should be updated via the package manager (brew/apt/choco/scoop/winget).

## Authentication
Use the login flow or set an environment variable:
```bash
timeshit login
```
or
```bash
export TOGGL_API_TOKEN="your-token"
```
Token file: `~/.toggl2tsc`

## Usage
```bash
timeshit
```

Optional flags:
```bash
timeshit --date YYYY-MM-DD
timeshit --start-date YYYY-MM-DD --end-date YYYY-MM-DD
```

## Keybinds (core)
- `h` help
- `c` copy client entries
- `v` copy project entries
- `x` copy entries with client + project
- `d` set date range
- `y` yesterday
- `r` refresh
- Arrow keys to navigate projects

## Docs
Full documentation lives in `docs/` and is published via GitHub Pages.

## License
MIT
