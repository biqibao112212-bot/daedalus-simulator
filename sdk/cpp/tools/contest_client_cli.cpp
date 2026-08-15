#include <daedalus_sim_sdk/contest_client.hpp>

#include <cstdlib>
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
      << "  scene shooting-range|large-energy\n"
      << "  frame\n"
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
                           : value == "large-energy" ? ContestScene::LargeEnergy
                                                       : ContestScene::ShootingRange;
    if (value != "shooting-range" && value != "large-energy") {
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
