[CmdletBinding()]
param(
    [switch]$Visible,
    [string]$IpcDir
)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($IpcDir)) {
    $workspace = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $root))
    $IpcDir = Join-Path $workspace 'runtime\talos-ipc'
}
$binary = Join-Path $root 'bin\daedalus.exe'
if (-not (Test-Path -LiteralPath $binary)) {
    throw "Simulator executable is missing: $binary"
}

New-Item -ItemType Directory -Force -Path $IpcDir | Out-Null
$env:BEVY_ASSET_ROOT = $root
$env:TALOS_IPC_DIR = $IpcDir
$env:WGPU_BACKEND = 'dx12'
$env:WGPU_POWER_PREF = 'high'
$env:DAEDALUS_CONFIG = 'config.performance.toml'
$env:DAEDALUS_TALOS_RGB_ONLY = '1'
$env:DAEDALUS_TALOS_CAPTURE_MAX_HZ = '200'
$env:DAEDALUS_TALOS_IMAGE_TRANSPORT = 'tcp'
$env:DAEDALUS_AUTO_AIM_ON_START = '1'
$env:PATH = "$(Join-Path $root 'bin');$env:PATH"

if ($Visible) {
    Remove-Item Env:DAEDALUS_PERF_DISABLE_UI -ErrorAction SilentlyContinue
    $env:DAEDALUS_PREVIEW_ENABLED = '1'
    $env:DAEDALUS_PREVIEW_MAX_HZ = '60'
} else {
    $env:DAEDALUS_PERF_DISABLE_UI = '1'
    Remove-Item Env:DAEDALUS_PREVIEW_ENABLED -ErrorAction SilentlyContinue
    Remove-Item Env:DAEDALUS_PREVIEW_MAX_HZ -ErrorAction SilentlyContinue
}

& $binary
