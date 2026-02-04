$ErrorActionPreference = 'Stop'

$toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$exePath = Join-Path $toolsDir 'timeshit.exe'

if (Test-Path $exePath) {
  Remove-Item $exePath -Force
}
