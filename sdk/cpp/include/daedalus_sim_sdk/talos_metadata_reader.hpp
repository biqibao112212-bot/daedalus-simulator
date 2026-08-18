#pragma once

#include <daedalus_sim_sdk/client_result.hpp>
#include <daedalus_sim_sdk/talos_v1.hpp>

#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>

namespace daedalus::sim::sdk::v1 {

enum class TalosCompatibility {
  Compatible,
  NullMapping,
  MappingTooSmall,
  InvalidMagic,
  UnsupportedVersion,
  InvalidDimensions,
  InvalidMetaSize,
  UnsupportedAbiRevision,
};

struct GroundTruthExposureSnapshot {
  // Exact producer identity from ShmHeader::created_ns. Compare this and the
  // frame/timestamp below with TcpImageFrame::header before consuming truth.
  std::uint64_t producer_epoch = 0;
  GroundTruthBatch ground_truth{};
  ExposureState exposure_state{};
  std::uint64_t publication = 0;
};

// Public command-space gimbal feedback. yaw_deg uses the same zero as
// UdpGimbalCommand::yaw_deg. pitch_deg is 90 degrees at level aim, matching
// UdpGimbalCommand::pitch_deg.
struct GimbalState {
  std::uint64_t frame_seq = 0;
  std::uint64_t timestamp_ns = 0;
  std::uint64_t last_applied_command_id = 0;
  float yaw_deg = 0.0F;
  float pitch_deg = 90.0F;
  float yaw_velocity_deg_s = 0.0F;
  float pitch_velocity_deg_s = 0.0F;
  std::uint32_t status_flags = 0;
};

class TalosMetadataReader {
 public:
  TalosMetadataReader(const void* mapped_memory,
                      std::size_t mapped_bytes) noexcept;

  [[nodiscard]] TalosCompatibility compatibility() const noexcept;
  [[nodiscard]] ClientResult<ShmHeader> readHeader() const;
  [[nodiscard]] ClientResult<ImageMeta> readLatestImageMeta() const;
  [[nodiscard]] ClientResult<PoseMeta> readLatestPose(
      std::size_t pose_index) const;
  [[nodiscard]] ClientResult<CameraInfo> readCameraInfo() const;
  [[nodiscard]] ClientResult<ChassisObservation> readChassisObservation() const;
  [[nodiscard]] ClientResult<GroundTruthBatch> readLatestGroundTruth() const;
  [[nodiscard]] ClientResult<RuntimeState> readRuntimeState() const;
  [[nodiscard]] ClientResult<GimbalState> readGimbalState() const;
  // Reads the actual gimbal state captured with a specific image frame. The
  // history retains the latest 16 exposure frames.
  [[nodiscard]] ClientResult<GimbalState> readGimbalStateForFrame(
      std::uint64_t frame_seq) const;
  [[nodiscard]] ClientResult<ExposureState> readExposureStateForFrame(
      std::uint64_t frame_seq) const;
  [[nodiscard]] ClientResult<GroundTruthExposureSnapshot>
  readGroundTruthForFrame(std::uint64_t frame_seq) const;

 private:
  const ShmMetaRegion* region_ = nullptr;
  std::size_t mapped_bytes_ = 0;
};

// Owns a read-only mapping of talos_ipc_meta. This is the normal entry point
// for consumers; TalosMetadataReader remains available for embedded/shared
// mappings supplied by another runtime.
class TalosMetadataMapping {
 public:
  TalosMetadataMapping();
  ~TalosMetadataMapping();
  TalosMetadataMapping(const TalosMetadataMapping&) = delete;
  TalosMetadataMapping& operator=(const TalosMetadataMapping&) = delete;
  TalosMetadataMapping(TalosMetadataMapping&&) noexcept;
  TalosMetadataMapping& operator=(TalosMetadataMapping&&) noexcept;

  [[nodiscard]] ClientStatus open(const std::string& metadata_path);
  void close() noexcept;
  [[nodiscard]] bool isOpen() const noexcept;
  [[nodiscard]] ClientResult<TalosMetadataReader> reader() const;

 private:
  class Impl;
  std::unique_ptr<Impl> impl_;
};

}  // namespace daedalus::sim::sdk::v1
