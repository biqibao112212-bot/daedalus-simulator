[CmdletBinding()]
param(
    [string]$OutputRoot,
    [switch]$Force,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $workspace = Split-Path -Parent (Split-Path -Parent $root)
    $OutputRoot = Join-Path $workspace 'releases\daedalus-simulator'
}
$dirty = @(git -C $root status --porcelain)
if ($dirty.Count -ne 0) {
    throw 'Release packaging requires a clean committed worktree.'
}
$sourceCommit = (git -C $root rev-parse HEAD).Trim()
$version = (Get-Content -LiteralPath (Join-Path $root 'VERSION') -Raw).Trim()
& (Join-Path $PSScriptRoot 'check-compatibility.ps1')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$target = [IO.Path]::GetFullPath((Join-Path $OutputRoot $version)).TrimEnd('\')
$output = [IO.Path]::GetFullPath($OutputRoot).TrimEnd('\')
$zip = "$target.zip"
if (-not $target.StartsWith($output + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw "Unsafe release target: $target"
}
if ((Test-Path -LiteralPath $target) -or (Test-Path -LiteralPath $zip)) {
    if (-not $Force) { throw "Release $version already exists. Use -Force to replace it." }
    if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target -Recurse -Force }
    if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
}

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'build-release.ps1')
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

New-Item -ItemType Directory -Force -Path $target,(Join-Path $target 'bin'),(Join-Path $target 'docs') | Out-Null
Copy-Item -LiteralPath (Join-Path $root 'target\release\daedalus.exe') -Destination (Join-Path $target 'bin\daedalus.exe')
Get-ChildItem -LiteralPath (Join-Path $root 'target\release\deps') -Filter '*.dll' -File -ErrorAction SilentlyContinue |
    Copy-Item -Destination (Join-Path $target 'bin')
Copy-Item -LiteralPath (Join-Path $root 'assets') -Destination (Join-Path $target 'assets') -Recurse
Copy-Item -LiteralPath (Join-Path $root 'config.toml'),(Join-Path $root 'config.performance.toml') -Destination $target
Copy-Item -LiteralPath (Join-Path $root 'release\start-simulator.ps1') -Destination $target
Copy-Item -LiteralPath (Join-Path $root 'release\release.json') -Destination $target
Copy-Item -LiteralPath (Join-Path $root 'sdk\contract.json') -Destination (Join-Path $target 'docs\sdk-contract.json')
Copy-Item -LiteralPath `
    (Join-Path $root 'SIMULATOR_PERFORMANCE.md'),`
    (Join-Path $root 'SIMULATOR_TROUBLESHOOTING.md'),`
    (Join-Path $root 'sdk\README.md'),`
    (Join-Path $root 'agent-team\SIMULATOR_INTERFACE.md'),`
    (Join-Path $root 'agent-team\SCENARIO_CONTROL.md') `
    -Destination (Join-Path $target 'docs')

$targetForWsl = $target.Replace('\','/')
$rootForWsl = $root.Replace('\','/')
$targetWsl = (& wsl.exe -d Ubuntu-OSTEP -- wslpath -a -u $targetForWsl).Trim()
$rootWsl = (& wsl.exe -d Ubuntu-OSTEP -- wslpath -a -u $rootForWsl).Trim()
& wsl.exe -- bash -lc "cmake --install '$rootWsl/build/sim-sdk' --prefix '$targetWsl/sdk'"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$entries = Get-ChildItem -LiteralPath $target -Recurse -File | ForEach-Object {
    $relative = $_.FullName.Substring($target.Length).TrimStart('\').Replace('\','/')
    [pscustomobject]@{
        path = $relative
        bytes = $_.Length
        sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
    }
}
[pscustomobject]@{version=$version; source_commit=$sourceCommit; generated_at=(Get-Date).ToUniversalTime().ToString('o'); files=$entries} |
    ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $target 'release-manifest.json') -Encoding UTF8
Compress-Archive -Path (Join-Path $target '*') -DestinationPath $zip -CompressionLevel Optimal
"release_dir=$target"
"release_zip=$zip"
