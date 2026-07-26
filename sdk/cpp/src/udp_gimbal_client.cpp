#include <daedalus_sim_sdk/udp_gimbal_client.hpp>

#include "socket_platform.hpp"

#include <atomic>
#include <cmath>
#include <iomanip>
#include <sstream>

namespace daedalus::sim::sdk::v1 {

namespace {
std::atomic<std::uint64_t> g_next_command_id{1};
}

ClientResult<std::string> encodeUdpGimbalCommand(
    const UdpGimbalCommand& command) {
  const auto finite = [](const std::optional<float>& value) {
    return !value || std::isfinite(*value);
  };
  if (!finite(command.yaw_deg) || !finite(command.pitch_deg) ||
      !finite(command.distance_m)) {
    return ClientResult<std::string>::failure(
        ClientError::InvalidArgument, "gimbal values must be finite");
  }
  std::ostringstream json;
  json << std::setprecision(9) << '{';
  json << "\"command_id\":" << command.command_id;
  bool comma = true;
  const auto append = [&](const char* name, const std::optional<float>& value) {
    if (!value) return;
    if (comma) json << ',';
    json << '"' << name << "\":" << *value;
    comma = true;
  };
  append("yaw_deg", command.yaw_deg);
  append("pitch_deg", command.pitch_deg);
  append("distance_m", command.distance_m);
  if (comma) json << ',';
  json << "\"fire_advice\":" << (command.fire_advice ? "true" : "false")
       << '}';
  return ClientResult<std::string>::success(json.str());
}

UdpGimbalClient::UdpGimbalClient(UdpEndpoint endpoint)
    : endpoint_(std::move(endpoint)) {}

ClientStatus UdpGimbalClient::send(const UdpGimbalCommand& command) const {
  return sendTracked(command).status;
}

ClientResult<std::uint64_t> UdpGimbalClient::sendTracked(
    const UdpGimbalCommand& command) const {
  auto outgoing = command;
  if (outgoing.command_id == 0) {
    outgoing.command_id =
        g_next_command_id.fetch_add(1, std::memory_order_relaxed);
  }
  auto payload = encodeUdpGimbalCommand(outgoing);
  if (!payload) {
    return ClientResult<std::uint64_t>::failure(payload.status.error,
                                                 payload.status.message);
  }
  auto address = detail::resolveIpv4(endpoint_, SOCK_DGRAM, IPPROTO_UDP);
  if (!address) {
    return ClientResult<std::uint64_t>::failure(address.status.error,
                                                 address.status.message);
  }
  detail::SocketHandle socket = ::socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
  if (socket == detail::kInvalidSocket) {
    return ClientResult<std::uint64_t>::failure(
        ClientError::SocketCreateFailed, detail::socketErrorMessage("socket"));
  }
  const int sent = ::sendto(
      socket, payload.value->data(), static_cast<int>(payload.value->size()), 0,
      reinterpret_cast<const sockaddr*>(&address.value->storage),
      address.value->length);
  const auto message = sent == static_cast<int>(payload.value->size())
                           ? std::string{}
                           : detail::socketErrorMessage("sendto");
  detail::closeSocket(socket);
  if (!message.empty()) {
    return ClientResult<std::uint64_t>::failure(ClientError::SendFailed,
                                                 message);
  }
  return ClientResult<std::uint64_t>::success(outgoing.command_id);
}

}  // namespace daedalus::sim::sdk::v1
