[CmdletBinding()]
param(
    [switch]$Visible,
    [string]$IpcDir,
    [ValidateSet('dx12', 'vulkan')]
    [string]$RenderBackend
)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($IpcDir)) {
    $IpcDir = Join-Path $root 'runtime\talos-ipc'
}
$binary = Join-Path $root 'bin\daedalus.exe'
if (-not (Test-Path -LiteralPath $binary)) {
    throw "Simulator executable is missing: $binary"
}

New-Item -ItemType Directory -Force -Path $IpcDir | Out-Null
$env:TALOS_IPC_DIR = $IpcDir
$launchMode = if ($Visible) { 'visible' } else { 'performance' }
if ([string]::IsNullOrWhiteSpace($RenderBackend)) {
    # DX12 remains the measured high-performance backend. On the validated
    # Windows/NVIDIA configuration, resizing the visible DX12 swap chain can
    # fail in wgpu with `ResizeBuffers ... window is in use`; Vulkan does not.
    $RenderBackend = if ($Visible) { 'vulkan' } else { 'dx12' }
}
$env:WGPU_BACKEND = $RenderBackend
$env:WGPU_POWER_PREF = 'high'
$env:DAEDALUS_RELEASE_MODE = $launchMode
$env:PATH = "$(Join-Path $root 'bin');$env:PATH"

Write-Host "Daedalus launch mode=$launchMode backend=$RenderBackend ipc=$IpcDir"
& $binary
