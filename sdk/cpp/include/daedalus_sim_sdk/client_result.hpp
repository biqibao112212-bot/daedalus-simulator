#pragma once

#include <optional>
#include <cstdint>
#include <string>
#include <utility>

namespace daedalus::sim::sdk::v1 {

enum class ClientError {
  None,
  InvalidArgument,
  SocketStartupFailed,
  ResolveFailed,
  SocketCreateFailed,
  ConnectFailed,
  SendFailed,
  ReceiveFailed,
  NotReady,
  Timeout,
  PeerClosed,
  ProtocolError,
  PayloadTooLarge,
  IncompatibleMetadata,
  UnstableSnapshot,
};

struct ClientStatus {
  ClientError error = ClientError::None;
  std::string message;

  [[nodiscard]] bool ok() const noexcept { return error == ClientError::None; }
  explicit operator bool() const noexcept { return ok(); }

  static ClientStatus success() { return {}; }
  static ClientStatus failure(ClientError code, std::string detail) {
    return {code, std::move(detail)};
  }
};

template <typename T> struct ClientResult {
  std::optional<T> value;
  ClientStatus status;

  [[nodiscard]] bool ok() const noexcept {
    return status.ok() && value.has_value();
  }
  explicit operator bool() const noexcept { return ok(); }

  static ClientResult success(T result) {
    return {std::move(result), ClientStatus::success()};
  }
  static ClientResult failure(ClientError code, std::string detail) {
    return {std::nullopt, ClientStatus::failure(code, std::move(detail))};
  }
};

struct UdpEndpoint {
  std::string host = "127.0.0.1";
  std::uint16_t port = 0;
};

}  // namespace daedalus::sim::sdk::v1
