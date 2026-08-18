#include <daedalus_sim_sdk/talos_metadata_reader.hpp>

#include <atomic>
#include <cmath>
#include <cstring>
#include <limits>
#include <memory>
#include <string>
#include <type_traits>

#ifdef _WIN32
#define NOMINMAX
#include <windows.h>
#else
#include <cerrno>
#include <fcntl.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>
#endif

namespace daedalus::sim::sdk::v1 {
namespace {

constexpr int kSnapshotAttempts = 8;

template <typename T>
bool copyTwiceStable(const T* source, T* output) noexcept {
  static_assert(std::is_trivially_copyable<T>::value,
                "shared-memory snapshots must be trivially copyable");
  T first{};
  T second{};
  for (int attempt = 0; attempt < kSnapshotAttempts; ++attempt) {
    std::atomic_thread_fence(std::memory_order_acquire);
    std::memcpy(&first, source, sizeof(T));
    std::atomic_thread_fence(std::memory_order_seq_cst);
    std::memcpy(&second, source, sizeof(T));
    std::atomic_thread_fence(std::memory_order_acquire);
    if (std::memcmp(&first, &second, sizeof(T)) == 0) {
      *output = second;
      return true;
    }
  }
  return false;
}

std::uint8_t loadSharedByte(const std::uint8_t* source) noexcept {
  const auto value = *reinterpret_cast<const volatile std::uint8_t*>(source);
  std::atomic_thread_fence(std::memory_order_acquire);
  return value;
}

std::uint64_t loadSharedU64(const std::uint64_t* source) noexcept {
  const auto value = *reinterpret_cast<const volatile std::uint64_t*>(source);
  std::atomic_thread_fence(std::memory_order_acquire);
  return value;
}

template <typename Buffer, typename Slot>
bool readLatestTripleBuffer(const Buffer& buffer, Slot* output) noexcept {
  for (int attempt = 0; attempt < kSnapshotAttempts; ++attempt) {
    const std::uint8_t state_before = loadSharedByte(&buffer.state);
    const std::uint8_t read_before = loadSharedByte(&buffer.read_idx);
    const std::uint8_t index = (state_before & kFlagNew) != 0
                                   ? state_before & kIndexMask
                                   : read_before;
    if (index >= 3) return false;
    Slot snapshot{};
    std::memcpy(&snapshot, &buffer.slots[index], sizeof(Slot));
    std::atomic_thread_fence(std::memory_order_acquire);
    const std::uint8_t state_after = loadSharedByte(&buffer.state);
    const std::uint8_t read_after = loadSharedByte(&buffer.read_idx);
    if (state_before == state_after && read_before == read_after) {
      *output = snapshot;
      return true;
    }
  }
  return false;
}

const char* compatibilityMessage(TalosCompatibility status) noexcept {
  switch (status) {
    case TalosCompatibility::Compatible: return "compatible";
    case TalosCompatibility::NullMapping: return "metadata mapping is null";
    case TalosCompatibility::MappingTooSmall:
      return "metadata mapping is smaller than ShmMetaRegion";
    case TalosCompatibility::InvalidMagic: return "Talos magic mismatch";
    case TalosCompatibility::UnsupportedVersion:
      return "Talos SHM version mismatch";
    case TalosCompatibility::InvalidDimensions:
      return "Talos image dimensions mismatch";
    case TalosCompatibility::InvalidMetaSize:
      return "Talos metadata size mismatch";
    case TalosCompatibility::UnsupportedAbiRevision:
      return "Talos SDK ABI revision mismatch";
  }
  return "unknown Talos compatibility error";
}

template <typename T>
ClientResult<T> unstable(const char* name) {
  return ClientResult<T>::failure(ClientError::UnstableSnapshot,
                                  std::string(name) +
                                      " changed while being copied");
}

}  // namespace

TalosMetadataReader::TalosMetadataReader(const void* mapped_memory,
                                         std::size_t mapped_bytes) noexcept
    : region_(static_cast<const ShmMetaRegion*>(mapped_memory)),
      mapped_bytes_(mapped_bytes) {}

TalosCompatibility TalosMetadataReader::compatibility() const noexcept {
  if (region_ == nullptr) return TalosCompatibility::NullMapping;
  if (mapped_bytes_ < sizeof(ShmMetaRegion)) {
    return TalosCompatibility::MappingTooSmall;
  }
  ShmHeader header{};
  std::memcpy(&header, &region_->header, sizeof(header));
  if (header.magic != kShmMagic) return TalosCompatibility::InvalidMagic;
  if (header.version != kShmVersion) {
    return TalosCompatibility::UnsupportedVersion;
  }
  if (header.image_width != kImageWidth ||
      header.image_height != kImageHeight) {
    return TalosCompatibility::InvalidDimensions;
  }
  if (header.meta_size != kMetaSize) {
    return TalosCompatibility::InvalidMetaSize;
  }
  if (header.sdk_abi_revision != kSdkAbiRevision) {
    return TalosCompatibility::UnsupportedAbiRevision;
  }
  return TalosCompatibility::Compatible;
}

ClientResult<ShmHeader> TalosMetadataReader::readHeader() const {
  const auto compatible = compatibility();
  if (compatible != TalosCompatibility::Compatible) {
    return ClientResult<ShmHeader>::failure(
        ClientError::IncompatibleMetadata, compatibilityMessage(compatible));
  }
  ShmHeader header{};
  if (!copyTwiceStable(&region_->header, &header)) {
    return unstable<ShmHeader>("Talos header");
  }
  return ClientResult<ShmHeader>::success(header);
}

ClientResult<ImageMeta> TalosMetadataReader::readLatestImageMeta() const {
  const auto compatible = compatibility();
  if (compatible != TalosCompatibility::Compatible) {
    return ClientResult<ImageMeta>::failure(
        ClientError::IncompatibleMetadata, compatibilityMessage(compatible));
  }
  ImageMeta value{};
  if (!readLatestTripleBuffer(region_->image, &value)) {
    return unstable<ImageMeta>("Talos image metadata");
  }
  if (value.seq == 0 || value.timestamp_ns == 0 || value.buffer_id >= 3 ||
      value.width == 0 || value.height == 0 || value.width > kImageWidth ||
      value.height > kImageHeight) {
    return ClientResult<ImageMeta>::failure(
        ClientError::ProtocolError, "Talos image metadata is not initialized");
  }
  return ClientResult<ImageMeta>::success(value);
}

ClientResult<PoseMeta> TalosMetadataReader::readLatestPose(
    std::size_t pose_index) const {
  const auto compatible = compatibility();
  if (compatible != TalosCompatibility::Compatible) {
    return ClientResult<PoseMeta>::failure(
        ClientError::IncompatibleMetadata, compatibilityMessage(compatible));
  }
  if (pose_index >= 5) {
    return ClientResult<PoseMeta>::failure(ClientError::InvalidArgument,
                                           "pose index must be in [0, 4]");
  }
  PoseMeta value{};
  if (!readLatestTripleBuffer(region_->poses[pose_index], &value)) {
    return unstable<PoseMeta>("Talos pose metadata");
  }
  if (value.frame_seq == 0 || value.timestamp_ns == 0) {
    return ClientResult<PoseMeta>::failure(
        ClientError::ProtocolError, "Talos pose metadata is not initialized");
  }
  return ClientResult<PoseMeta>::success(value);
}

ClientResult<CameraInfo> TalosMetadataReader::readCameraInfo() const {
  const auto compatible = compatibility();
  if (compatible != TalosCompatibility::Compatible) {
    return ClientResult<CameraInfo>::failure(
        ClientError::IncompatibleMetadata, compatibilityMessage(compatible));
  }
  CameraInfo value{};
  if (!copyTwiceStable(&region_->camera_info, &value)) {
    return unstable<CameraInfo>("Talos camera info");
  }
  if (value.timestamp_ns == 0 || value.width != kImageWidth ||
      value.height != kImageHeight) {
    return ClientResult<CameraInfo>::failure(
        ClientError::ProtocolError, "Talos camera info is not initialized");
  }
  return ClientResult<CameraInfo>::success(value);
}

ClientResult<ChassisObservation>
TalosMetadataReader::readChassisObservation() const {
  const auto compatible = compatibility();
  if (compatible != TalosCompatibility::Compatible) {
    return ClientResult<ChassisObservation>::failure(
        ClientError::IncompatibleMetadata, compatibilityMessage(compatible));
  }
  ChassisObservation value{};
  if (!copyTwiceStable(&region_->chassis_observation, &value)) {
    return unstable<ChassisObservation>("Talos chassis observation");
  }
  if (value.frame_seq == 0 || value.timestamp_ns == 0) {
    return ClientResult<ChassisObservation>::failure(
        ClientError::ProtocolError,
        "Talos chassis observation is not initialized");
  }
  return ClientResult<ChassisObservation>::success(value);
}

ClientResult<GroundTruthBatch>
TalosMetadataReader::readLatestGroundTruth() const {
  const auto compatible = compatibility();
  if (compatible != TalosCompatibility::Compatible) {
    return ClientResult<GroundTruthBatch>::failure(
        ClientError::IncompatibleMetadata, compatibilityMessage(compatible));
  }
  GroundTruthBatch value{};
  if (!copyTwiceStable(&region_->ground_truth, &value)) {
    return unstable<GroundTruthBatch>("Talos ground truth");
  }
  if (value.frame_seq == 0 || value.timestamp_ns == 0 ||
      value.target_count > kGroundTruthMaxTargets ||
      value.rune_count > kGroundTruthMaxRunes) {
    return ClientResult<GroundTruthBatch>::failure(
        ClientError::ProtocolError, "Talos ground truth is invalid");
  }
  return ClientResult<GroundTruthBatch>::success(value);
}

ClientResult<RuntimeState> TalosMetadataReader::readRuntimeState() const {
  const auto compatible = compatibility();
  if (compatible != TalosCompatibility::Compatible) {
    return ClientResult<RuntimeState>::failure(
        ClientError::IncompatibleMetadata, compatibilityMessage(compatible));
  }
  RuntimeState value{};
  if (!copyTwiceStable(&region_->runtime_state, &value)) {
    return unstable<RuntimeState>("Talos runtime state");
  }
  if (value.timestamp_ns == 0) {
    return ClientResult<RuntimeState>::failure(
        ClientError::ProtocolError, "Talos runtime state is not initialized");
  }
  return ClientResult<RuntimeState>::success(value);
}

ClientResult<GimbalState> TalosMetadataReader::readGimbalState() const {
  const auto runtime = readRuntimeState();
  if (!runtime) {
    return ClientResult<GimbalState>::failure(runtime.status.error,
                                               runtime.status.message);
  }
  constexpr float kRadiansToDegrees =
      180.0F / 3.14159265358979323846F;
  GimbalState state{};
  state.frame_seq = runtime.value->frame_seq;
  state.timestamp_ns = runtime.value->timestamp_ns;
  state.last_applied_command_id = runtime.value->last_applied_command_id;
  state.yaw_deg = runtime.value->gimbal_yaw_rad * kRadiansToDegrees;
  state.pitch_deg = 90.0F + runtime.value->gimbal_pitch_rad * kRadiansToDegrees;
  state.yaw_velocity_deg_s =
      runtime.value->gimbal_yaw_velocity_rad_s * kRadiansToDegrees;
  state.pitch_velocity_deg_s =
      runtime.value->gimbal_pitch_velocity_rad_s * kRadiansToDegrees;
  state.status_flags = runtime.value->status_flags;
  return ClientResult<GimbalState>::success(state);
}

ClientResult<ExposureState> TalosMetadataReader::readExposureStateForFrame(
    std::uint64_t frame_seq) const {
  const auto snapshot = readGroundTruthForFrame(frame_seq);
  if (!snapshot) {
    return ClientResult<ExposureState>::failure(snapshot.status.error,
                                                 snapshot.status.message);
  }
  return ClientResult<ExposureState>::success(snapshot.value->exposure_state);
}

ClientResult<GimbalState> TalosMetadataReader::readGimbalStateForFrame(
    std::uint64_t frame_seq) const {
  const auto exposure = readExposureStateForFrame(frame_seq);
  if (!exposure) {
    return ClientResult<GimbalState>::failure(exposure.status.error,
                                               exposure.status.message);
  }
  constexpr float kRadiansToDegrees =
      180.0F / 3.14159265358979323846F;
  GimbalState state{};
  state.frame_seq = exposure.value->frame_seq;
  state.timestamp_ns = exposure.value->timestamp_ns;
  state.yaw_deg = exposure.value->gimbal_yaw_rad * kRadiansToDegrees;
  state.pitch_deg =
      90.0F + exposure.value->gimbal_pitch_rad * kRadiansToDegrees;
  return ClientResult<GimbalState>::success(state);
}

ClientResult<GroundTruthExposureSnapshot>
TalosMetadataReader::readGroundTruthForFrame(std::uint64_t frame_seq) const {
  const auto compatible = compatibility();
  if (compatible != TalosCompatibility::Compatible) {
    return ClientResult<GroundTruthExposureSnapshot>::failure(
        ClientError::IncompatibleMetadata, compatibilityMessage(compatible));
  }
  if (frame_seq == 0) {
    return ClientResult<GroundTruthExposureSnapshot>::failure(
        ClientError::InvalidArgument, "frame_seq must be non-zero");
  }
  const auto header = readHeader();
  if (!header || header.value->created_ns == 0) {
    return ClientResult<GroundTruthExposureSnapshot>::failure(
        header ? ClientError::ProtocolError : header.status.error,
        header ? "Talos producer epoch is not initialized" : header.status.message);
  }

  GroundTruthExposureSnapshot best{};
  bool found = false;
  bool unstable_slot = false;
  for (const auto& slot : region_->ground_truth_history.slots) {
    for (int attempt = 0; attempt < kSnapshotAttempts; ++attempt) {
      const std::uint64_t commit_before = loadSharedU64(&slot.commit_seq);
      if (commit_before == 0 || (commit_before & 1U) != 0) break;
      GroundTruthExposureSnapshot candidate{};
      std::memcpy(&candidate.ground_truth, &slot.ground_truth,
                  sizeof(candidate.ground_truth));
      std::memcpy(&candidate.exposure_state, &slot.exposure_state,
                  sizeof(candidate.exposure_state));
      std::atomic_thread_fence(std::memory_order_acquire);
      const std::uint64_t commit_after = loadSharedU64(&slot.commit_seq);
      if (commit_before != commit_after || (commit_after & 1U) != 0) {
        unstable_slot = true;
        continue;
      }
      candidate.publication = commit_after / 2U;
      candidate.producer_epoch = header.value->created_ns;
      if (candidate.ground_truth.frame_seq == frame_seq &&
          candidate.exposure_state.frame_seq == frame_seq &&
          candidate.ground_truth.timestamp_ns != 0 &&
          candidate.ground_truth.timestamp_ns == candidate.exposure_state.timestamp_ns &&
          (!found || candidate.publication > best.publication)) {
        best = candidate;
        found = true;
      }
      break;
    }
  }
  if (found) {
    return ClientResult<GroundTruthExposureSnapshot>::success(best);
  }
  return ClientResult<GroundTruthExposureSnapshot>::failure(
      unstable_slot ? ClientError::UnstableSnapshot : ClientError::ProtocolError,
      unstable_slot ? "ground-truth history changed while being copied"
                    : "requested frame is absent from ground-truth history");
}

class TalosMetadataMapping::Impl {
 public:
  ~Impl() { close(); }

  ClientStatus open(const std::string& path) {
    close();
#ifdef _WIN32
    file_ = CreateFileA(path.c_str(), GENERIC_READ,
                        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                        nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file_ == INVALID_HANDLE_VALUE) {
      return ClientStatus::failure(ClientError::ReceiveFailed,
                                   "could not open Talos metadata file");
    }
    LARGE_INTEGER size{};
    if (!GetFileSizeEx(file_, &size) || size.QuadPart < 0) {
      close();
      return ClientStatus::failure(ClientError::ReceiveFailed,
                                   "could not read Talos metadata size");
    }
    bytes_ = static_cast<std::size_t>(size.QuadPart);
    mapping_ = CreateFileMappingA(file_, nullptr, PAGE_READONLY, 0, 0, nullptr);
    if (mapping_ == nullptr) {
      close();
      return ClientStatus::failure(ClientError::ReceiveFailed,
                                   "could not create Talos metadata mapping");
    }
    data_ = MapViewOfFile(mapping_, FILE_MAP_READ, 0, 0, 0);
    if (data_ == nullptr) {
      close();
      return ClientStatus::failure(ClientError::ReceiveFailed,
                                   "could not map Talos metadata file");
    }
#else
    fd_ = ::open(path.c_str(), O_RDONLY);
    if (fd_ < 0) {
      return ClientStatus::failure(ClientError::ReceiveFailed,
                                   "could not open Talos metadata file");
    }
    struct stat info {};
    if (::fstat(fd_, &info) != 0 || info.st_size < 0) {
      close();
      return ClientStatus::failure(ClientError::ReceiveFailed,
                                   "could not read Talos metadata size");
    }
    bytes_ = static_cast<std::size_t>(info.st_size);
    data_ = ::mmap(nullptr, bytes_, PROT_READ, MAP_SHARED, fd_, 0);
    if (data_ == MAP_FAILED) {
      data_ = nullptr;
      close();
      return ClientStatus::failure(ClientError::ReceiveFailed,
                                   "could not map Talos metadata file");
    }
#endif
    TalosMetadataReader candidate(data_, bytes_);
    if (candidate.compatibility() != TalosCompatibility::Compatible) {
      close();
      return ClientStatus::failure(ClientError::IncompatibleMetadata,
                                   "Talos metadata mapping is incompatible");
    }
    return ClientStatus::success();
  }

  void close() noexcept {
#ifdef _WIN32
    if (data_ != nullptr) UnmapViewOfFile(data_);
    if (mapping_ != nullptr) CloseHandle(mapping_);
    if (file_ != INVALID_HANDLE_VALUE) CloseHandle(file_);
    file_ = INVALID_HANDLE_VALUE;
    mapping_ = nullptr;
#else
    if (data_ != nullptr) ::munmap(data_, bytes_);
    if (fd_ >= 0) ::close(fd_);
    fd_ = -1;
#endif
    data_ = nullptr;
    bytes_ = 0;
  }

  const void* data() const noexcept { return data_; }
  std::size_t bytes() const noexcept { return bytes_; }

 private:
  void* data_ = nullptr;
  std::size_t bytes_ = 0;
#ifdef _WIN32
  HANDLE file_ = INVALID_HANDLE_VALUE;
  HANDLE mapping_ = nullptr;
#else
  int fd_ = -1;
#endif
};

TalosMetadataMapping::TalosMetadataMapping() : impl_(std::make_unique<Impl>()) {}
TalosMetadataMapping::~TalosMetadataMapping() = default;
TalosMetadataMapping::TalosMetadataMapping(TalosMetadataMapping&&) noexcept = default;
TalosMetadataMapping& TalosMetadataMapping::operator=(TalosMetadataMapping&&) noexcept = default;
ClientStatus TalosMetadataMapping::open(const std::string& metadata_path) {
  if (metadata_path.empty()) {
    return ClientStatus::failure(ClientError::InvalidArgument,
                                 "metadata path must not be empty");
  }
  return impl_->open(metadata_path);
}
void TalosMetadataMapping::close() noexcept { impl_->close(); }
bool TalosMetadataMapping::isOpen() const noexcept {
  return impl_->data() != nullptr;
}
ClientResult<TalosMetadataReader> TalosMetadataMapping::reader() const {
  if (!isOpen()) {
    return ClientResult<TalosMetadataReader>::failure(
        ClientError::NotReady, "Talos metadata mapping is not open");
  }
  return ClientResult<TalosMetadataReader>::success(
      TalosMetadataReader(impl_->data(), impl_->bytes()));
}

}  // namespace daedalus::sim::sdk::v1
