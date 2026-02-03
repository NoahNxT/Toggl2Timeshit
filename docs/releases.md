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
- Builds release binaries for **Linux, macOS, Windows**.
- Creates a GitHub Release with tag `v<version>`.
- Attaches packaged binaries to the release.
