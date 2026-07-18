#pragma once

#include <daedalus_sim_sdk/client_result.hpp>
#include <daedalus_sim_sdk/endpoints_v1.hpp>

#include <optional>
#include <string>

namespace daedalus::sim::sdk::v1 {

struct UdpGimbalCommand {
  std::optional<float> yaw_deg;
  std::optional<float> pitch_deg;
  std::optional<float> distance_m;
  bool fire_advice = false;
};

[[nodiscard]] ClientResult<std::string> encodeUdpGimbalCommand(
    const UdpGimbalCommand& command);

class UdpGimbalClient {
 public:
  explicit UdpGimbalClient(
      UdpEndpoint endpoint = {"127.0.0.1", kUdpCommandPort});

  [[nodiscard]] ClientStatus send(const UdpGimbalCommand& command) const;

 private:
  UdpEndpoint endpoint_;
};

}  // namespace daedalus::sim::sdk::v1
