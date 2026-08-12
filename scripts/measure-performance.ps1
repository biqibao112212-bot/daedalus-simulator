[CmdletBinding()]
param(
    [string]$BinaryPath,
    [string]$ConfigPath,
    [string]$OutputDirectory,
    [switch]$Build,
    [ValidateRange(10, 600)]
    [int]$DurationSeconds = 20,
    [ValidateRange(1, 1000)]
    [double]$MinimumMainUpdateHz = 100,
    [ValidateRange(1, 1000)]
    [double]$MinimumCaptureSubmitHz = 100
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($BinaryPath)) { $BinaryPath = Join-Path $root 'target\x86_64-pc-windows-msvc\release\daedalus.exe' }
if ([string]::IsNullOrWhiteSpace($ConfigPath)) { $ConfigPath = Join-Path $root 'config.performance.toml' }
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) { $OutputDirectory = Join-Path (Split-Path -Parent (Split-Path -Parent $root)) 'runtime\simulator-performance' }

$BinaryPath = [IO.Path]::GetFullPath($BinaryPath)
$ConfigPath = [IO.Path]::GetFullPath($ConfigPath)
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (-not (Test-Path -LiteralPath $ConfigPath -PathType Leaf)) { throw "Performance configuration is missing: $ConfigPath" }
if ((Split-Path -Leaf (Split-Path -Parent $BinaryPath)) -ne 'release') { throw "Performance measurements require an optimised Release binary under a release directory; refusing: $BinaryPath" }
if (Get-Process -Name daedalus -ErrorAction SilentlyContinue) { throw 'A Daedalus process is already running. Stop it explicitly before starting an isolated performance measurement.' }
if ($Build) {
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if ($null -eq $cargo) {
        $cargoCandidate = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
        if (Test-Path -LiteralPath $cargoCandidate -PathType Leaf) { $cargo = Get-Item -LiteralPath $cargoCandidate }
    }
    if ($null -eq $cargo) { throw 'Release build requested but cargo was not found on PATH or under USERPROFILE\.cargo\bin.' }
    & $cargo.Source build --locked --release --features talos,distribution-release --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
if (-not (Test-Path -LiteralPath $BinaryPath -PathType Leaf)) { throw "Simulator executable is missing: $BinaryPath" }

$stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
$runDirectory = Join-Path $OutputDirectory $stamp
New-Item -ItemType Directory -Force -Path $runDirectory | Out-Null
$statsPath = Join-Path $runDirectory 'simulator.stats.json'
$stdoutPath = Join-Path $runDirectory 'simulator.stdout.log'
$stderrPath = Join-Path $runDirectory 'simulator.stderr.log'
$ipcDirectory = Join-Path $runDirectory 'talos-ipc'
$savedEnvironment = @{}
foreach ($name in @('DAEDALUS_CONFIG', 'DAEDALUS_STATS_JSON', 'DAEDALUS_TALOS_IMAGE_TRANSPORT', 'DAEDALUS_TALOS_TCP_BIND', 'DAEDALUS_PERF_DISABLE_UI', 'DAEDALUS_CORNER_LABELS_JSONL', 'TALOS_PERFORMANCE_EVIDENCE_JSON', 'TALOS_IPC_DIR', 'WGPU_BACKEND', 'WGPU_POWER_PREF')) { $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$process = $null
try {
    $env:DAEDALUS_CONFIG = $ConfigPath
    Remove-Item Env:DAEDALUS_STATS_JSON -ErrorAction SilentlyContinue
    $env:TALOS_PERFORMANCE_EVIDENCE_JSON = $statsPath
    $env:DAEDALUS_TALOS_IMAGE_TRANSPORT = 'tcp'
    $env:DAEDALUS_TALOS_TCP_BIND = '127.0.0.1:5602'
    $env:DAEDALUS_PERF_DISABLE_UI = '1'
    Remove-Item Env:DAEDALUS_CORNER_LABELS_JSONL -ErrorAction SilentlyContinue
    $env:TALOS_IPC_DIR = $ipcDirectory
    $env:WGPU_BACKEND = 'dx12'
    $env:WGPU_POWER_PREF = 'high'
    $process = Start-Process -FilePath $BinaryPath -WorkingDirectory $root -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath -PassThru -WindowStyle Hidden
    Start-Sleep -Seconds $DurationSeconds
    $process.Refresh()
    if ($process.HasExited) { throw "Simulator exited early with code $($process.ExitCode). See $stderrPath" }
    if (-not (Test-Path -LiteralPath $statsPath -PathType Leaf)) { throw "Simulator did not write frequency statistics: $statsPath" }
    $metrics = Get-Content -LiteralPath $statsPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $evidence = [ordered]@{ schema = 'daedalus-performance-v1'; version = (Get-Content -LiteralPath (Join-Path $root 'VERSION') -Raw).Trim(); profile = 'release'; rust_target = 'x86_64-pc-windows-msvc'; features = @('talos', 'distribution-release'); corner_labels_enabled = $false; binary_sha256 = (Get-FileHash -LiteralPath $BinaryPath -Algorithm SHA256).Hash.ToLowerInvariant(); started_utc = $stamp; duration_seconds = $DurationSeconds; source_commit = (git -C $root rev-parse HEAD).Trim(); source_dirty = @((git -C $root status --porcelain -- .)).Count -ne 0; binary_path = $BinaryPath; config_path = $ConfigPath; metrics = $metrics }
    $evidencePath = Join-Path $runDirectory 'performance-evidence.json'
    $evidence | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $evidencePath -Encoding UTF8
    if ($metrics.talos_image_transport -ne 'tcp') { throw "Expected TCP image transport, got $($metrics.talos_image_transport)" }
    if ([double]$metrics.main_update_hz -lt $MinimumMainUpdateHz) { throw "main_update_hz $($metrics.main_update_hz) is below the required $MinimumMainUpdateHz Hz" }
    if ([double]$metrics.capture_copy_submit_hz -lt $MinimumCaptureSubmitHz) { throw "capture_copy_submit_hz $($metrics.capture_copy_submit_hz) is below the required $MinimumCaptureSubmitHz Hz" }
    Write-Output "performance_evidence=$evidencePath"
    Write-Output "main_update_hz=$($metrics.main_update_hz)"
    Write-Output "capture_copy_submit_hz=$($metrics.capture_copy_submit_hz)"
}
finally {
    if ($null -ne $process) { $process.Refresh(); if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force } }
    foreach ($name in $savedEnvironment.Keys) { [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process') }
}
