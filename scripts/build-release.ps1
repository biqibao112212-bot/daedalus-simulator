[CmdletBinding()]
param(
    [ValidateSet('windows', 'linux')]
    [string]$Platform = 'windows',
    [ValidateSet('x86_64')]
    [string]$Arch = 'x86_64',
    [string]$RustTarget,
    [switch]$SkipSimulator,
    [switch]$SkipSdk
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
if ($Platform -ne 'windows') {
    throw 'Linux release builds must run scripts/build-release.sh on an x86_64 Linux/WSL environment. PowerShell does not cross-compile the Linux SDK here.'
}

if (-not $SkipSimulator) {
    cargo build --locked --release --features talos,distribution-release --target $RustTarget
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

if (-not $SkipSdk) {
    $cmake = Get-Command cmake -ErrorAction SilentlyContinue
    $ctest = Get-Command ctest -ErrorAction SilentlyContinue
    if ($null -eq $cmake -or $null -eq $ctest) {
        throw 'Windows SDK release builds require cmake and ctest on PATH. Install a Windows CMake toolchain, then rerun this script.'
    }

    $packageId = "$Platform-$Arch"
    $buildRoot = Join-Path $root "build\release\$packageId"
    $sdkBuild = Join-Path $buildRoot 'sdk-build'
    $sdkInstall = Join-Path $buildRoot 'sdk-install'
    New-Item -ItemType Directory -Force -Path $buildRoot | Out-Null

    cmake -S (Join-Path $root 'sdk\cpp') -B $sdkBuild `
        '-DCMAKE_BUILD_TYPE=Release' `
        '-DBUILD_TESTING=ON' `
        "-DCMAKE_INSTALL_PREFIX=$sdkInstall"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    # Visual Studio is a multi-config generator; CMAKE_BUILD_TYPE alone does
    # not select Release for build, test, or install.
    cmake --build $sdkBuild --config Release --parallel
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    ctest --test-dir $sdkBuild -C Release --output-on-failure
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cmake --install $sdkBuild --config Release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Output "rust_target=$RustTarget"
Write-Output "binary=$(Join-Path $root "target\$RustTarget\release\daedalus.exe")"
Write-Output "sdk_install=$(Join-Path $root "build\release\windows-$Arch\sdk-install")"
