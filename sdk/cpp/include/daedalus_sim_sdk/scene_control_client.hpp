#pragma once

#include <daedalus_sim_sdk/client_result.hpp>
#include <daedalus_sim_sdk/endpoints_v1.hpp>

#include <chrono>
#include <atomic>
#include <cstdint>
#include <string>
#include <vector>

namespace daedalus::sim::sdk::v1 {

inline constexpr const char* kSceneControlProtocol =
    "daedalus.scene-control/2";

enum class SceneControlStatus {
  Ok,
  InvalidRequest,
  Unsupported,
  NotReady,
  InternalError,
};

struct SceneControlResponse {
  std::string protocol;
  std::uint64_t command_id = 0;
  std::string session_id;
  SceneControlStatus status = SceneControlStatus::InternalError;
  std::uint64_t applied_frame_seq = 0;
  std::uint64_t timestamp_ns = 0;
  std::string message;
};

struct SceneControlOptions {
  UdpEndpoint endpoint{"127.0.0.1", kUdpSceneControlPort};
  std::string session_id;
  std::chrono::milliseconds timeout{1000};
  std::uint64_t first_command_id = 1;
};

enum class SceneMode { Armor, Energy, Outpost, ShootingRange };
enum class RangeMotionMode { Stationary, Linear, Spin, LinearAndSpin };
enum class RuneMode { Off, Small, Large };

struct RangeTargetMotion {
  std::uint8_t target = 3;
  RangeMotionMode mode = RangeMotionMode::Stationary;
  float direction_deg = 90.0F;
  float linear_speed_mps = 0.0F;
  float linear_span_m = 0.0F;
  float spin_deg_s = 0.0F;
};

struct RangeTargetGeometry {
  std::uint8_t target = 3;
  float radial_scale = 1.0F;
};

struct RuneState {
  RuneMode mode = RuneMode::Off;
  std::vector<std::uint8_t> pending_targets;
  std::vector<std::uint8_t> activated_targets;
};

[[nodiscard]] ClientResult<std::string> encodeSetSceneArgs(SceneMode mode);
[[nodiscard]] ClientResult<std::string> encodeRangeTargetMotionArgs(
    const RangeTargetMotion& motion);
[[nodiscard]] ClientResult<std::string> encodeRangeTargetGeometryArgs(
    const RangeTargetGeometry& geometry);
[[nodiscard]] ClientResult<std::string> encodeRuneStateArgs(
    const RuneState& state);

[[nodiscard]] ClientResult<std::string> buildSceneControlRequest(
    std::uint64_t command_id, const std::string& session_id,
    const std::string& op, const std::string& args_json = "{}");

[[nodiscard]] ClientResult<SceneControlResponse> parseSceneControlResponse(
    const std::string& json, std::uint64_t expected_command_id,
    const std::string& expected_session_id);

class SceneControlClient {
 public:
  explicit SceneControlClient(SceneControlOptions options);

  [[nodiscard]] ClientResult<SceneControlResponse> request(
      const std::string& op, const std::string& args_json = "{}");
  [[nodiscard]] ClientResult<SceneControlResponse> ping(
      const std::string& args_json = "{}");
  [[nodiscard]] ClientResult<SceneControlResponse> createSession(
      const std::string& args_json = "{}");
  [[nodiscard]] ClientResult<SceneControlResponse> status(
      const std::string& args_json = "{}");
  [[nodiscard]] ClientResult<SceneControlResponse> resetScene(
      const std::string& args_json = "{}");
  [[nodiscard]] ClientResult<SceneControlResponse> setScene(
      const std::string& args_json);
  [[nodiscard]] ClientResult<SceneControlResponse> setScene(SceneMode mode);
  [[nodiscard]] ClientResult<SceneControlResponse> setRangeTargetMotion(
      const std::string& args_json);
  [[nodiscard]] ClientResult<SceneControlResponse> setRangeTargetMotion(
      const RangeTargetMotion& motion);
  [[nodiscard]] ClientResult<SceneControlResponse> setRangeTargetGeometry(
      const std::string& args_json);
  [[nodiscard]] ClientResult<SceneControlResponse> setRangeTargetGeometry(
      const RangeTargetGeometry& geometry);
  [[nodiscard]] ClientResult<SceneControlResponse> setRuneState(
      const std::string& args_json);
  [[nodiscard]] ClientResult<SceneControlResponse> setRuneState(
      const RuneState& state);

  [[nodiscard]] const SceneControlOptions& options() const noexcept {
    return options_;
  }

 private:
  SceneControlOptions options_;
  std::atomic<std::uint64_t> next_command_id_;
};

}  // namespace daedalus::sim::sdk::v1
