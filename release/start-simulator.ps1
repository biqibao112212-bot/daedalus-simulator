[CmdletBinding()]
param(
    [switch]$Visible,
    [string]$IpcDir,
    [string]$CornerLabelsJsonl,
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
if (-not [string]::IsNullOrWhiteSpace($CornerLabelsJsonl)) {
    $CornerLabelsJsonl = [IO.Path]::GetFullPath($CornerLabelsJsonl)
    if ([IO.Path]::GetExtension($CornerLabelsJsonl) -ne '.jsonl') {
        throw 'CornerLabelsJsonl must be an absolute .jsonl path.'
    }
    if (Test-Path -LiteralPath $CornerLabelsJsonl) {
        throw "Corner label output already exists: $CornerLabelsJsonl"
    }
    $cornerParent = Split-Path -Parent $CornerLabelsJsonl
    if (-not (Test-Path -LiteralPath $cornerParent -PathType Container)) {
        throw "Corner label parent directory does not exist: $cornerParent"
    }
    $packageRoot = [IO.Path]::GetFullPath($root).TrimEnd('\')
    if ($CornerLabelsJsonl.StartsWith($packageRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Corner label output must be outside the installed Release tree.'
    }
    $env:DAEDALUS_CORNER_LABELS_JSONL = $CornerLabelsJsonl
} else {
    Remove-Item Env:DAEDALUS_CORNER_LABELS_JSONL -ErrorAction SilentlyContinue
}
$env:PATH = "$(Join-Path $root 'bin');$env:PATH"

Write-Host "Daedalus launch mode=$launchMode backend=$RenderBackend ipc=$IpcDir"
& $binary
