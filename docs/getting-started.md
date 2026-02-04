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
   ```bash
   timeshit login
   ```
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

On launch, Timeshit checks GitHub Releases for updates. If a newer version is found, you must install it to continue. If the update check or download fails (offline/GitHub down), the app shows a warning and continues. After a successful update, the app exits and should be relaunched.

Optional date flags:
```bash
timeshit --date YYYY-MM-DD
timeshit --start-date YYYY-MM-DD --end-date YYYY-MM-DD
```

## First Launch Flow
1. Login screen (if no token is available)
2. Workspace selection (if multiple workspaces)
3. Dashboard with cached or live data
