#pragma once

#include <daedalus_sim_sdk/client_result.hpp>
#include <daedalus_sim_sdk/runtime_capabilities.hpp>
#include <daedalus_sim_sdk/scene_control_client.hpp>
#include <daedalus_sim_sdk/talos_metadata_reader.hpp>
#include <daedalus_sim_sdk/tcp_image_client.hpp>
#include <daedalus_sim_sdk/udp_gimbal_client.hpp>

#include <chrono>
#include <cstdint>
#include <string>

namespace daedalus::sim::sdk::v1 {

// The only maps deliberately exposed by the 1.3.1-contest package.
enum class ContestScene { ShootingRange, Energy, LargeEnergy = Energy };

struct ContestClientOptions {
  std::string ipc_directory;
  UdpEndpoint image_endpoint{"127.0.0.1", kTcpImagePort};
  UdpEndpoint command_endpoint{"127.0.0.1", kUdpCommandPort};
  SceneControlOptions scene_control{
      {"127.0.0.1", kUdpSceneControlPort}, "contest-cpp-client",
      std::chrono::milliseconds{1000}, 1};
  std::chrono::milliseconds frame_timeout{1000};
};

// An image and the actual gimbal pose that was published for that exact
// exposure.  It intentionally never supplies online target truth.
struct ContestFrame {
  TcpImageFrame image;
  GimbalState gimbal;
};

// Convenience facade for contest participants.  It owns the supported public
// transports and removes the need to compose socket, shared-memory and scene
// control primitives for the common loop.
class ContestClient {
 public:
  explicit ContestClient(ContestClientOptions options);

  [[nodiscard]] ClientStatus connect();
  void close() noexcept;
  [[nodiscard]] bool connected() const noexcept;

  [[nodiscard]] ClientResult<RuntimeCapabilities> health() const;
  [[nodiscard]] ClientResult<SceneControlResponse> selectScene(
      ContestScene scene);
  [[nodiscard]] ClientResult<SceneControlResponse> setRuneScenario(
      const RuneScenario& scenario);
  [[nodiscard]] ClientResult<ContestFrame> nextFrame(
      std::uint64_t after_source_sequence = 0) const;
  [[nodiscard]] ClientResult<std::uint64_t> sendAim(
      const UdpGimbalCommand& command) const;

  [[nodiscard]] const ContestClientOptions& options() const noexcept {
    return options_;
  }

 private:
  ContestClientOptions options_;
  TalosMetadataMapping metadata_mapping_;
  TcpImageClient image_client_;
  UdpGimbalClient gimbal_client_;
  SceneControlClient scene_client_;
  bool connected_ = false;
};

}  // namespace daedalus::sim::sdk::v1
