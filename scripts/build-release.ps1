[CmdletBinding()]
param([switch]$SkipSimulator)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if (-not $SkipSimulator) {
    cargo build --release --features talos
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$rootForWsl = $root.Replace('\','/')
$rootWsl = (& wsl.exe -d Ubuntu-OSTEP -- wslpath -a -u $rootForWsl).Trim()
$command = "cmake -S '$rootWsl/sdk/cpp' -B '$rootWsl/build/sim-sdk' -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON && cmake --build '$rootWsl/build/sim-sdk' --parallel && ctest --test-dir '$rootWsl/build/sim-sdk' --output-on-failure && cmake --install '$rootWsl/build/sim-sdk' --prefix '$rootWsl/build/sim-sdk-install'"
& wsl.exe -- bash -lc $command
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
