#pragma once

#include <cstddef>
#include <cstdint>

namespace daedalus::sim::sdk::v1 {

inline constexpr const char *kSdkVersion = "1.3.0";
inline constexpr std::uint32_t kShmMagic = 0x54414C05;
inline constexpr std::uint32_t kShmVersion = 7;
inline constexpr std::uint32_t kSdkAbiRevision = 2;
inline constexpr std::uint32_t kImageWidth = 1440;
inline constexpr std::uint32_t kImageHeight = 1080;
inline constexpr std::uint32_t kImageChannels = 3;
inline constexpr std::size_t kMaxImagePayloadBytes =
    kImageWidth * kImageHeight * kImageChannels;
inline constexpr std::size_t kImageSlotStrideBytes = kMaxImagePayloadBytes;
inline constexpr std::size_t kImagePoolSize = kImageSlotStrideBytes * 3;
inline constexpr std::size_t kMetaSize = 76992;
inline constexpr std::size_t kGimbalPoseIndex = 0;
inline constexpr std::size_t kOdomPoseIndex = 1;
inline constexpr std::size_t kMuzzlePoseIndex = 2;
inline constexpr std::size_t kCameraPoseIndex = 3;
inline constexpr std::uint8_t kFlagNew = 0x80;
inline constexpr std::uint8_t kIndexMask = 0x03;

struct alignas(32) ImageMeta {
  std::uint64_t seq;
  std::uint64_t timestamp_ns;
  std::uint32_t width;
  std::uint32_t height;
  std::uint8_t buffer_id;
  std::uint8_t format;
  std::uint8_t pad[6];
};
static_assert(sizeof(ImageMeta) == 32);

struct alignas(64) PoseMeta {
  std::uint64_t frame_seq;
  float position[3];
  float quaternion[4];
  std::uint64_t timestamp_ns;
  std::uint8_t pad[16];
};
static_assert(sizeof(PoseMeta) == 64);

struct alignas(32) GimbalCmd {
  std::uint64_t timestamp_ns;
  float yaw_deg;
  float pitch_deg;
  float distance_m;
  std::uint8_t fire_advice;
  std::uint8_t pad[11];
};
static_assert(sizeof(GimbalCmd) == 32);

struct alignas(64) CameraInfo {
  std::uint64_t timestamp_ns;
  double fx;
  double fy;
  double cx;
  double cy;
  double distortion[5];
  std::uint32_t width;
  std::uint32_t height;
  std::uint8_t pad[24];
};
static_assert(sizeof(CameraInfo) == 128);

struct alignas(64) ChassisObservation {
  std::uint64_t frame_seq;
  std::uint64_t timestamp_ns;
  float dt_s;
  float v_body[2];
  float wz_radps;
  float wheel_linear_mps[4];
  float wheel_angular_radps[4];
  float a_body[2];
  float alpha_z_radps2;
  float rpy_rad[3];
  float gyro_xyz_radps[3];
  float accel_xyz_mps2[3];
  std::uint8_t pad[16];
};
static_assert(sizeof(ChassisObservation) == 128);

struct alignas(64) RuntimeState {
  std::uint64_t timestamp_ns;
  std::uint64_t frame_seq;
  std::uint64_t last_applied_command_id;
  std::uint8_t following;
  std::uint8_t pad1[3];
  float gimbal_yaw_rad;
  float gimbal_pitch_rad;
  float gimbal_yaw_velocity_rad_s;
  float gimbal_pitch_velocity_rad_s;
  std::uint32_t status_flags;
  std::uint8_t pad[16];
};
static_assert(sizeof(RuntimeState) == 64);

inline constexpr std::size_t kGroundTruthMaxTargets = 16;
inline constexpr std::size_t kGroundTruthMaxRunes = 4;
inline constexpr std::size_t kGroundTruthMaxArmorsPerTarget = 4;
inline constexpr std::uint8_t kGroundTruthVisibilityUnknown = 0;
inline constexpr std::uint8_t kGroundTruthVisibilityHidden = 1;
inline constexpr std::uint8_t kGroundTruthFrameUnknown = 0;
inline constexpr std::uint8_t kGroundTruthFrameRosOdom = 1;
inline constexpr std::uint8_t kGroundTruthFrameChassisLocalRos = 2;
inline constexpr std::uint32_t kGroundTruthTargetHasWorldState = 1U << 0;
inline constexpr std::uint32_t kGroundTruthTargetHasWorldOrientation = 1U << 1;
inline constexpr std::uint32_t kGroundTruthTargetHasArmorGeometry = 1U << 2;
inline constexpr std::uint32_t kExposureStateHasChassisWorldPose = 1U << 0;
inline constexpr std::uint32_t kExposureStateHasGimbalWorldPose = 1U << 1;
inline constexpr std::uint32_t kExposureStateHasCameraWorldPose = 1U << 2;
inline constexpr std::size_t kGroundTruthHistorySlots = 16;

struct alignas(32) GroundTruthArmor {
  std::uint8_t relative_slot;
  std::uint8_t visibility;
  std::uint8_t pad1[2];
  float relative_position[3];
  float outward_normal[3];
  float relative_yaw;
};
static_assert(sizeof(GroundTruthArmor) == 32);
static_assert(offsetof(GroundTruthArmor, relative_position) == 4);
static_assert(offsetof(GroundTruthArmor, outward_normal) == 16);
static_assert(offsetof(GroundTruthArmor, relative_yaw) == 28);

struct alignas(32) GroundTruthTarget {
  std::uint64_t frame_seq;
  std::uint64_t timestamp_ns;
  std::uint8_t team;
  std::uint8_t armor_label;
  std::uint8_t is_outpost;
  std::uint8_t armor_count;
  float position[3];
  float vyaw;
  float yaw;
  float velocity[3];
  float radius_even;
  float radius_odd;
  float armor_height;
  GroundTruthArmor armors[kGroundTruthMaxArmorsPerTarget];
  std::uint64_t target_id;
  float world_quaternion_wxyz[4];
  std::uint8_t world_state_frame;
  std::uint8_t armor_geometry_frame;
  std::uint8_t pad2[2];
  std::uint32_t state_flags;
};
static_assert(sizeof(GroundTruthTarget) == 224);
static_assert(offsetof(GroundTruthTarget, position) == 20);
static_assert(offsetof(GroundTruthTarget, vyaw) == 32);
static_assert(offsetof(GroundTruthTarget, yaw) == 36);
static_assert(offsetof(GroundTruthTarget, velocity) == 40);
static_assert(offsetof(GroundTruthTarget, radius_even) == 52);
static_assert(offsetof(GroundTruthTarget, armors) == 64);
static_assert(offsetof(GroundTruthTarget, target_id) == 192);
static_assert(offsetof(GroundTruthTarget, world_quaternion_wxyz) == 200);
static_assert(offsetof(GroundTruthTarget, world_state_frame) == 216);
static_assert(offsetof(GroundTruthTarget, state_flags) == 220);

struct alignas(64) GroundTruthRune {
  std::uint64_t frame_seq;
  std::uint64_t timestamp_ns;
  std::uint8_t team;
  std::uint8_t rune_mode;
  std::uint8_t mechanism_state;
  std::uint8_t pad1;
  float r_center_odom[3];
  float radius;
  float current_angle;
  float v_roll;
  std::int32_t direction;
  float sin_amplitude;
  float sin_omega;
  float sin_phase;
  float sin_offset;
  float relative_time;
  std::int32_t blade_id;
  std::uint8_t target_activations[5];
  std::uint8_t pad[20];
};
static_assert(sizeof(GroundTruthRune) == 128);

struct alignas(64) GroundTruthBatch {
  std::uint64_t frame_seq;
  std::uint64_t timestamp_ns;
  std::uint32_t target_count;
  std::uint32_t rune_count;
  GroundTruthTarget targets[kGroundTruthMaxTargets];
  GroundTruthRune runes[kGroundTruthMaxRunes];
  std::uint8_t pad[64];
};
static_assert(sizeof(GroundTruthBatch) == 4224);
static_assert(offsetof(GroundTruthBatch, targets) == 32);
static_assert(offsetof(GroundTruthBatch, runes) == 3648);

struct alignas(64) ExposureState {
  std::uint64_t frame_seq;
  std::uint64_t timestamp_ns;
  std::uint32_t state_flags;
  std::uint8_t world_frame;
  std::uint8_t pad1[3];
  float chassis_position_world[3];
  float chassis_quaternion_world_wxyz[4];
  float chassis_rpy_world[3];
  float gimbal_position_world[3];
  float gimbal_quaternion_world_wxyz[4];
  float camera_position_world[3];
  float camera_quaternion_world_wxyz[4];
  float gimbal_yaw_rad;
  float gimbal_pitch_rad;
};
static_assert(sizeof(ExposureState) == 128);
static_assert(offsetof(ExposureState, chassis_position_world) == 24);
static_assert(offsetof(ExposureState, gimbal_position_world) == 64);
static_assert(offsetof(ExposureState, camera_position_world) == 92);

struct alignas(64) GroundTruthHistorySlot {
  std::uint64_t commit_seq;
  std::uint8_t pad1[56];
  GroundTruthBatch ground_truth;
  ExposureState exposure_state;
};
static_assert(sizeof(GroundTruthHistorySlot) == 4416);
static_assert(offsetof(GroundTruthHistorySlot, ground_truth) == 64);
static_assert(offsetof(GroundTruthHistorySlot, exposure_state) == 4288);

struct alignas(64) GroundTruthHistory {
  std::uint64_t next_publication;
  std::uint8_t pad1[56];
  GroundTruthHistorySlot slots[kGroundTruthHistorySlots];
};
static_assert(sizeof(GroundTruthHistory) == 70720);
static_assert(offsetof(GroundTruthHistory, slots) == 64);

template <typename Slot, std::size_t Size> struct alignas(64) TripleBuffer {
  std::uint8_t state;
  std::uint8_t write_idx;
  std::uint8_t read_idx;
  std::uint8_t pad[61];
  Slot slots[3];
};

using ImageTripleBuffer = TripleBuffer<ImageMeta, 192>;
using PoseTripleBuffer = TripleBuffer<PoseMeta, 256>;
using GimbalTripleBuffer = TripleBuffer<GimbalCmd, 192>;
static_assert(sizeof(ImageTripleBuffer) == 192);
static_assert(sizeof(PoseTripleBuffer) == 256);
static_assert(sizeof(GimbalTripleBuffer) == 192);

struct alignas(64) ShmHeader {
  std::uint32_t magic;
  std::uint32_t version;
  std::uint64_t created_ns;
  std::uint64_t heartbeat_ns;
  std::uint32_t image_width;
  std::uint32_t image_height;
  std::uint32_t meta_size;
  std::uint32_t sdk_abi_revision;
  std::uint8_t pad[24];
};
static_assert(sizeof(ShmHeader) == 64);

struct ShmMetaRegion {
  ShmHeader header;
  ImageTripleBuffer image;
  PoseTripleBuffer poses[5];
  GimbalTripleBuffer gimbal_cmd;
  CameraInfo camera_info;
  ChassisObservation chassis_observation;
  GroundTruthBatch ground_truth;
  RuntimeState runtime_state;
  GroundTruthHistory ground_truth_history;
};
static_assert(sizeof(ShmMetaRegion) == kMetaSize);
static_assert(offsetof(ShmMetaRegion, gimbal_cmd) == 1536);
static_assert(offsetof(ShmMetaRegion, camera_info) == 1728);
static_assert(offsetof(ShmMetaRegion, chassis_observation) == 1856);
static_assert(offsetof(ShmMetaRegion, ground_truth) == 1984);
static_assert(offsetof(ShmMetaRegion, runtime_state) == 6208);
static_assert(offsetof(ShmMetaRegion, ground_truth_history) == 6272);

[[nodiscard]] inline bool isCompatible(const ShmHeader &header) noexcept {
  return header.magic == kShmMagic && header.version == kShmVersion &&
         header.image_width == kImageWidth &&
         header.image_height == kImageHeight && header.meta_size == kMetaSize &&
         header.sdk_abi_revision == kSdkAbiRevision;
}

} // namespace daedalus::sim::sdk::v1
