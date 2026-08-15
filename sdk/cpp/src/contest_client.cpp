#include <daedalus_sim_sdk/contest_client.hpp>

#include <filesystem>
#include <utility>

namespace daedalus::sim::sdk::v1 {

ContestClient::ContestClient(ContestClientOptions options)
    : options_(std::move(options)),
      image_client_(options_.image_endpoint),
      gimbal_client_(options_.command_endpoint),
      scene_client_(options_.scene_control) {}

ClientStatus ContestClient::connect() {
  if (connected_) return ClientStatus::success();
  if (options_.ipc_directory.empty()) {
    return ClientStatus::failure(ClientError::InvalidArgument,
                                 "IPC directory must not be empty");
  }
  const auto metadata_path =
      (std::filesystem::path(options_.ipc_directory) /
       std::string(kMetaFileName))
          .string();
  auto status = metadata_mapping_.open(metadata_path);
  if (!status) return status;
  status = image_client_.connect();
  if (!status) {
    metadata_mapping_.close();
    return status;
  }
  const auto session = scene_client_.createSession();
  if (!session) {
    image_client_.close();
    metadata_mapping_.close();
    return session.status;
  }
  connected_ = true;
  return ClientStatus::success();
}

void ContestClient::close() noexcept {
  image_client_.close();
  metadata_mapping_.close();
  connected_ = false;
}

bool ContestClient::connected() const noexcept { return connected_; }

ClientResult<RuntimeCapabilities> ContestClient::health() const {
  if (!connected_) {
    return ClientResult<RuntimeCapabilities>::failure(
        ClientError::NotReady, "connect() must succeed before health()");
  }
  return readRuntimeCapabilities(options_.ipc_directory);
}

ClientResult<SceneControlResponse> ContestClient::selectScene(
    ContestScene scene) {
  if (!connected_) {
    return ClientResult<SceneControlResponse>::failure(
        ClientError::NotReady, "connect() must succeed before selectScene()");
  }
  const auto selected = scene_client_.setScene(
      scene == ContestScene::ShootingRange ? SceneMode::ShootingRange
                                           : SceneMode::Energy);
  if (!selected) return selected;
  // Selecting the Energy map creates a live large-rune cycle in the simulator.
  // Do not send an empty RuneState here: explicit scene-control target lists
  // are frozen inspection snapshots, so they suppress hit-driven rounds and
  // the mandatory timeout reset.
  return selected;
}

ClientResult<SceneControlResponse> ContestClient::setRuneScenario(
    const RuneScenario& scenario) {
  if (!connected_) {
    return ClientResult<SceneControlResponse>::failure(
        ClientError::NotReady, "connect() must succeed before setRuneScenario()");
  }
  return scene_client_.setRuneScenario(scenario);
}

ClientResult<BigRuneScore> ContestClient::getBigRuneScore(RuneTeam team) {
  if (!connected_) {
    return ClientResult<BigRuneScore>::failure(
        ClientError::NotReady, "connect() must succeed before getBigRuneScore()");
  }
  return scene_client_.getBigRuneScore(team);
}

ClientResult<ContestFrame> ContestClient::nextFrame(
    std::uint64_t after_source_sequence) const {
  if (!connected_) {
    return ClientResult<ContestFrame>::failure(
        ClientError::NotReady, "connect() must succeed before nextFrame()");
  }
  const auto image = image_client_.waitForLatest(after_source_sequence,
                                                  options_.frame_timeout);
  if (!image) return ClientResult<ContestFrame>::failure(image.status.error,
                                                          image.status.message);
  const auto reader = metadata_mapping_.reader();
  if (!reader) return ClientResult<ContestFrame>::failure(reader.status.error,
                                                           reader.status.message);
  const auto gimbal = reader.value->readGimbalStateForFrame(
      image.value->header.source_sequence);
  if (!gimbal) return ClientResult<ContestFrame>::failure(gimbal.status.error,
                                                           gimbal.status.message);
  return ClientResult<ContestFrame>::success({*image.value, *gimbal.value});
}

ClientResult<std::uint64_t> ContestClient::sendAim(
    const UdpGimbalCommand& command) const {
  if (!connected_) {
    return ClientResult<std::uint64_t>::failure(
        ClientError::NotReady, "connect() must succeed before sendAim()");
  }
  return gimbal_client_.sendTracked(command);
}

}  // namespace daedalus::sim::sdk::v1
