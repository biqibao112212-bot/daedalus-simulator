#pragma once

#include <daedalus_sim_sdk/client_result.hpp>
#include <daedalus_sim_sdk/endpoints_v1.hpp>

#include <optional>
#include <cstdint>
#include <string>

namespace daedalus::sim::sdk::v1 {

struct UdpGimbalCommand {
  // Zero asks UdpGimbalClient::send to allocate a process-local monotonic id.
  std::uint64_t command_id = 0;
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
  // Returns the explicit or SDK-allocated command id after the datagram has
  // been handed to the operating system.
  [[nodiscard]] ClientResult<std::uint64_t> sendTracked(
      const UdpGimbalCommand& command) const;

 private:
  UdpEndpoint endpoint_;
};

}  // namespace daedalus::sim::sdk::v1
