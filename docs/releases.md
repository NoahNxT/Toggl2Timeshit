# Releases

Releases are built and published via GitHub Actions.

## Release Workflow
Workflow file:
```
.github/workflows/release.yml
```

## How to Release
1. Push changes to `main`.
2. Go to **GitHub Actions** → **🔖 Release TUI**.
3. Click **Run workflow** and enter a version (e.g., `1.2.0`).

## What Happens
- The workflow updates `Cargo.toml` + `Cargo.lock` on `main` to the release version.
- A git tag `v<version>` is created and pushed.
- Builds release binaries for **Linux, macOS, Windows**.
- Creates a GitHub Release with tag `v<version>`.
- Attaches packaged binaries to the release.

## Release Assets
The workflow publishes:
- `timeshit-linux.tar.gz`
- `timeshit-macos.tar.gz`
- `timeshit-macos.pkg`
- `timeshit-windows.zip`

These assets are consumed by the Homebrew and Scoop automation.
