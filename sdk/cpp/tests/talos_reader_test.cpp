#include <daedalus_sim_sdk/talos_metadata_reader.hpp>

#include <cstdio>
#include <fstream>

using namespace daedalus::sim::sdk::v1;

int main() {
  ShmMetaRegion region{};
  TalosMetadataReader invalid(&region, sizeof(region));
  if (invalid.compatibility() != TalosCompatibility::InvalidMagic) return 1;

  region.header.magic = kShmMagic;
  region.header.version = kShmVersion;
  region.header.image_width = kImageWidth;
  region.header.image_height = kImageHeight;
  region.header.meta_size = static_cast<std::uint32_t>(kMetaSize);
  region.header.sdk_abi_revision = kSdkAbiRevision;
  region.header.created_ns = 123;
  region.camera_info.timestamp_ns = 1;
  region.camera_info.width = kImageWidth;
  region.camera_info.height = kImageHeight;
  region.runtime_state.timestamp_ns = 17;
  region.runtime_state.frame_seq = 13;
  region.runtime_state.last_applied_command_id = 19;
  region.runtime_state.gimbal_yaw_rad = 0.5F;
  region.runtime_state.gimbal_pitch_rad = 0.0F;
  region.image.read_idx = 0;
  region.image.slots[0].seq = 7;
  region.image.slots[0].timestamp_ns = 11;
  region.image.slots[0].width = kImageWidth;
  region.image.slots[0].height = kImageHeight;
  region.ground_truth_history.slots[0].commit_seq = 2;
  region.ground_truth_history.slots[0].ground_truth.frame_seq = 21;
  region.ground_truth_history.slots[0].ground_truth.timestamp_ns = 22;
  region.ground_truth_history.slots[0].exposure_state.frame_seq = 21;
  region.ground_truth_history.slots[0].exposure_state.timestamp_ns = 22;
  region.ground_truth_history.slots[0].exposure_state.gimbal_yaw_rad = 0.25F;
  region.ground_truth_history.slots[0].exposure_state.gimbal_pitch_rad = 0.0F;

  TalosMetadataReader reader(&region, sizeof(region));
  if (reader.compatibility() != TalosCompatibility::Compatible) return 2;
  const auto truth = reader.readGroundTruthForFrame(21);
  if (!truth) return 8;
  if (truth.value->producer_epoch != region.header.created_ns) return 9;
  if (truth.value->ground_truth.timestamp_ns !=
      truth.value->exposure_state.timestamp_ns) return 10;
  region.ground_truth_history.slots[1].commit_seq = 4;
  region.ground_truth_history.slots[1].ground_truth.frame_seq = 22;
  region.ground_truth_history.slots[1].ground_truth.timestamp_ns = 23;
  region.ground_truth_history.slots[1].exposure_state.frame_seq = 22;
  region.ground_truth_history.slots[1].exposure_state.timestamp_ns = 24;
  if (reader.readGroundTruthForFrame(22)) return 11;
  const auto header = reader.readHeader();
  if (!header || header.value->meta_size != kMetaSize) return 3;
  const auto image = reader.readLatestImageMeta();
  if (!image || image.value->seq != 7 || image.value->timestamp_ns != 11) {
    return 4;
  }
  const auto camera = reader.readCameraInfo();
  if (!camera || camera.value->width != kImageWidth) return 5;
  if (reader.readLatestPose(5)) return 6;
  const auto gimbal = reader.readGimbalState();
  if (!gimbal || gimbal.value->timestamp_ns != 17 ||
      gimbal.value->frame_seq != 13 ||
      gimbal.value->last_applied_command_id != 19 ||
      gimbal.value->pitch_deg != 90.0F) return 9;
  const auto exposure_gimbal = reader.readGimbalStateForFrame(21);
  if (!exposure_gimbal || exposure_gimbal.value->frame_seq != 21 ||
      exposure_gimbal.value->timestamp_ns != 22 ||
      exposure_gimbal.value->pitch_deg != 90.0F) return 10;

  const char* path = "daedalus_sdk_test_meta.bin";
  {
    std::ofstream output(path, std::ios::binary | std::ios::trunc);
    output.write(reinterpret_cast<const char*>(&region), sizeof(region));
  }
  TalosMetadataMapping mapping;
  if (!mapping.open(path) || !mapping.isOpen()) return 7;
  const auto mapped_reader = mapping.reader();
  if (!mapped_reader || !mapped_reader.value->readLatestImageMeta()) return 8;
  mapping.close();
  std::remove(path);
  return 0;
}
