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
  GroundTruthBatch ground_truth{};
  ExposureState exposure_state{};
  std::uint64_t publication = 0;
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
