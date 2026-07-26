[CmdletBinding()]
param(
    [ValidateSet('windows', 'linux')]
    [string]$Platform = 'windows',
    [ValidateSet('x86_64')]
    [string]$Arch = 'x86_64',
    [string]$RustTarget,
    [string]$OutputRoot,
    [string]$SdkInstallRoot,
    [switch]$Force,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if ([string]::IsNullOrWhiteSpace($RustTarget)) {
    $RustTarget = switch ($Platform) {
        'windows' { 'x86_64-pc-windows-msvc' }
        'linux' { 'x86_64-unknown-linux-gnu' }
    }
}
if ($Platform -eq 'linux') {
    throw 'Use scripts/package-release.sh for the Linux package so the binary and SDK are built by the Linux toolchain.'
}

if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $workspace = Split-Path -Parent (Split-Path -Parent $root)
    $OutputRoot = Join-Path $workspace 'releases\daedalus-simulator'
}
$OutputRoot = [IO.Path]::GetFullPath($OutputRoot).TrimEnd('\')
$packageId = "$Platform-$Arch"
$version = (Get-Content -LiteralPath (Join-Path $root 'VERSION') -Raw).Trim()
$target = [IO.Path]::GetFullPath((Join-Path $OutputRoot "$version\$packageId")).TrimEnd('\')
$zip = "$target.zip"
if (-not $target.StartsWith($OutputRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw "Unsafe release target: $target"
}

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'build-release.ps1') -Platform $Platform -Arch $Arch -RustTarget $RustTarget
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$dirty = @(git -C $root status --porcelain -- . ':(exclude)release/COMMERCIAL_LICENSE.txt')
if ($dirty.Count -ne 0) {
    throw 'Release packaging requires a clean committed worktree.'
}
$sourceCommit = (git -C $root rev-parse HEAD).Trim()
$commercialLicense = Join-Path $root 'release\COMMERCIAL_LICENSE.txt'
if (-not (Test-Path -LiteralPath $commercialLicense)) {
    throw 'Closed-source packaging is blocked: provide an approved release/COMMERCIAL_LICENSE.txt after legal review.'
}
if ((Test-Path -LiteralPath $target) -or (Test-Path -LiteralPath $zip)) {
    if (-not $Force) { throw "Release $version/$packageId already exists. Use -Force to replace it." }
    if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target -Recurse -Force }
    if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
}

if ([string]::IsNullOrWhiteSpace($SdkInstallRoot)) {
    $SdkInstallRoot = Join-Path $root "build\release\$packageId\sdk-install"
}
$binary = Join-Path $root "target\$RustTarget\release\daedalus.exe"
if (-not (Test-Path -LiteralPath $binary)) { throw "Simulator executable is missing: $binary" }
if (-not (Test-Path -LiteralPath $SdkInstallRoot)) { throw "SDK install tree is missing: $SdkInstallRoot" }

New-Item -ItemType Directory -Force -Path $target,(Join-Path $target 'bin'),(Join-Path $target 'docs'),(Join-Path $target 'sdk') | Out-Null
Copy-Item -LiteralPath $binary -Destination (Join-Path $target 'bin\daedalus.exe')
Get-ChildItem -LiteralPath (Join-Path $root "target\$RustTarget\release\deps") -Filter '*.dll' -File -ErrorAction SilentlyContinue |
    Copy-Item -Destination (Join-Path $target 'bin')
Copy-Item -LiteralPath (Join-Path $root 'assets') -Destination (Join-Path $target 'assets') -Recurse
Copy-Item -LiteralPath `
    (Join-Path $root 'release\release.json'),
    (Join-Path $root 'release\platform-matrix.json'),
    (Join-Path $root 'release\camera-calibration.json') `
    -Destination $target
Copy-Item -LiteralPath (Join-Path $root 'release\start-simulator.ps1') -Destination $target
Copy-Item -LiteralPath $commercialLicense -Destination (Join-Path $target 'LICENSE.txt')
Copy-Item -LiteralPath `
    (Join-Path $root 'SIMULATOR_PERFORMANCE.md'),
    (Join-Path $root 'SIMULATOR_TROUBLESHOOTING.md'),
    (Join-Path $root 'RELEASE.md'),
    (Join-Path $root 'release\PLATFORM_SUPPORT.md'),
    (Join-Path $root 'release\LEGAL_RELEASE_GATE.md'),
    (Join-Path $root 'docs\SIMULATOR_USER_GUIDE_ZH.md'),
    (Join-Path $root 'docs\RELEASE_PROGRESS_ZH.md'),
    (Join-Path $root 'sdk\README.md') `
    -Destination (Join-Path $target 'docs')
Copy-Item -LiteralPath (Join-Path $root 'sdk\contract.json') -Destination (Join-Path $target 'docs\sdk-contract.json')
Get-ChildItem -LiteralPath $SdkInstallRoot -Force | Copy-Item -Destination (Join-Path $target 'sdk') -Recurse -Force
$forbiddenInferenceFiles = @(Get-ChildItem -LiteralPath $target -Recurse -File | Where-Object {
    $_.Name -match '(?i)(cuda|cudnn|tensorrt|onnx|\.engine$|\.plan$|\.trt$|\.pt$|\.pth$|\.safetensors$|\.ckpt$|\.tflite$|\.pb$|\.mlmodel$|checkpoint)'
})
if ($forbiddenInferenceFiles.Count -ne 0) {
    throw "Simulator package contains forbidden inference payloads: $($forbiddenInferenceFiles.FullName -join ', ')"
}

$forbiddenSourceFiles = @(Get-ChildItem -LiteralPath $target -Recurse -File | Where-Object {
    $_.Extension -in @('.rs', '.cpp', '.cc', '.cxx', '.pdb') -or $_.Name -in @('Cargo.toml', 'Cargo.lock')
})
if ($forbiddenSourceFiles.Count -ne 0) {
    throw "Simulator package contains forbidden source/debug files: $($forbiddenSourceFiles.FullName -join ', ')"
}

$entries = @(Get-ChildItem -LiteralPath $target -Recurse -File | Where-Object Name -ne 'release-manifest.json' | ForEach-Object {
    $relative = $_.FullName.Substring($target.Length).TrimStart('\').Replace('\','/')
    [ordered]@{
        path = $relative
        bytes = $_.Length
        sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
})
[ordered]@{
    schema_version = 1
    product = 'daedalus-simulator'
    version = $version
    package_id = $packageId
    rust_target = $RustTarget
    source_commit = $sourceCommit
    generated_at = (Get-Date).ToUniversalTime().ToString('o')
    files = $entries
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $target 'release-manifest.json') -Encoding UTF8

Compress-Archive -Path (Join-Path $target '*') -DestinationPath $zip -CompressionLevel Optimal
Write-Output "release_dir=$target"
Write-Output "release_zip=$zip"
