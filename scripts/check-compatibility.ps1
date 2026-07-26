[CmdletBinding()]
param([string]$ConsumerLock)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$release = Get-Content -LiteralPath (Join-Path $root 'release\release.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$contract = Get-Content -LiteralPath (Join-Path $root 'sdk\contract.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$matrix = Get-Content -LiteralPath (Join-Path $root 'release\platform-matrix.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$calibration = Get-Content -LiteralPath (Join-Path $root 'release\camera-calibration.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$sdkHeader = Get-Content -LiteralPath (Join-Path $root 'sdk\cpp\include\daedalus_sim_sdk\talos_v1.hpp') -Raw -Encoding UTF8
$requiredCapabilities = @(
    'image_identity',
    'fixed_calibration',
    'gimbal_command',
    'gimbal_actual_state',
    'frame_synchronized_gimbal_state',
    'exposure_pose_by_frame',
    'tcp_image',
    'scene_control',
    'runtime_capabilities'
)

if ($release.sdk_version -ne $contract.sdk_version -or
    $release.shm_version -ne $contract.shm_version -or
    $release.sdk_abi_revision -ne $contract.sdk_abi_revision -or
    $release.image.width -ne $contract.image_width -or
    $release.image.height -ne $contract.image_height -or
    $release.image.format -ne $contract.tcp_image_default_format -or
    $release.image.channels -ne 4 -or
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
if ($sdkHeader -notmatch ('kSdkVersion\s*=\s*"' + [regex]::Escape($contract.sdk_version) + '"') -or
    $sdkHeader -notmatch ('kSdkAbiRevision\s*=\s*' + $contract.sdk_abi_revision + '\s*;')) {
    throw 'SDK public header version/ABI constants disagree with sdk/contract.json.'
}
if ($release.ballistics.projectile_speed_mps -le 0 -or
    $release.ballistics.gravity_world_mps2.Count -ne 3 -or
    $release.gimbal.command_mode -ne 'absolute_chassis_local' -or
    $release.gimbal.pitch_level_deg -ne 90 -or
    $release.gimbal.command_stale_timeout_ms -ne 250) {
    throw 'Release ballistics/gimbal constants are missing or invalid.'
}
if ($calibration.schema_version -ne 1 -or
    $calibration.read_only -ne $true -or
    $calibration.runtime_setter_available -ne $false -or
    $calibration.image.width -ne $contract.image_width -or
    $calibration.image.height -ne $contract.image_height -or
    $calibration.intrinsics.fx -ne $calibration.intrinsics.fy -or
    [string]::IsNullOrWhiteSpace($calibration.calibration_id)) {
    throw 'Fixed camera calibration is missing, mutable, or inconsistent with the SDK image contract.'
}
if ($matrix.schema_version -ne 1 -or
    $matrix.architecture.name -ne 'x86_64' -or
    $matrix.architecture.family -ne 'x86' -or
    $matrix.targets.Count -ne 2) {
    throw 'Platform matrix must define exactly the Windows/Linux x86_64 release targets.'
}
foreach ($target in $matrix.targets) {
    if ($target.os -notin @('windows', 'linux') -or
        $target.rust_target -notin @('x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu')) {
        throw "Unsupported platform target in release matrix: $($target.id)."
    }
}
if ($matrix.gpu_and_inference_boundary.simulator_bundles_inference_runtime -ne $false -or
    $matrix.gpu_and_inference_boundary.inference_owner -ne 'consumer') {
    throw 'GPU inference must remain an external consumer-owned runtime.'
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
