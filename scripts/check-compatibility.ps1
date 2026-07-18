[CmdletBinding()]
param([string]$ConsumerLock)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$release = Get-Content -LiteralPath (Join-Path $root 'release\release.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$contract = Get-Content -LiteralPath (Join-Path $root 'sdk\contract.json') -Raw -Encoding UTF8 | ConvertFrom-Json

if ($release.sdk_version -ne $contract.sdk_version -or
    $release.shm_version -ne $contract.shm_version -or
    $release.image.width -ne $contract.image_width -or
    $release.image.height -ne $contract.image_height) {
    throw 'Simulator release.json and sdk/contract.json disagree.'
}

if (-not [string]::IsNullOrWhiteSpace($ConsumerLock)) {
    $lock = Get-Content -LiteralPath $ConsumerLock -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($lock.simulator.version -ne $release.version -or
        $lock.simulator.sdk_version -ne $release.sdk_version -or
        $lock.simulator.shm_version -ne $release.shm_version -or
        $lock.simulator.image.width -ne $release.image.width -or
        $lock.simulator.image.height -ne $release.image.height) {
        throw "Consumer lock is incompatible with simulator $($release.version)."
    }
}

"compatible simulator=$($release.version) sdk=$($release.sdk_version) shm=$($release.shm_version) image=$($release.image.width)x$($release.image.height)"
