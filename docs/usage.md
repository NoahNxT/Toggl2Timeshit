# Usage & Keybinds

## Navigation
- `Up/Down`: Select project
- `Enter`: Browse entries (dashboard) / Select workspace (workspace picker)
- `Right` / `Tab`: Switch to entries (dashboard)
- `Left` / `Shift+Tab`: Switch to projects (dashboard)
- `Esc`: Back to projects (from entries) / Close modal
- `q`: Quit

## Entries (Dashboard)
- `Up/Down`: Select entry (when browsing entries)
- `b`: Copy selected entry title
- `n`: Copy selected entry hours

## Dates
- `t`: Today
- `y`: Yesterday
- `d`: Open date range modal
- `[` / `]`: Shift current active date range backward/forward
- `Tab`: Switch between start/end in date range modal

## Rollups
- `o`: Open rollups view
- `w`: Weekly rollups
- `m`: Monthly rollups
- `y`: Yearly rollups
- `Tab`: Switch focus between periods and days
- `Up/Down`: Navigate periods or days
- `Left/Right`: Move one step in period/day lists
- `Shift+R`: Refetch selected day/week/month/year from Toggl API
- `Esc`: Back to dashboard

## Clipboard
- `c`: Copy **all entries for the selected client**
- `v`: Copy **entries for the selected project**
- `x`: Copy entries with **client + project + entry** and **total hours**

Clipboard format:
```
• Client — Project — Entry (2.50h)

Total hours: 8.00h
```

## Refresh & Cache
- `r`: Manual refresh (API call if quota allows)

## Help & Settings
- `h`: Help modal
- `s`: Settings modal
- `m`: Toggle light/dark theme

Settings → General includes optional **time rounding** (increment + mode).

## Status & Toasts
Short status messages appear in the footer and auto-hide after a few seconds. Copy actions also show a small toast in the dashboard.
