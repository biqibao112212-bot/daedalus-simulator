#pragma once

#include <daedalus_sim_sdk/client_result.hpp>
#include <daedalus_sim_sdk/endpoints_v1.hpp>

#include <chrono>
#include <array>
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
  std::string data_json;
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
enum class RuneMotion { RuleDriven, Static };
enum class RuneDirection { Clockwise, CounterClockwise };
enum class RuneLeafState { Deactivated, Activating, Activated, Completed };
enum class RuneTeam { Red, Blue };

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

// Bounded energy-mechanism scenario for annotation and algorithm evaluation.
// RuleDriven uses the official small/large speed model and has no caller-set
// leaf states. Static stops rotation and accepts exactly five enumerated leaf
// appearances; it never accepts arbitrary colors, meshes, or target truth.
struct RuneScenario {
  RuneMode mode = RuneMode::Large;
  RuneMotion motion = RuneMotion::RuleDriven;
  RuneDirection red_face_direction = RuneDirection::Clockwise;
  std::vector<RuneLeafState> leaf_states;
};

// Read-only live score for one side's current or most recently completed
// large-rune activation. Ring 10 is centre and ring 1 is the outer ring.
struct BigRuneScore {
  RuneTeam team = RuneTeam::Red;
  std::uint64_t run_id = 0;
  bool run_active = false;
  std::uint8_t activated_arms = 0;
  bool has_hit = false;
  float average_ring = 0.0F;
  std::uint8_t last_ring = 0;
  std::uint16_t last_radius_mm = 0;
  std::int8_t last_target = -1;
};

[[nodiscard]] ClientResult<std::string> encodeSetSceneArgs(SceneMode mode);
[[nodiscard]] ClientResult<std::string> encodeRangeTargetMotionArgs(
    const RangeTargetMotion& motion);
[[nodiscard]] ClientResult<std::string> encodeRangeTargetGeometryArgs(
    const RangeTargetGeometry& geometry);
[[nodiscard]] ClientResult<std::string> encodeRuneStateArgs(
    const RuneState& state);
[[nodiscard]] ClientResult<std::string> encodeRuneScenarioArgs(
    const RuneScenario& scenario);

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
  [[nodiscard]] ClientResult<SceneControlResponse> setRuneScenario(
      const RuneScenario& scenario);
  [[nodiscard]] ClientResult<BigRuneScore> getBigRuneScore(RuneTeam team);

  [[nodiscard]] const SceneControlOptions& options() const noexcept {
    return options_;
  }

 private:
  SceneControlOptions options_;
  std::atomic<std::uint64_t> next_command_id_;
};

}  // namespace daedalus::sim::sdk::v1
