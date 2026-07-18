#include <daedalus_sim_sdk/scene_control_client.hpp>

#include <string>

using namespace daedalus::sim::sdk::v1;

int main() {
  const auto request = buildSceneControlRequest(
      42, "session-\"a", "set_scene", "{\"scene\":\"range\"}");
  if (!request || request.value->find("daedalus.scene-control/1") ==
                      std::string::npos ||
      request.value->find("session-\\\"a") == std::string::npos ||
      request.value->find("\"op\":\"set_scene\"") == std::string::npos) {
    return 1;
  }
  if (buildSceneControlRequest(1, "s", "ping", "[]")) return 2;

  const std::string response =
      "{\"protocol\":\"daedalus.scene-control/1\",\"command_id\":42,"
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
  RuneState rune{};
  rune.mode = RuneMode::Large;
  rune.pending_targets = {2};
  rune.activated_targets = {0, 4};
  if (!encodeRuneStateArgs(rune)) return 8;
  return 0;
}
