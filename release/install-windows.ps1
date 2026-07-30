[CmdletBinding()]
param(
    [string]$InstallDir,
    [switch]$NoShortcuts,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$version = '1.1.1'
$source = [IO.Path]::GetFullPath($PSScriptRoot).TrimEnd('\')
if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $InstallDir = Join-Path $env:LOCALAPPDATA "DaedalusSimulator\$version"
}
$destination = [IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
$separator = [IO.Path]::DirectorySeparatorChar
$root = [IO.Path]::GetPathRoot($destination).TrimEnd('\')
if ($destination -eq $root -or
    $destination -eq $source -or
    $destination.StartsWith($source + $separator, [StringComparison]::OrdinalIgnoreCase) -or
    $source.StartsWith($destination + $separator, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Unsafe install directory: $destination"
}
if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'Daedalus Simulator requires 64-bit Windows.'
}

if (Test-Path -LiteralPath $destination) {
    $marker = Join-Path $destination 'release.json'
    if (-not (Test-Path -LiteralPath $marker)) {
        throw "Existing directory is not a Daedalus installation: $destination"
    }
    $installed = Get-Content -Raw -LiteralPath $marker | ConvertFrom-Json
    if ($installed.product -ne 'daedalus-simulator') {
        throw "Existing directory has an unexpected release marker: $destination"
    }
    if (-not $Force) {
        $answer = Read-Host "Daedalus Simulator already exists at $destination. Replace it? [y/N]"
        if ($answer -notin @('y','Y','yes','YES')) {
            Write-Host 'Installation cancelled.'
            exit 2
        }
    }
    Remove-Item -LiteralPath $destination -Recurse -Force
}

New-Item -ItemType Directory -Force -Path $destination | Out-Null
Get-ChildItem -LiteralPath $source -Force | Copy-Item -Destination $destination -Recurse -Force
if (-not (Test-Path -LiteralPath (Join-Path $destination 'bin\daedalus.exe'))) {
    throw 'Installed simulator executable is missing.'
}

if (-not $NoShortcuts) {
    $shortcutDirectory = Join-Path ([Environment]::GetFolderPath('Programs')) 'Daedalus Simulator'
    New-Item -ItemType Directory -Force -Path $shortcutDirectory | Out-Null
    $shell = New-Object -ComObject WScript.Shell
    foreach ($entry in @(
        @{Name='Daedalus Simulator'; Extra=''},
        @{Name='Daedalus Simulator (Visible)'; Extra=' -Visible'}
    )) {
        $shortcut = $shell.CreateShortcut((Join-Path $shortcutDirectory ($entry.Name + '.lnk')))
        $shortcut.TargetPath = Join-Path $PSHOME 'powershell.exe'
        $launcher = Join-Path $destination 'start-simulator.ps1'
        $shortcut.Arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$launcher`"$($entry.Extra)"
        $shortcut.WorkingDirectory = $destination
        $shortcut.IconLocation = (Join-Path $destination 'bin\daedalus.exe') + ',0'
        $shortcut.Save()
    }
}

$runtime = Join-Path $env:WINDIR 'System32\VCRUNTIME140.dll'
if (-not (Test-Path -LiteralPath $runtime)) {
    Write-Warning 'Microsoft Visual C++ v14 x64 Runtime is missing.'
    Write-Warning 'Install it from https://aka.ms/vc14/vc_redist.x64.exe'
}
$adapters = @(Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
    Select-Object -ExpandProperty Name)

Write-Host "Installed Daedalus Simulator $version"
Write-Host "Location: $destination"
if ($adapters.Count -gt 0) { Write-Host "Graphics adapters: $($adapters -join '; ')" }
Write-Host "Read first: $(Join-Path $destination 'README_ZH.md')"
