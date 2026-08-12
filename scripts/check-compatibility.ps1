[CmdletBinding()]
param([string]$ConsumerLock)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$release = Get-Content -LiteralPath (Join-Path $root 'release\release.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$contract = Get-Content -LiteralPath (Join-Path $root 'sdk\contract.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$matrix = Get-Content -LiteralPath (Join-Path $root 'release\platform-matrix.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$calibration = Get-Content -LiteralPath (Join-Path $root 'release\camera-calibration.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$sdkHeader = Get-Content -LiteralPath (Join-Path $root 'sdk\cpp\include\daedalus_sim_sdk\talos_v1.hpp') -Raw -Encoding UTF8
$endpointsHeader = Get-Content -LiteralPath (Join-Path $root 'sdk\cpp\include\daedalus_sim_sdk\endpoints_v1.hpp') -Raw -Encoding UTF8
$version = (Get-Content -LiteralPath (Join-Path $root 'VERSION') -Raw -Encoding UTF8).Trim()
$cargoManifest = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw -Encoding UTF8
$cmakeManifest = Get-Content -LiteralPath (Join-Path $root 'sdk\cpp\CMakeLists.txt') -Raw -Encoding UTF8
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
    $release.tcp_image_port -ne $contract.tcp_image_port -or
    $release.udp_command_port -ne $contract.udp_command_port -or
    $release.scene_control_protocol -ne $contract.scene_control_protocol -or
    $release.scene_control_port -ne $contract.scene_control_port) {
    throw 'Simulator release.json and sdk/contract.json disagree.'
}
if ($release.version -ne $version -or
    $cargoManifest -notmatch ('(?m)^version\s*=\s*"' + [regex]::Escape($version) + '"\s*$') -or
    $cmakeManifest -notmatch ('project\(DaedalusSimSdk VERSION ' + [regex]::Escape($contract.sdk_version) + '\s')) {
    throw 'VERSION, Cargo package, release contract, and SDK CMake versions disagree.'
}

foreach ($capability in $requiredCapabilities) {
    if ($contract.capabilities -notcontains $capability) {
        throw "SDK contract is missing required capability: $capability"
    }
}
if ($contract.scene_control_protocol -ne 2 -or $contract.scene_control_port -ne 5603) {
    throw 'SDK scene-control protocol/port contract is invalid.'
}
if ($contract.tcp_image_protocol -ne 1 -or
    $contract.tcp_image_port -ne 5602 -or
    $contract.udp_command_port -ne 5601 -or
    $endpointsHeader -notmatch 'kTcpImagePort\s*=\s*5602\s*;' -or
    $endpointsHeader -notmatch 'kUdpCommandPort\s*=\s*5601\s*;' -or
    $endpointsHeader -notmatch 'kUdpSceneControlPort\s*=\s*5603\s*;') {
    throw 'TCP/UDP protocol or public endpoint constants changed unexpectedly.'
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
$releaseExport = $release.offline_exports.exact_corner_labels
$contractExport = $contract.offline_exports.exact_corner_labels
if ($null -eq $releaseExport -or $null -eq $contractExport -or
    $releaseExport.schema_version -ne 'daedalus.offline-exact-corners/1' -or
    $releaseExport.schema_version -ne $contractExport.schema_version -or
    $releaseExport.schema_file -ne $contractExport.schema_file -or
    $releaseExport.environment_variable -ne 'DAEDALUS_CORNER_LABELS_JSONL' -or
    $releaseExport.environment_variable -ne $contractExport.environment_variable -or
    $releaseExport.default_enabled -ne $false -or $contractExport.default_enabled -ne $false -or
    $releaseExport.requires_tcp_client -ne $true -or $contractExport.requires_tcp_client -ne $true -or
    $releaseExport.commit_point -ne 'after_complete_tcp_frame_write' -or
    $releaseExport.commit_point -ne $contractExport.commit_point -or
    $releaseExport.online_target_truth_enabled -ne $false -or
    $contractExport.online_target_truth_enabled -ne $false -or
    $releaseExport.online_target_count -ne 0 -or $contractExport.online_target_count -ne 0 -or
    $releaseExport.future_truth_included -ne $false -or
    $contractExport.future_truth_included -ne $false) {
    throw 'Offline exact-corner export contract is missing, inconsistent, or leaks online/future truth.'
}
$schemaPath = Join-Path $root ('sdk\' + $releaseExport.schema_file.Replace('/', '\'))
if (-not (Test-Path -LiteralPath $schemaPath -PathType Leaf)) {
    throw "Offline exact-corner schema is missing: $schemaPath"
}
$cornerSchema = Get-Content -LiteralPath $schemaPath -Raw -Encoding UTF8 | ConvertFrom-Json
if ($cornerSchema.properties.schema_version.const -ne $releaseExport.schema_version -or
    $cornerSchema.properties.future_truth_included.const -ne $false -or
    $cornerSchema.properties.target_number.const -ne 3 -or
    $cornerSchema.properties.motion_uniform_guard_ns.const -ne 100000000 -or
    $cornerSchema.properties.plate_geometry.properties.corner_order.const -ne 'bl,tl,tr,br') {
    throw 'Offline exact-corner schema does not enforce identity, target, ordering, motion, and future-truth constants.'
}
$vehicleHash = (Get-FileHash -LiteralPath (Join-Path $root 'assets\vehicle.glb') -Algorithm SHA256).Hash.ToLowerInvariant()
if ($cornerSchema.properties.plate_geometry.properties.asset_sha256.const -ne $vehicleHash) {
    throw 'Offline exact-corner schema asset hash does not match assets/vehicle.glb.'
}
if ($calibration.schema_version -ne 1 -or
    $calibration.read_only -ne $true -or
    $calibration.runtime_setter_available -ne $false -or
    $calibration.revision -ne $release.fixed_calibration_revision -or
    $calibration.image.width -ne $contract.image_width -or
    $calibration.image.height -ne $contract.image_height -or
    $calibration.intrinsics.fx -ne $calibration.intrinsics.fy -or
    [string]::IsNullOrWhiteSpace($calibration.calibration_id)) {
    throw 'Fixed camera calibration is missing, mutable, or inconsistent with the SDK image contract.'
}
if ($calibration.exposure.model -ne 'fixed_ev100' -or
    $calibration.exposure.ev100 -ne 9.7 -or
    [math]::Abs($calibration.exposure.exposure_scalar - 0.00100190788846429) -gt 1e-15 -or
    $calibration.exposure.auto_exposure -ne $false -or
    $calibration.exposure.tonemapping -ne 'none' -or
    $calibration.exposure.physical_sensor_parameters_applicable -ne $false) {
    throw 'Fixed digital camera exposure is missing or inconsistent with the release contract.'
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

"compatible simulator=$($release.version) sdk=$($release.sdk_version) shm=$($release.shm_version) abi=$($release.sdk_abi_revision) image=$($release.image.width)x$($release.image.height) tcp=$($release.tcp_image_port) udp=$($release.udp_command_port) scene_control=$($release.scene_control_protocol)/$($release.scene_control_port) offline_exact_corners=default_off"
