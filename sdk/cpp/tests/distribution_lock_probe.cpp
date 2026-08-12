#include <daedalus_sim_sdk/talos_metadata_reader.hpp>

#include <chrono>
#include <cstdint>
#include <iostream>
#include <string>
#include <thread>

using namespace daedalus::sim::sdk::v1;

int main(int argc, char** argv) {
  if (argc != 2) {
    std::cerr << "usage: daedalus_sim_sdk_distribution_lock_probe <talos_ipc_meta>\n";
    return 2;
  }
  TalosMetadataMapping mapping;
  const auto opened = mapping.open(argv[1]);
  if (!opened) {
    std::cerr << "metadata open failed: " << opened.message << '\n';
    return 3;
  }
  const auto reader_result = mapping.reader();
  if (!reader_result) {
    std::cerr << "metadata reader failed: " << reader_result.status.message << '\n';
    return 4;
  }
  const auto reader = *reader_result.value;
  const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(10);
  while (std::chrono::steady_clock::now() < deadline) {
    const auto truth = reader.readLatestGroundTruth();
    if (truth && truth.value->frame_seq != 0 && truth.value->timestamp_ns != 0) {
      if (truth.value->target_count != 0 || truth.value->rune_count != 0) {
        std::cerr << "distribution target truth leaked: target_count="
                  << truth.value->target_count << " rune_count="
                  << truth.value->rune_count << '\n';
        return 5;
      }
      std::cout << "distribution_lock_ok frame_seq=" << truth.value->frame_seq
                << " timestamp_ns=" << truth.value->timestamp_ns
                << " target_count=0 rune_count=0\n";
      return 0;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
  }
  std::cerr << "timed out waiting for a published distribution exposure batch\n";
  return 6;
}
