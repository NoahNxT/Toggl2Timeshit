# Distribution & Package Managers

Timeshit publishes official release assets on GitHub Releases and can optionally publish to multiple package managers.

## Homebrew (macOS/Linux)
Homebrew repo:
```
https://github.com/NoahNxT/homebrew-nxt-solutions-packages
```

Install:
```bash
brew tap NoahNxT/nxt-solutions-packages
brew install timeshit
```
Upgrade:
```bash
brew upgrade timeshit
```
If you previously installed the legacy Node package, the old binary may conflict:
```bash
rm /opt/homebrew/bin/timeshit
brew link --overwrite timeshit
```
You can also remove the legacy global npm package:
```bash
npm remove -g toggl2timeshit
```

## Scoop (Windows)
Scoop bucket repo:
```powershell
scoop bucket add nxt-solutions https://github.com/NoahNxT/scoop-nxt-solutions-packages
scoop install timeshit
```

## Chocolatey (Windows)
Requires Chocolatey.org account + API key.

Automation uses:
- `packaging/chocolatey/timeshit.nuspec`
- `packaging/chocolatey/tools/chocolateyinstall.ps1`
- `packaging/chocolatey/tools/chocolateyuninstall.ps1`

On each release, a GitHub Action packs and pushes `timeshit.<version>.nupkg`.

Install:
```powershell
choco install timeshit
```

Install a specific version:
```powershell
choco install timeshit --version=1.3.6
```

## Winget (Windows)
Requires a GitHub token to open PRs against `microsoft/winget-pkgs`.

## APT / RPM (Linux)
Recommended: Cloudsmith repo and API key for publishing `.deb` and `.rpm`.

### APT (Cloudsmith) setup
1. Create a Cloudsmith account and a **Debian** repository.
2. Add repo variable:
   - `CLOUDSMITH_REPO` (e.g. `nxt-solutions/timeshit`)
   - `CLOUDSMITH_DISTRO` (e.g. `debian` or `ubuntu`)
   - `CLOUDSMITH_RELEASE` (e.g. `bookworm` for Debian 12, `bullseye` for Debian 11, or `jammy` for Ubuntu 22.04)
3. Add repo secret:
   - `CLOUDSMITH_API_KEY`
4. On release, GitHub Actions builds a `.deb` and uploads it to Cloudsmith.

Tip: to see valid distro/release pairs for your repo, run:
```
cloudsmith list distros deb
```

### APT install (end users)
```bash
curl -1sLf 'https://dl.cloudsmith.io/public/nxt-solutions/timeshit/setup.deb.sh' | sudo -E bash
sudo apt-get update
sudo apt-get install timeshit
```

Troubleshooting (command not found):
```bash
dpkg -s timeshit
which timeshit
```
If `apt` can’t find the package, force the distro/codename to match what you publish:
```bash
curl -1sLf 'https://dl.cloudsmith.io/public/nxt-solutions/timeshit/setup.deb.sh' \
  | sudo -E distro=ubuntu codename=noble bash
```

Install a specific version:
```bash
sudo apt-get install timeshit=1.3.5-1
```

Force distro/codename (if needed):
```bash
curl -1sLf 'https://dl.cloudsmith.io/public/nxt-solutions/timeshit/setup.deb.sh' \
  | sudo -E distro=debian codename=bookworm bash
```

## AUR (Arch)
Requires AUR account and an SSH key for pushing package updates.

## Automation Notes
Homebrew + Scoop are fully automated via GitHub Actions on release.
Other managers can be added once credentials/accounts are available.

### Homebrew + Scoop automation setup
1. Create a PAT with **repo** access to:
   - `NoahNxT/homebrew-nxt-solutions-packages`
   - `NoahNxT/scoop-nxt-solutions-packages`
2. Add it as a secret in this repo: `PACKAGES_REPO_TOKEN`.
3. Run a release (`🔖 Release TUI`) or trigger:
   - `🍺 Publish Homebrew`
   - `🪣 Publish Scoop`

### Winget automation setup
1. Fork `microsoft/winget-pkgs` to your account.
2. Create a PAT with **public_repo** scope and add it as `WINGET_TOKEN`.
3. Set repo variables:
   - `WINGET_PACKAGE_ID` (`NxTSolutions.Timeshit`)
4. Run the **🪟 Publish Winget (Initial)** workflow with a release tag (e.g. `v1.3.6`).
   - This generates the manifest and opens the initial PR automatically.
5. After the initial PR is merged, releases will auto-open PRs for updates.
