# Getting Started

## Install

### GitHub Releases
Download the latest `timeshit` binary for your OS from GitHub Releases and put it on your PATH.

### Build from Source
```bash
cargo build --release
```
Binary output:
```
target/release/timeshit
```

## Authenticate

You can authenticate in three ways:

1. **In-app login**
   - Open Settings → Integrations and paste your Toggl token.
2. **Environment variable**
   ```bash
   export TOGGL_API_TOKEN="your-token"
   ```
3. **Token file (compatible with old CLI)**
   ```
   ~/.toggl2tsc
   ```

## Run
```bash
timeshit
```

On launch, Timeshit checks GitHub Releases for updates. If a newer version is found, the app shows an alert; press `u` to install. If the update check fails (offline/GitHub down), the app shows a warning and continues. After a successful update, the app exits and should be relaunched.

Date ranges are selected inside the TUI (`d`).

## First Launch Flow
1. Login screen (if no token is available)
2. Workspace selection (if multiple workspaces)
3. Dashboard with cached or live data
