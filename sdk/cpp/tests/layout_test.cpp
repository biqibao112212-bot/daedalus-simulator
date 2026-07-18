#include <daedalus_sim_sdk/talos_v1.hpp>

#include <cstddef>

using namespace daedalus::sim::sdk::v1;

int main() {
  static_assert(kShmVersion == 7);
  static_assert(kImageWidth == 1440);
  static_assert(kImageHeight == 1080);
  static_assert(kImagePoolSize == 1440ULL * 1080ULL * 3ULL * 3ULL);
  static_assert(sizeof(ShmMetaRegion) == 76992);
  static_assert(offsetof(ShmMetaRegion, ground_truth_history) == 6272);

  ShmHeader valid{};
  valid.magic = kShmMagic;
  valid.version = kShmVersion;
  valid.image_width = kImageWidth;
  valid.image_height = kImageHeight;
  valid.meta_size = static_cast<std::uint32_t>(kMetaSize);
  valid.sdk_abi_revision = kSdkAbiRevision;
  if (!isCompatible(valid)) {
    return 1;
  }
  valid.image_width = 1280;
  return isCompatible(valid) ? 2 : 0;
}
