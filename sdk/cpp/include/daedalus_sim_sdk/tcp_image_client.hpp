#pragma once

#include <daedalus_sim_sdk/client_result.hpp>
#include <daedalus_sim_sdk/endpoints_v1.hpp>
#include <daedalus_sim_sdk/tcp_image_v1.hpp>

#include <chrono>
#include <cstdint>
#include <memory>
#include <vector>

namespace daedalus::sim::sdk::v1 {

struct TcpImageFrame {
  tcp_image::FrameHeader header;
  std::vector<std::uint8_t> payload;
};

// A connected client owns a receiver thread. Complete, validated frames replace
// the previous frame atomically, so a slow consumer observes only the latest
// complete frame and never a partially received payload.
class TcpImageClient {
 public:
  explicit TcpImageClient(
      UdpEndpoint endpoint = {"127.0.0.1", kTcpImagePort});
  ~TcpImageClient();

  TcpImageClient(const TcpImageClient&) = delete;
  TcpImageClient& operator=(const TcpImageClient&) = delete;
  TcpImageClient(TcpImageClient&&) noexcept;
  TcpImageClient& operator=(TcpImageClient&&) noexcept;

  [[nodiscard]] ClientStatus connect();
  void close() noexcept;
  [[nodiscard]] bool connected() const noexcept;

  [[nodiscard]] ClientResult<TcpImageFrame> latest() const;
  [[nodiscard]] ClientResult<TcpImageFrame> waitForLatest(
      std::uint64_t after_source_sequence,
      std::chrono::milliseconds timeout) const;

 private:
  class Impl;
  std::unique_ptr<Impl> impl_;
};

}  // namespace daedalus::sim::sdk::v1
