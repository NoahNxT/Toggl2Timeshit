$ErrorActionPreference = 'Stop'

$packageName = $env:ChocolateyPackageName
$toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$version = '@VERSION@'

$zipUrl = "https://github.com/NoahNxT/Toggl2Timeshit/releases/download/v$version/timeshit-windows.zip"
$checksum = '@CHECKSUM@'

Install-ChocolateyZipPackage `
  -PackageName $packageName `
  -Url $zipUrl `
  -Checksum $checksum `
  -ChecksumType 'sha256' `
  -UnzipLocation $toolsDir
