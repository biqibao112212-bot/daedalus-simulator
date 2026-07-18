[CmdletBinding()]
param([string]$ConsumerLock)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$release = Get-Content -LiteralPath (Join-Path $root 'release\release.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$contract = Get-Content -LiteralPath (Join-Path $root 'sdk\contract.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$requiredCapabilities = @(
    'image_identity',
    'camera_intrinsics',
    'exposure_pose',
    'chassis_observation',
    'armor_ground_truth',
    'rune_ground_truth',
    'gimbal_command',
    'tcp_image',
    'scene_control'
)

if ($release.sdk_version -ne $contract.sdk_version -or
    $release.shm_version -ne $contract.shm_version -or
    $release.sdk_abi_revision -ne $contract.sdk_abi_revision -or
    $release.image.width -ne $contract.image_width -or
    $release.image.height -ne $contract.image_height -or
    $release.scene_control_protocol -ne $contract.scene_control_protocol -or
    $release.scene_control_port -ne $contract.scene_control_port) {
    throw 'Simulator release.json and sdk/contract.json disagree.'
}

foreach ($capability in $requiredCapabilities) {
    if ($contract.capabilities -notcontains $capability) {
        throw "SDK contract is missing required capability: $capability"
    }
}
if ($contract.scene_control_protocol -ne 1 -or $contract.scene_control_port -ne 5603) {
    throw 'SDK scene-control protocol/port contract is invalid.'
}

if (-not [string]::IsNullOrWhiteSpace($ConsumerLock)) {
    $lock = Get-Content -LiteralPath $ConsumerLock -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($lock.simulator.version -ne $release.version -or
        $lock.simulator.sdk_version -ne $release.sdk_version -or
        $lock.simulator.shm_version -ne $release.shm_version -or
        $lock.simulator.sdk_abi_revision -ne $release.sdk_abi_revision -or
        $lock.simulator.image.width -ne $release.image.width -or
        $lock.simulator.image.height -ne $release.image.height -or
        $lock.simulator.scene_control_protocol -ne $release.scene_control_protocol -or
        $lock.simulator.scene_control_port -ne $release.scene_control_port) {
        throw "Consumer lock is incompatible with simulator $($release.version)."
    }
}

"compatible simulator=$($release.version) sdk=$($release.sdk_version) shm=$($release.shm_version) abi=$($release.sdk_abi_revision) image=$($release.image.width)x$($release.image.height) scene_control=$($release.scene_control_protocol)"
