# Distribution & Package Managers

Timeshit publishes official release assets on GitHub Releases and can optionally publish to multiple package managers.

## Homebrew (macOS/Linux)
Hosted in the combined packages repo:
```
https://github.com/NoahNxT/nxt-solutions-packages
```

Install:
```bash
brew tap NoahNxT/nxt-solutions-packages
brew install timeshit
```

## Scoop (Windows)
Same combined packages repo:
```powershell
scoop bucket add nxt-solutions https://github.com/NoahNxT/nxt-solutions-packages
scoop install timeshit
```

## Chocolatey (Windows)
Requires Chocolatey.org account + API key.

Automation uses:
- `packaging/chocolatey/timeshit.nuspec`
- `packaging/chocolatey/tools/chocolateyinstall.ps1`
- `packaging/chocolatey/tools/chocolateyuninstall.ps1`

On each release, a GitHub Action packs and pushes `timeshit.<version>.nupkg`.

## Winget (Windows)
Requires a GitHub token to open PRs against `microsoft/winget-pkgs`.

## APT / RPM (Linux)
Recommended: Cloudsmith repo and API key for publishing `.deb` and `.rpm`.

### APT (Cloudsmith) setup
1. Create a Cloudsmith account and a **Debian** repository.
2. Add repo variable:
   - `CLOUDSMITH_REPO` (e.g. `noahnxt/timeshit`)
3. Add repo secret:
   - `CLOUDSMITH_API_KEY`
4. On release, GitHub Actions builds a `.deb` and uploads it to Cloudsmith.

## AUR (Arch)
Requires AUR account and an SSH key for pushing package updates.

## Automation Notes
Homebrew + Scoop are fully automated via GitHub Actions on release.
Other managers can be added once credentials/accounts are available.

### Homebrew + Scoop automation setup
1. Create a PAT with **repo** access to `NoahNxT/nxt-solutions-packages`.
2. Add it as a secret in this repo: `PACKAGES_REPO_TOKEN`.
3. Run a release (`🔖 Release TUI`). The publish workflow will update the formula and Scoop manifest.

### Winget automation setup
1. Fork `microsoft/winget-pkgs` to your account.
2. Create a PAT with **public_repo** scope and add it as `WINGET_TOKEN`.
3. Set repo variables:
   - `WINGET_PACKAGE_ID` (`NxTSolutions.Timeshit`)
   - `WINGET_PACKAGE_NAME` (e.g. `Timeshit`)
4. Submit the **first** Winget manifest manually (required by Winget).
5. Releases will auto-open PRs for updates.
