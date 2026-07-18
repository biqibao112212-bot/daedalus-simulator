#include <daedalus_sim_sdk/udp_gimbal_client.hpp>

#include <cmath>
#include <limits>
#include <string>

using namespace daedalus::sim::sdk::v1;

int main() {
  UdpGimbalCommand command{};
  command.yaw_deg = 12.5F;
  command.pitch_deg = 91.0F;
  command.distance_m = 3.25F;
  command.fire_advice = true;
  const auto json = encodeUdpGimbalCommand(command);
  if (!json || json.value->find("\"yaw_deg\":12.5") == std::string::npos ||
      json.value->find("\"pitch_deg\":91") == std::string::npos ||
      json.value->find("\"distance_m\":3.25") == std::string::npos ||
      json.value->find("\"fire_advice\":true") == std::string::npos) {
    return 1;
  }
  command.yaw_deg = std::numeric_limits<float>::infinity();
  return encodeUdpGimbalCommand(command) ? 2 : 0;
}
