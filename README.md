# Toggl2Timeshit (Timeshit TUI)

Toggl2Timeshit is now a Rust-based Terminal UI (TUI) that turns Toggl Track time entries into a clean, navigable timesheet dashboard.

## Features
- TUI dashboard with project summaries and ticket details
- Workspace selection
- Date filters (single day or range)
- Total hours with visual status
- Secure token storage compatible with previous versions (`~/.toggl2tsc`)
- Persistent cache to minimize Toggl API usage (manual refresh only)
- Built-in settings for target hours and integrations

## Installation
### GitHub Releases
Download the latest release binary for your OS and place it in your PATH.

### Build from Source
```bash
cargo build --release
```
The binary will be available at `target/release/timeshit`.

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
