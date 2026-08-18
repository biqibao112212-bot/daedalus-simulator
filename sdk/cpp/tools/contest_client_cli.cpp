#include <daedalus_sim_sdk/contest_client.hpp>

#include <cstdlib>
#include <filesystem>
#include <iostream>
#include <optional>
#include <string>

using namespace daedalus::sim::sdk::v1;

namespace {

void usage() {
  std::cerr
      << "Usage: daedalus-contest-client --ipc-dir PATH <command> [args]\n"
      << "Commands:\n"
      << "  health\n"
      << "  scene shooting-range|energy|large-energy\n"
      << "  score red|blue\n"
      << "  frame\n"
      << "  truth\n"
      << "  aim YAW_DEG PITCH_DEG [--fire]\n";
}

std::optional<float> parseFloat(const char* raw) {
  char* end = nullptr;
  const float value = std::strtof(raw, &end);
  if (end == raw || *end != '\0') return std::nullopt;
  return value;
}

int failure(const ClientStatus& status) {
  std::cerr << "error=" << static_cast<int>(status.error)
            << " message=" << status.message << '\n';
  return 1;
}

}  // namespace

int main(int argc, char** argv) {
  ContestClientOptions options;
  int cursor = 1;
  while (cursor < argc && std::string(argv[cursor]) == "--ipc-dir") {
    if (++cursor >= argc) {
      usage();
      return 2;
    }
    options.ipc_directory = argv[cursor++];
  }
  if (options.ipc_directory.empty()) {
    const char* environment = std::getenv("TALOS_IPC_DIR");
    if (environment != nullptr) options.ipc_directory = environment;
  }
  if (cursor >= argc || options.ipc_directory.empty()) {
    usage();
    return 2;
  }
  const std::string command = argv[cursor++];
  ContestClient client(options);
  const auto connected = client.connect();
  if (!connected) return failure(connected);

  if (command == "health") {
    const auto health = client.health();
    if (!health) return failure(health.status);
    std::cout << "product_version=" << health.value->product_version
              << " distribution_locked=" << health.value->distribution_locked
              << " backend=" << health.value->render_backend
              << " adapter=" << health.value->adapter_name << '\n';
    return 0;
  }
  if (command == "scene" && cursor < argc) {
    const std::string value = argv[cursor++];
    const auto scene = value == "shooting-range"
                           ? ContestScene::ShootingRange
                           : (value == "energy" || value == "large-energy") ? ContestScene::Energy
                                                       : ContestScene::ShootingRange;
    if (value != "shooting-range" && value != "energy" && value != "large-energy") {
      usage();
      return 2;
    }
    const auto response = client.selectScene(scene);
    if (!response) return failure(response.status);
    std::cout << "scene=" << value
              << " applied_frame_seq=" << response.value->applied_frame_seq
              << " message=" << response.value->message << '\n';
    return 0;
  }
  if (command == "score" && cursor + 1 == argc) {
    const std::string team = argv[cursor++];
    if (team != "red" && team != "blue") {
      usage();
      return 2;
    }
    const auto score = client.getBigRuneScore(
        team == "red" ? RuneTeam::Red : RuneTeam::Blue);
    if (!score) return failure(score.status);
    std::cout << "team=" << team << " run_id=" << score.value->run_id
              << " active=" << score.value->run_active
              << " activated_arms=" << static_cast<unsigned>(score.value->activated_arms)
              << " has_hit=" << score.value->has_hit
              << " average_ring=" << score.value->average_ring
              << " last_ring=" << static_cast<unsigned>(score.value->last_ring)
              << " last_radius_mm=" << score.value->last_radius_mm
              << " last_target=" << static_cast<int>(score.value->last_target) << '\n';
    return 0;
  }
  if (command == "frame" && cursor == argc) {
    const auto frame = client.nextFrame();
    if (!frame) return failure(frame.status);
    std::cout << "source_sequence=" << frame.value->image.header.source_sequence
              << " timestamp_ns=" << frame.value->image.header.capture_timestamp_ns
              << " bytes=" << frame.value->image.payload.size()
              << " yaw_deg=" << frame.value->gimbal.yaw_deg
              << " pitch_deg=" << frame.value->gimbal.pitch_deg << '\n';
    return 0;
  }
  if (command == "truth" && cursor == argc) {
    const auto frame = client.nextFrame();
    if (!frame) return failure(frame.status);
    TalosMetadataMapping mapping;
    const auto mapped = mapping.open(
        (std::filesystem::path(options.ipc_directory) / kMetaFileName).string());
    if (!mapped) return failure(mapped);
    const auto reader = mapping.reader();
    if (!reader) return failure(reader.status);
    const auto truth = reader.value->readGroundTruthForFrame(
        frame.value->image.header.source_sequence);
    if (!truth) return failure(truth.status);
    const auto& image = frame.value->image.header;
    if (truth.value->producer_epoch != image.producer_epoch ||
        truth.value->ground_truth.timestamp_ns != image.capture_timestamp_ns ||
        truth.value->exposure_state.timestamp_ns != image.capture_timestamp_ns) {
      return failure(ClientStatus::failure(
          ClientError::ProtocolError, "image and truth exposure identities differ"));
    }
    std::cout << "producer_epoch=" << truth.value->producer_epoch
              << " source_sequence=" << image.source_sequence
              << " timestamp_ns=" << image.capture_timestamp_ns
              << " target_count=" << truth.value->ground_truth.target_count
              << " rune_count=" << truth.value->ground_truth.rune_count << '\n';
    return 0;
  }
  if (command == "aim" && cursor + 1 < argc) {
    const auto yaw = parseFloat(argv[cursor++]);
    const auto pitch = parseFloat(argv[cursor++]);
    if (!yaw || !pitch) {
      usage();
      return 2;
    }
    UdpGimbalCommand aim;
    aim.yaw_deg = *yaw;
    aim.pitch_deg = *pitch;
    if (cursor < argc && std::string(argv[cursor++]) == "--fire") aim.fire_advice = true;
    if (cursor != argc) {
      usage();
      return 2;
    }
    const auto sent = client.sendAim(aim);
    if (!sent) return failure(sent.status);
    std::cout << "command_id=" << *sent.value << '\n';
    return 0;
  }
  usage();
  return 2;
}
