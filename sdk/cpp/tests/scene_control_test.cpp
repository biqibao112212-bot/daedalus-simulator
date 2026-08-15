#include <daedalus_sim_sdk/scene_control_client.hpp>

#include <string>

using namespace daedalus::sim::sdk::v1;

int main() {
  const auto request = buildSceneControlRequest(
      42, "session-\"a", "set_scene", "{\"scene\":\"range\"}");
  if (!request || request.value->find("daedalus.scene-control/2") ==
                      std::string::npos ||
      request.value->find("session-\\\"a") == std::string::npos ||
      request.value->find("\"op\":\"set_scene\"") == std::string::npos) {
    return 1;
  }
  if (buildSceneControlRequest(1, "s", "ping", "[]")) return 2;

  const std::string response =
      "{\"protocol\":\"daedalus.scene-control/2\",\"command_id\":42,"
      "\"session_id\":\"session-a\",\"status\":\"ok\","
      "\"applied_frame_seq\":99,\"timestamp_ns\":123456,"
      "\"message\":\"ready\"}";
  const auto parsed = parseSceneControlResponse(response, 42, "session-a");
  if (!parsed || parsed.value->status != SceneControlStatus::Ok ||
      parsed.value->applied_frame_seq != 99 ||
      parsed.value->timestamp_ns != 123456 ||
      parsed.value->message != "ready") {
    return 3;
  }
  if (parseSceneControlResponse(response, 41, "session-a")) return 4;

  SceneControlOptions options{};
  options.session_id = "session-a";
  SceneControlClient client(options);
  if (client.options().endpoint.host != "127.0.0.1" ||
      client.options().endpoint.port != 5603) {
    return 5;
  }
  const auto scene = encodeSetSceneArgs(SceneMode::ShootingRange);
  if (!scene || scene.value->find("shooting_range") == std::string::npos) return 6;
  RangeTargetMotion motion{};
  motion.target = 3;
  motion.mode = RangeMotionMode::LinearAndSpin;
  motion.linear_speed_mps = 1.5F;
  const auto range = encodeRangeTargetMotionArgs(motion);
  if (!range || range.value->find("linear_and_spin") == std::string::npos) return 7;
  RangeTargetGeometry geometry{};
  geometry.target = 3;
  geometry.radial_scale = 1.2F;
  const auto geometry_args = encodeRangeTargetGeometryArgs(geometry);
  if (!geometry_args || geometry_args.value->find("1.2") == std::string::npos) {
    return 8;
  }
  RuneState rune{};
  rune.mode = RuneMode::Large;
  rune.pending_targets = {2};
  rune.activated_targets = {0, 4};
  if (!encodeRuneStateArgs(rune)) return 9;
  RuneScenario rule{};
  rule.mode = RuneMode::Large;
  rule.motion = RuneMotion::RuleDriven;
  if (!encodeRuneScenarioArgs(rule)) return 10;
  rule.leaf_states = {RuneLeafState::Activating};
  if (encodeRuneScenarioArgs(rule)) return 11;
  RuneScenario frozen{};
  frozen.mode = RuneMode::Small;
  frozen.motion = RuneMotion::Static;
  frozen.red_face_direction = RuneDirection::CounterClockwise;
  frozen.leaf_states = {
      RuneLeafState::Activating, RuneLeafState::Activated,
      RuneLeafState::Completed, RuneLeafState::Deactivated,
      RuneLeafState::Deactivated};
  const auto frozen_args = encodeRuneScenarioArgs(frozen);
  if (!frozen_args || frozen_args.value->find("static") == std::string::npos ||
      frozen_args.value->find("completed") == std::string::npos) return 12;
  return 0;
}
